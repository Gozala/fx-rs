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

            // Keep the generics from the original type
            let args = &last_segment.arguments;

            Some(quote! { #provider_ident #args })
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
    let existing_where = &fn_generics.where_clause;

    // Collect provider trait names from capabilities (just the trait names, not full bounds)
    let provider_traits: Vec<TokenStream2> = attr
        .capabilities
        .iter()
        .filter_map(|cap| get_provider_trait(cap))
        .collect();

    // Build the type parameter bounds
    let type_bounds = if provider_traits.is_empty() {
        quote! { Send }
    } else {
        quote! { #(#provider_traits +)* Send }
    };

    // Generate the transformed function
    let output: TokenStream2 = if existing_params.is_empty() {
        quote! {
            #vis fn #fn_name<__FxP>(#fn_args) -> fx::Effectful<
                impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
            >
            where
                __FxP: #type_bounds,
                #existing_where
            {
                fx::Effectful::new(move |__fx_provider: &mut __FxP| -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>> {
                    Box::pin(async move #body)
                })
            }
        }
    } else {
        quote! {
            #vis fn #fn_name<__FxP, #existing_params>(#fn_args) -> fx::Effectful<
                impl FnOnce(&mut __FxP) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>>
            >
            where
                __FxP: #type_bounds,
                #existing_where
            {
                fx::Effectful::new(move |__fx_provider: &mut __FxP| -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = #return_type> + Send + '_>> {
                    Box::pin(async move #body)
                })
            }
        }
    };

    output.into()
}
