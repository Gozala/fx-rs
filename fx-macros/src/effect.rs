//! Implementation of the `effect!` macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Expr, ExprYield, parse_macro_input,
    visit_mut::{self, VisitMut},
};

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

pub fn effect_impl(input: TokenStream) -> TokenStream {
    // Parse the input as a block of statements/expression
    let mut block: syn::Block = match syn::parse(input.clone()) {
        Ok(b) => b,
        Err(_) => {
            // Try parsing as a single expression and wrap it
            let expr: Expr = parse_macro_input!(input as Expr);
            syn::parse_quote!({ #expr })
        }
    };

    // Transform all yield expressions
    let mut transformer = YieldTransformer;
    transformer.visit_block_mut(&mut block);

    let stmts = &block.stmts;

    // Wrap in Task::new
    let output: TokenStream2 = quote! {
        fx::task::Task::new(|__fx_provider| async move {
            #(#stmts)*
        })
    };

    output.into()
}
