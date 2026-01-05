//! Implementation of the `#[effectful]` attribute macro.

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    ItemFn, Result, Token, Type, TypePath,
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
            // Get the last segment (e.g., "Counter" or "State")
            let last_segment = path.segments.last()?;
            let ident = &last_segment.ident;
            let provider_ident = Ident::new(&format!("{}Provider", ident), Span::call_site());

            // Build the full path with Provider suffix
            let mut segments = path.segments.clone();
            if let Some(last) = segments.last_mut() {
                last.ident = provider_ident;
            }

            let prefix: Vec<_> = segments.iter().collect();
            Some(quote! { #(#prefix)::* })
        }
        _ => None,
    }
}

pub fn effectful_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute (capability types)
    let attr = parse_macro_input!(attr as EffectfulAttr);

    // Parse the function
    let func = parse_macro_input!(item as ItemFn);

    // Extract function components
    let vis = &func.vis;
    let fn_name = &func.sig.ident;
    let fn_generics = &func.sig.generics;

    // Get the return type
    let return_type = match &func.sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    // Get the function arguments
    let fn_args = &func.sig.inputs;

    // Get the function body
    let body = &func.block;

    // Build generics with __FxP added
    let existing_params = &fn_generics.params;
    let existing_where = fn_generics.where_clause.as_ref();

    // Collect provider trait bounds from capabilities
    // For each capability like Counter, we add __FxP: CounterProvider
    let provider_bounds: Vec<TokenStream2> = attr
        .capabilities
        .iter()
        .filter_map(|cap| get_provider_trait(cap))
        .map(|provider_trait| quote! { __FxP: #provider_trait })
        .collect();

    // Generate a unique name for the inner async function
    let inner_fn_name = Ident::new(&format!("__{}_inner", fn_name), Span::call_site());

    // Generate the transformed function
    // We define an inner async function and wrap it in Effectful
    let output: TokenStream2 = if existing_params.is_empty() {
        if provider_bounds.is_empty() {
            quote! {
                #vis fn #fn_name<__FxP: Send>(#fn_args) -> fx::Effectful<
                    impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                >
                #existing_where
                {
                    fn #inner_fn_name<__FxP: Send>(__fx_provider: &mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>> {
                        Box::pin(async move #body)
                    }
                    fx::Effectful::new(#inner_fn_name)
                }
            }
        } else {
            quote! {
                #vis fn #fn_name<__FxP: Send + #(#provider_bounds +)*>(#fn_args) -> fx::Effectful<
                    impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                >
                #existing_where
                {
                    fn #inner_fn_name<__FxP: Send + #(#provider_bounds +)*>(__fx_provider: &mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>> {
                        Box::pin(async move #body)
                    }
                    fx::Effectful::new(#inner_fn_name)
                }
            }
        }
    } else {
        if provider_bounds.is_empty() {
            quote! {
                #vis fn #fn_name<__FxP: Send, #existing_params>(#fn_args) -> fx::Effectful<
                    impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                >
                #existing_where
                {
                    fn #inner_fn_name<__FxP: Send, #existing_params>(__fx_provider: &mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                    #existing_where
                    {
                        Box::pin(async move #body)
                    }
                    fx::Effectful::new(#inner_fn_name)
                }
            }
        } else {
            quote! {
                #vis fn #fn_name<__FxP: Send + #(#provider_bounds +)*, #existing_params>(#fn_args) -> fx::Effectful<
                    impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                >
                #existing_where
                {
                    fn #inner_fn_name<__FxP: Send + #(#provider_bounds +)*, #existing_params>(__fx_provider: &mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
                    #existing_where
                    {
                        Box::pin(async move #body)
                    }
                    fx::Effectful::new(#inner_fn_name)
                }
            }
        }
    };

    output.into()
}
