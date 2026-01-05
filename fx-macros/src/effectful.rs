//! Implementation of the `#[effectful]` attribute macro.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    FnArg, ItemFn, Pat, Result, Token, Type,
};

/// The capability groups specified in the attribute.
struct EffectfulAttr {
    capabilities: Punctuated<Type, Token![,]>,
}

impl Parse for EffectfulAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        let capabilities = Punctuated::parse_terminated(input)?;
        Ok(EffectfulAttr { capabilities })
    }
}

/// Build Provider bounds with exact output type.
/// For Counter, generates:
///   __FxP: fx::Provider<
///       <Counter as fx::CapabilityGroup>::Capabilities,
///       Output = <Counter as fx::OutputVariant>::ExpectedOutput
///   >
///
/// By requiring the exact Output type, Extract bounds are automatically satisfied
/// since we know the concrete output variant type.
///
/// This allows users to satisfy bounds by implementing Provider<Capabilities> directly,
/// similar to how effing-mad handlers work.
fn build_handles_bounds(caps: &Punctuated<Type, Token![,]>) -> TokenStream2 {
    if caps.is_empty() {
        return quote! {};
    }

    let bounds: Vec<TokenStream2> = caps
        .iter()
        .map(|ty| {
            // Require Provider with exact output type
            // This ensures all Extract bounds are satisfied since we know the concrete type
            quote! {
                __FxP: fx::Provider<
                    <#ty as fx::CapabilityGroup>::Capabilities,
                    Output = <#ty as fx::OutputVariant>::ExpectedOutput
                >
            }
        })
        .collect();

    quote! { #(#bounds),* }
}

/// Build the combined capabilities type from ability types.
/// For single ability: <Counter as fx::CapabilityGroup>::Capabilities
/// For multiple: flattened variant of all capabilities
fn build_capabilities_type(caps: &Punctuated<Type, Token![,]>) -> TokenStream2 {
    if caps.is_empty() {
        return quote! { fx::Never };
    }

    if caps.len() == 1 {
        let cap = caps.first().unwrap();
        return quote! { <#cap as fx::CapabilityGroup>::Capabilities };
    }

    // For multiple capabilities, build a Variant and flatten it
    let cap_types: Vec<_> = caps.iter().collect();
    let mut result = quote! { fx::Never };
    for cap in cap_types.iter().rev() {
        result = quote! { fx::Variant<#cap, #result> };
    }
    quote! { <#result as fx::FlattenGroups>::Flattened }
}

/// Extract argument name and type from FnArg
fn extract_arg_info(arg: &FnArg) -> Option<(Ident, Type)> {
    match arg {
        FnArg::Typed(pat_type) => {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                Some((pat_ident.ident.clone(), (*pat_type.ty).clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn effectful_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as EffectfulAttr);
    let func = parse_macro_input!(item as ItemFn);

    let vis = &func.vis;
    let fn_name = &func.sig.ident;
    let fn_generics = &func.sig.generics;

    let return_type = match &func.sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    let fn_args = &func.sig.inputs;
    let body = &func.block;

    let existing_params = &fn_generics.params;
    let existing_where = &fn_generics.where_clause;
    let (_, type_generics, _) = fn_generics.split_for_impl();

    // Build abstract Handles<P> bounds (e.g., Counter: fx::Handles<__FxP>)
    let handles_bounds = build_handles_bounds(&attr.capabilities);

    // Build combined capabilities type for perform! to use with PerformIn
    let capabilities_type = build_capabilities_type(&attr.capabilities);

    // Generate struct name from function name (PascalCase + Effect)
    let struct_name = Ident::new(
        &format!(
            "{}Effect",
            fn_name
                .to_string()
                .split('_')
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<String>()
        ),
        Span::call_site(),
    );

    // Extract argument names and types
    let args: Vec<_> = fn_args.iter().filter_map(extract_arg_info).collect();
    let arg_names: Vec<_> = args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = args.iter().map(|(_, ty)| ty).collect();

    // Generate the effect struct and implementation
    // The body is wrapped with __FxCaps type alias for perform! macro
    let output: TokenStream2 = if args.is_empty() {
        // No arguments - unit struct
        if existing_params.is_empty() {
            quote! {
                #vis struct #struct_name;

                impl #struct_name {
                    /// Execute this effect with the given provider.
                    pub async fn perform<__FxP>(self, __fx_provider: &mut __FxP) -> #return_type
                    where
                        __FxP: Send,
                        #handles_bounds,
                        #existing_where
                    {
                        #[allow(dead_code)]
                        type __FxCaps = #capabilities_type;
                        #body
                    }
                }

                #vis fn #fn_name() -> #struct_name {
                    #struct_name
                }
            }
        } else {
            quote! {
                #vis struct #struct_name #fn_generics #existing_where {
                    _marker: ::core::marker::PhantomData<(#existing_params)>,
                }

                impl #fn_generics #struct_name #type_generics #existing_where {
                    /// Execute this effect with the given provider.
                    pub async fn perform<__FxP>(self, __fx_provider: &mut __FxP) -> #return_type
                    where
                        __FxP: Send,
                        #handles_bounds,
                    {
                        #[allow(dead_code)]
                        type __FxCaps = #capabilities_type;
                        #body
                    }
                }

                #vis fn #fn_name #fn_generics () -> #struct_name #type_generics #existing_where {
                    #struct_name { _marker: ::core::marker::PhantomData }
                }
            }
        }
    } else {
        // Has arguments - struct with fields
        if existing_params.is_empty() {
            quote! {
                #vis struct #struct_name {
                    #(#arg_names: #arg_types),*
                }

                impl #struct_name {
                    /// Execute this effect with the given provider.
                    pub async fn perform<__FxP>(self, __fx_provider: &mut __FxP) -> #return_type
                    where
                        __FxP: Send,
                        #handles_bounds,
                        #existing_where
                    {
                        let Self { #(#arg_names),* } = self;
                        #[allow(dead_code)]
                        type __FxCaps = #capabilities_type;
                        #body
                    }
                }

                #vis fn #fn_name(#(#arg_names: #arg_types),*) -> #struct_name {
                    #struct_name { #(#arg_names),* }
                }
            }
        } else {
            quote! {
                #vis struct #struct_name #fn_generics #existing_where {
                    #(#arg_names: #arg_types),*,
                    _marker: ::core::marker::PhantomData<(#existing_params)>,
                }

                impl #fn_generics #struct_name #type_generics #existing_where {
                    /// Execute this effect with the given provider.
                    pub async fn perform<__FxP>(self, __fx_provider: &mut __FxP) -> #return_type
                    where
                        __FxP: Send,
                        #handles_bounds,
                    {
                        let Self { #(#arg_names),*, _marker } = self;
                        #[allow(dead_code)]
                        type __FxCaps = #capabilities_type;
                        #body
                    }
                }

                #vis fn #fn_name #fn_generics (#(#arg_names: #arg_types),*) -> #struct_name #type_generics #existing_where {
                    #struct_name { #(#arg_names),*, _marker: ::core::marker::PhantomData }
                }
            }
        }
    };

    output.into()
}
