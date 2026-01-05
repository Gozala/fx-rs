//! Implementation of the `#[effectful]` attribute macro.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    FnArg, ItemFn, Pat, Result, Token, Type, TypePath,
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

/// Extract the provider trait name from a capability type.
/// For `Counter`, returns `CounterProvider`.
/// For `State<T>`, returns `StateProvider<T>`.
fn get_provider_trait(ty: &Type) -> Option<TokenStream2> {
    match ty {
        Type::Path(TypePath { qself: None, path }) => {
            let last_segment = path.segments.last()?;
            let ident = &last_segment.ident;
            let provider_ident = Ident::new(&format!("{}Provider", ident), Span::call_site());
            let args = &last_segment.arguments;
            Some(quote! { #provider_ident #args })
        }
        _ => None,
    }
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

    // Get provider traits from capability types
    let provider_traits: Vec<TokenStream2> = attr
        .capabilities
        .iter()
        .filter_map(|cap| get_provider_trait(cap))
        .collect();

    // Build the trait bounds
    let provider_bounds = if provider_traits.is_empty() {
        quote! { Send }
    } else {
        quote! { #(#provider_traits +)* Send }
    };

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
    let output: TokenStream2 = if args.is_empty() {
        // No arguments - unit struct
        if existing_params.is_empty() {
            quote! {
                #vis struct #struct_name;

                impl #struct_name {
                    /// Execute this effect with the given provider.
                    pub async fn perform<__FxP>(self, __fx_provider: &mut __FxP) -> #return_type
                    where
                        __FxP: #provider_bounds,
                        #existing_where
                    #body
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
                        __FxP: #provider_bounds,
                    #body
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
                        __FxP: #provider_bounds,
                        #existing_where
                    {
                        let Self { #(#arg_names),* } = self;
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
                        __FxP: #provider_bounds,
                    {
                        let Self { #(#arg_names),*, _marker } = self;
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
