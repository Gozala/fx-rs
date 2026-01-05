//! Implementation of the `effect!` macro.

use proc_macro::TokenStream;
use proc_macro2::{Spacing, TokenStream as TokenStream2, TokenTree};
use quote::quote;

/// Transform `yield expr` into `(expr).perform(__fx_provider).await` at the token level.
///
/// This works by scanning tokens and when we find `yield`, we collect the following
/// expression tokens until we hit a semicolon or closing delimiter at the same depth.
fn transform_yield_expressions(tokens: TokenStream2) -> TokenStream2 {
    let mut result = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(token) = iter.next() {
        match &token {
            TokenTree::Ident(ident) if ident == "yield" => {
                // Collect the expression after `yield` until we hit a semicolon at depth 0
                let mut expr_tokens = Vec::new();
                let depth = 0;

                while let Some(next) = iter.peek() {
                    match next {
                        // Stop at semicolon at depth 0
                        TokenTree::Punct(p)
                            if p.as_char() == ';' && depth == 0 && p.spacing() == Spacing::Alone =>
                        {
                            break;
                        }
                        // Track depth for nested groups
                        TokenTree::Group(g) => {
                            // Recursively transform the group contents
                            let inner = transform_yield_expressions(g.stream());
                            let new_group = proc_macro2::Group::new(g.delimiter(), inner);
                            expr_tokens.push(TokenTree::Group(new_group));
                            iter.next();
                        }
                        _ => {
                            expr_tokens.push(iter.next().unwrap());
                        }
                    }
                }

                // Generate: (expr).perform(__fx_provider).await
                let expr: TokenStream2 = expr_tokens.into_iter().collect();
                let transformed = quote! {
                    (#expr).perform(__fx_provider).await
                };
                result.extend(transformed);
            }
            TokenTree::Group(g) => {
                // Recursively transform groups (blocks, parens, brackets)
                let inner = transform_yield_expressions(g.stream());
                let new_group = proc_macro2::Group::new(g.delimiter(), inner);
                result.push(TokenTree::Group(new_group));
            }
            _ => {
                result.push(token);
            }
        }
    }

    result.into_iter().collect()
}

pub fn effect_impl(input: TokenStream) -> TokenStream {
    let input2: TokenStream2 = input.into();

    // Transform all `yield expr` into `(expr).perform(__fx_provider).await`
    let transformed = transform_yield_expressions(input2);

    // Wrap in Effectful for ergonomic .perform() syntax
    // Use a reborrowing pattern: take &mut P, reborrow as &mut inside the async block
    let output: TokenStream2 = quote! {
        fx::Effectful::new(|__fx_provider: &mut _| {
            // Reborrow the provider so we can use it multiple times in the async block
            let __fx_provider = &mut *__fx_provider;
            Box::pin(async move {
                #transformed
            })
        })
    };

    output.into()
}
