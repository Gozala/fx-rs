//! Implementation of the `#[effectful]` attribute macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Expr, ExprYield, ItemFn, Result, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    visit_mut::{self, VisitMut},
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

/// Visitor that transforms `yield expr` into `expr.perform(__fx_provider).await`.
struct YieldTransformer;

impl VisitMut for YieldTransformer {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        // First, recursively visit children
        visit_mut::visit_expr_mut(self, expr);

        // Then, check if this is a yield expression
        if let Expr::Yield(ExprYield {
            expr: Some(inner), ..
        }) = expr
        {
            // Transform: yield <inner> -> <inner>.perform(__fx_provider).await
            let transformed = quote! {
                (#inner).perform(__fx_provider).await
            };
            *expr = syn::parse2(transformed).expect("Failed to parse transformed yield");
        }
    }
}

pub fn effectful_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as EffectfulAttr);
    let mut func = parse_macro_input!(item as ItemFn);

    // Extract function components
    let vis = &func.vis;
    let sig = &func.sig;
    let fn_name = &sig.ident;
    let fn_generics = &sig.generics;
    let (impl_generics, _type_generics, where_clause) = fn_generics.split_for_impl();

    // Get the return type
    let return_type = match &sig.output {
        syn::ReturnType::Default => quote! { () },
        syn::ReturnType::Type(_, ty) => quote! { #ty },
    };

    // Get the function arguments (excluding self)
    let fn_args = &sig.inputs;

    // Note: capability types are parsed for future use (validation, type bounds)
    let _cap_types: Vec<&Type> = attr.capabilities.iter().collect();

    // Transform yield expressions in the function body
    let mut transformer = YieldTransformer;
    transformer.visit_block_mut(&mut func.block);

    let body = &func.block;

    // Generate the transformed function
    let output: TokenStream2 = quote! {
        #vis fn #fn_name #impl_generics (#fn_args) -> impl fx::effect::Effect<
            Output = #return_type,
        > + Send
        #where_clause
        {
            fx::task::Task::new(|__fx_provider| async move #body)
        }
    };

    output.into()
}
