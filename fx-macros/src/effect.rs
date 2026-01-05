//! Implementation of the `effect!` macro.

use proc_macro::TokenStream;
use proc_macro2::{Spacing, TokenStream as TokenStream2, TokenTree};
use quote::quote;

/// Transform `yield expr` into `(expr).perform(__fx_provider).await` at the token level.
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

/// Parse the effect! macro input.
/// Syntax: effect!(provider, { body })
/// Or just: effect!({ body }) - for use inside effectful functions where __fx_provider exists
struct EffectInput {
    provider: Option<syn::Expr>,
    body: TokenStream2,
}

impl syn::parse::Parse for EffectInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Try to parse as: provider, { body }
        if input.peek(syn::token::Brace) {
            // Just a body block - use __fx_provider from scope
            let content;
            syn::braced!(content in input);
            let body: TokenStream2 = content.parse()?;
            Ok(EffectInput {
                provider: None,
                body,
            })
        } else {
            // provider, { body }
            let provider: syn::Expr = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let content;
            syn::braced!(content in input);
            let body: TokenStream2 = content.parse()?;
            Ok(EffectInput {
                provider: Some(provider),
                body,
            })
        }
    }
}

pub fn effect_impl(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as EffectInput);

    // Transform all `yield expr` into `(expr).perform(__fx_provider).await`
    let transformed = transform_yield_expressions(parsed.body);

    let output: TokenStream2 = if let Some(provider) = parsed.provider {
        // effect!(provider, { body }) - creates an async block with provider bound
        quote! {
            {
                let __fx_provider = #provider;
                async move {
                    #transformed
                }
            }
        }
    } else {
        // effect!({ body }) - assumes __fx_provider is in scope
        quote! {
            async {
                #transformed
            }
        }
    };

    output.into()
}
