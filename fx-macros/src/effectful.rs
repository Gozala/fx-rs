//! Implementation of the `#[effectful]` attribute macro.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    FnArg, ItemFn, Pat, PathArguments, Result, Token, Type,
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

/// Build provider trait bounds from ability types.
/// For Counter, generates: CounterProvider
/// For State<String>, generates: StateProvider<String>
/// For (Counter, State<String>), generates: CounterProvider + StateProvider<String>
fn build_provider_bounds(caps: &Punctuated<Type, Token![,]>) -> TokenStream2 {
    if caps.is_empty() {
        return quote! { Send };
    }

    let bounds: Vec<TokenStream2> = caps
        .iter()
        .map(|ty| {
            match ty {
                Type::Path(type_path) => {
                    // Get the last segment (e.g., "Counter" or "State<String>")
                    if let Some(segment) = type_path.path.segments.last() {
                        let ability_name = &segment.ident;
                        let provider_name =
                            Ident::new(&format!("{}Provider", ability_name), ability_name.span());

                        // Handle generic arguments
                        match &segment.arguments {
                            PathArguments::None => {
                                quote! { #provider_name }
                            }
                            PathArguments::AngleBracketed(args) => {
                                let generic_args: Vec<_> = args.args.iter().collect();
                                quote! { #provider_name<#(#generic_args),*> }
                            }
                            PathArguments::Parenthesized(_) => {
                                // Function-like generics, just use the type as-is
                                quote! { #provider_name }
                            }
                        }
                    } else {
                        // Fallback - shouldn't happen
                        quote! {}
                    }
                }
                _ => {
                    // For non-path types, we can't generate provider bounds
                    quote! {}
                }
            }
        })
        .filter(|t| !t.is_empty())
        .collect();

    if bounds.is_empty() {
        quote! { Send }
    } else {
        quote! { #(#bounds)+* + Send }
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

    // Build provider trait bounds (e.g., CounterProvider + StateProvider<T>)
    let provider_bounds = build_provider_bounds(&attr.capabilities);

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
