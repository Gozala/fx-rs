//! Implementation of the `ability!` macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Generics, Ident, Result, Token, Type, Visibility, braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// A single method definition within an ability.
struct AbilityMethod {
    name: Ident,
    generics: Generics,
    args: Vec<(Ident, Type)>,
    return_type: Type,
}

impl Parse for AbilityMethod {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token![fn]>()?;
        let name: Ident = input.parse()?;

        // Parse optional generics
        let generics: Generics = input.parse()?;

        // Parse arguments
        let content;
        parenthesized!(content in input);
        let args_parsed: Punctuated<(Ident, Token![:], Type), Token![,]> = content
            .parse_terminated(
                |input| {
                    let name: Ident = input.parse()?;
                    let colon: Token![:] = input.parse()?;
                    let ty: Type = input.parse()?;
                    Ok((name, colon, ty))
                },
                Token![,],
            )?;
        let args: Vec<(Ident, Type)> = args_parsed.into_iter().map(|(n, _, t)| (n, t)).collect();

        // Parse return type
        input.parse::<Token![->]>()?;
        let return_type: Type = input.parse()?;

        Ok(AbilityMethod {
            name,
            generics,
            args,
            return_type,
        })
    }
}

/// The full ability definition.
struct AbilityDef {
    vis: Visibility,
    name: Ident,
    generics: Generics,
    methods: Vec<AbilityMethod>,
}

impl Parse for AbilityDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let vis: Visibility = input.parse()?;
        let name: Ident = input.parse()?;

        // Parse optional generics
        let generics: Generics = input.parse()?;

        // Parse methods in braces
        let content;
        braced!(content in input);

        let mut methods = Vec::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
            // Optional semicolon between methods
            let _ = content.parse::<Token![;]>();
        }

        Ok(AbilityDef {
            vis,
            name,
            generics,
            methods,
        })
    }
}

pub fn ability_impl(input: TokenStream) -> TokenStream {
    let def = syn::parse_macro_input!(input as AbilityDef);
    let output = generate_ability(&def);
    output.into()
}

fn generate_ability(def: &AbilityDef) -> TokenStream2 {
    let vis = &def.vis;
    let name = &def.name;
    let generics = &def.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    // Generate provider trait name
    let provider_trait_name = format_ident!("{}Provider", name);

    // Generate type alias names
    let do_type_name = format_ident!("{}Do", name);
    let done_type_name = format_ident!("{}Done", name);

    // Generate capability struct names
    let cap_struct_names: Vec<Ident> = def
        .methods
        .iter()
        .map(|m| format_ident!("{}{}", name, to_pascal_case(&m.name.to_string())))
        .collect();

    // Generate provider trait methods
    let provider_methods: Vec<TokenStream2> = def
        .methods
        .iter()
        .map(|m| {
            let method_name = &m.name;
            let method_generics = &m.generics;
            let return_type = &m.return_type;
            let args: Vec<TokenStream2> = m
                .args
                .iter()
                .map(|(arg_name, arg_type)| {
                    quote! { #arg_name: #arg_type }
                })
                .collect();

            quote! {
                fn #method_name #method_generics (&mut self #(, #args)*) -> #return_type;
            }
        })
        .collect();

    // Generate capability structs
    let cap_structs: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            if m.args.is_empty() {
                // Unit struct with PhantomData if there are generics
                if combined_generics.params.is_empty() {
                    quote! {
                        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                        #vis struct #cap_name;
                    }
                } else {
                    // Collect all generic params for PhantomData
                    let phantom_types: Vec<TokenStream2> = combined_generics
                        .params
                        .iter()
                        .filter_map(|p| match p {
                            syn::GenericParam::Type(t) => {
                                let ident = &t.ident;
                                Some(quote! { #ident })
                            }
                            _ => None,
                        })
                        .collect();

                    quote! {
                        #vis struct #cap_name #cap_impl_generics (
                            ::core::marker::PhantomData<(#(#phantom_types),*)>
                        ) #cap_where;

                        impl #cap_impl_generics ::core::fmt::Debug for #cap_name #cap_type_generics #cap_where {
                            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                                f.debug_struct(stringify!(#cap_name)).finish()
                            }
                        }

                        impl #cap_impl_generics ::core::clone::Clone for #cap_name #cap_type_generics #cap_where {
                            fn clone(&self) -> Self {
                                Self(::core::marker::PhantomData)
                            }
                        }

                        impl #cap_impl_generics ::core::marker::Copy for #cap_name #cap_type_generics #cap_where {}
                    }
                }
            } else {
                // Struct with fields
                let fields: Vec<TokenStream2> = m
                    .args
                    .iter()
                    .map(|(arg_name, arg_type)| {
                        quote! { pub #arg_name: #arg_type }
                    })
                    .collect();

                quote! {
                    #vis struct #cap_name #cap_impl_generics #cap_where {
                        #(#fields),*
                    }
                }
            }
        })
        .collect();

    // Build the variant types
    let yield_type = build_variant_type(&cap_struct_names, type_generics.clone());
    let resume_type = build_output_variant_type(def);

    // Generate Capability impls for each effect (individual effect's Yield/Resume)
    let cap_capability_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let return_type = &m.return_type;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            quote! {
                impl #cap_impl_generics fx::Capability for #cap_name #cap_type_generics #cap_where {
                    type Yield = fx::Variant<#cap_name #cap_type_generics, fx::Never>;
                    type Resume = fx::Variant<#return_type, fx::Never>;
                }
            }
        })
        .collect();

    // Generate Effect impls for each capability (with perform method)
    let effect_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .enumerate()
        .map(|(idx, (m, cap_name))| {
            let return_type = &m.return_type;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            // Build type-level index for this effect's position
            let _idx_type = build_index_type(idx);

            // The perform method bounds are fixed by the trait - only these three bounds
            // are allowed. Extra generic bounds must be on the impl block, not the method.
            quote! {
                impl #cap_impl_generics fx::Effect for #cap_name #cap_type_generics #cap_where {
                    type Outcome = #return_type;

                    async fn perform<__FxP>(self, provider: &mut __FxP) -> Self::Outcome
                    where
                        Self: Send,
                        __FxP: fx::Provider<Self::Yield, Output = Self::Resume> + Send,
                        Self::Outcome: Send,
                    {
                        fx::perform_effect::<Self, Self::Yield, Self::Resume, Self::Outcome, fx::Z, __FxP>(
                            self, provider
                        ).await
                    }
                }
            }
        })
        .collect();

    // Generate CapabilityOf impls linking each effect back to its ability
    let capability_of_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .enumerate()
        .map(|(idx, (m, cap_name))| {
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            // Build type-level index for this effect's position
            let idx_type = build_index_type(idx);

            quote! {
                impl #cap_impl_generics fx::CapabilityOf for #cap_name #cap_type_generics #cap_where {
                    type Ability = #name #type_generics;
                    type Index = #idx_type;
                }
            }
        })
        .collect();

    // Generate Provider impls for each capability (blanket impl from provider trait)
    let provider_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let method_name = &m.name;
            let method_generics = &m.generics;
            let return_type = &m.return_type;
            let combined_generics = combine_generics(generics, method_generics);
            let (_cap_impl_generics, cap_type_generics, _cap_where) =
                combined_generics.split_for_impl();

            let arg_names: Vec<&Ident> = m.args.iter().map(|(n, _)| n).collect();

            // Build generic params including __FxP
            let mut all_params: Vec<TokenStream2> = vec![quote! { __FxP }];
            for param in &combined_generics.params {
                all_params.push(quote! { #param });
            }
            let all_generics = quote! { <#(#all_params),*> };

            // Build where clause with provider bound
            let mut where_predicates: Vec<TokenStream2> =
                vec![quote! { __FxP: #provider_trait_name #type_generics + Send }];
            for param in &combined_generics.params {
                if let syn::GenericParam::Type(ty) = param {
                    let ident = &ty.ident;
                    where_predicates.push(quote! { #ident: Send });
                }
            }
            if let Some(wc) = &combined_generics.where_clause {
                for pred in &wc.predicates {
                    where_predicates.push(quote! { #pred });
                }
            }
            let where_clause = quote! { where #(#where_predicates),* };

            if m.args.is_empty() {
                quote! {
                    impl #all_generics fx::Provider<#cap_name #cap_type_generics> for __FxP
                        #where_clause
                    {
                        type Output = #return_type;

                        async fn invoke(&mut self, _: #cap_name #cap_type_generics) -> #return_type {
                            self.#method_name()
                        }
                    }
                }
            } else {
                quote! {
                    impl #all_generics fx::Provider<#cap_name #cap_type_generics> for __FxP
                        #where_clause
                    {
                        type Output = #return_type;

                        async fn invoke(&mut self, effect: #cap_name #cap_type_generics) -> #return_type {
                            self.#method_name(#(effect.#arg_names),*)
                        }
                    }
                }
            }
        })
        .collect();

    // Generate builder methods on the marker struct
    let builder_methods: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let method_name = &m.name;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (method_impl_generics, _method_type_generics, method_where) =
                method_generics.split_for_impl();
            let args: Vec<TokenStream2> = m
                .args
                .iter()
                .map(|(arg_name, arg_type)| {
                    quote! { #arg_name: #arg_type }
                })
                .collect();
            let arg_names: Vec<&Ident> = m.args.iter().map(|(n, _)| n).collect();

            // For the capability type, we need ability generics + method generics
            let (_, cap_type_generics, _) = combined_generics.split_for_impl();

            if m.args.is_empty() {
                // No arguments - check if there are ANY generics (ability + method)
                if combined_generics.params.is_empty() {
                    // Unit struct - no generics at all
                    quote! {
                        #[inline]
                        pub fn #method_name() -> #cap_name {
                            #cap_name
                        }
                    }
                } else {
                    // Has generics - use PhantomData
                    quote! {
                        #[inline]
                        pub fn #method_name #method_impl_generics () -> #cap_name #cap_type_generics #method_where {
                            #cap_name(::core::marker::PhantomData)
                        }
                    }
                }
            } else {
                quote! {
                    #[inline]
                    pub fn #method_name #method_impl_generics (#(#args),*) -> #cap_name #cap_type_generics #method_where {
                        #cap_name { #(#arg_names),* }
                    }
                }
            }
        })
        .collect();

    // Extract type parameter identifiers for PhantomData
    let phantom_types: Vec<TokenStream2> = generics
        .params
        .iter()
        .filter_map(|p| match p {
            syn::GenericParam::Type(t) => {
                let ident = &t.ident;
                Some(quote! { #ident })
            }
            syn::GenericParam::Lifetime(l) => {
                let lt = &l.lifetime;
                Some(quote! { &#lt () })
            }
            syn::GenericParam::Const(_) => None,
        })
        .collect();

    let phantom_tuple = if phantom_types.is_empty() {
        quote! { () }
    } else {
        quote! { (#(#phantom_types),*) }
    };

    quote! {
        // Provider trait (convenience for implementors)
        #vis trait #provider_trait_name #impl_generics #where_clause {
            #(#provider_methods)*
        }

        // Type aliases for the ability's Yield and Resume variants
        #vis type #do_type_name #impl_generics = #yield_type;
        #vis type #done_type_name #impl_generics = #resume_type;

        // Capability structs
        #(#cap_structs)*

        // Capability impls for each effect (individual Yield/Resume)
        #(#cap_capability_impls)*

        // Effect impls (with Outcome and perform method)
        #(#effect_impls)*

        // CapabilityOf impls - link effects to their ability
        #(#capability_of_impls)*

        // Provider impls (blanket impl from provider trait)
        #(#provider_impls)*

        // Marker struct for the ability
        #vis struct #name #impl_generics (::core::marker::PhantomData<#phantom_tuple>) #where_clause;

        impl #impl_generics #name #type_generics #where_clause {
            #(#builder_methods)*
        }

        // Capability impl for the ability itself (wide Yield/Resume)
        impl #impl_generics fx::Capability for #name #type_generics #where_clause {
            type Yield = #do_type_name #type_generics;
            type Resume = #done_type_name #type_generics;
        }

        // Effect impl for the ability (Outcome is the Resume variant)
        impl #impl_generics fx::Effect for #name #type_generics #where_clause {
            type Outcome = #done_type_name #type_generics;

            async fn perform<__FxP>(self, _provider: &mut __FxP) -> Self::Outcome
            where
                Self: Send,
                __FxP: fx::Provider<Self::Yield, Output = Self::Resume> + Send,
                Self::Outcome: Send,
            {
                // Ability markers can't be performed directly
                // They're used for type-level information
                unreachable!("ability marker struct cannot be performed directly")
            }
        }
    }
}

/// Convert snake_case to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect()
}

/// Combine two sets of generics.
fn combine_generics(a: &Generics, b: &Generics) -> Generics {
    let mut result = a.clone();
    for param in &b.params {
        result.params.push(param.clone());
    }
    if let Some(b_where) = &b.where_clause {
        if let Some(a_where) = &mut result.where_clause {
            for pred in &b_where.predicates {
                a_where.predicates.push(pred.clone());
            }
        } else {
            result.where_clause = Some(b_where.clone());
        }
    }
    result
}

/// Build a nested Variant type from capability names.
fn build_variant_type(cap_names: &[Ident], type_generics: syn::TypeGenerics) -> TokenStream2 {
    if cap_names.is_empty() {
        quote! { fx::Never }
    } else {
        let mut result = quote! { fx::Never };
        for cap in cap_names.iter().rev() {
            result = quote! { fx::Variant<#cap #type_generics, #result> };
        }
        result
    }
}

/// Build the output variant type from method return types.
/// E.g., for methods returning i32, (), i32 -> Variant<i32, Variant<(), Variant<i32, Never>>>
fn build_output_variant_type(def: &AbilityDef) -> TokenStream2 {
    if def.methods.is_empty() {
        quote! { fx::Never }
    } else {
        let mut result = quote! { fx::Never };
        for m in def.methods.iter().rev() {
            let return_type = &m.return_type;
            result = quote! { fx::Variant<#return_type, #result> };
        }
        result
    }
}

/// Build type-level index: Z, S<Z>, S<S<Z>>, etc.
fn build_index_type(idx: usize) -> TokenStream2 {
    let mut result = quote! { fx::Z };
    for _ in 0..idx {
        result = quote! { fx::S<#result> };
    }
    result
}
