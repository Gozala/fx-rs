//! Implementation of the `ability!` macro.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Generics, Ident, Result, Token, Type, Visibility, WhereClause, braced, parenthesized,
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

    // Generate CapabilityGroup impls for each capability
    let cap_group_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            quote! {
                impl #cap_impl_generics fx::capability::CapabilityGroup for #cap_name #cap_type_generics #cap_where {
                    type Capabilities = fx::variant::Variant<#cap_name #cap_type_generics, fx::variant::Never>;
                }
            }
        })
        .collect();

    // Generate Effect impls for each capability
    let effect_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let return_type = &m.return_type;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            // Add Send bounds for async
            let send_where = add_send_bounds(&combined_generics, cap_where.cloned());

            quote! {
                impl #cap_impl_generics fx::effect::Effect for #cap_name #cap_type_generics #send_where {
                    type Output = #return_type;
                }
            }
        })
        .collect();

    // Generate HasAbility impls linking each effect back to its ability
    let has_ability_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .map(|(m, cap_name)| {
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, cap_where) =
                combined_generics.split_for_impl();

            quote! {
                impl #cap_impl_generics fx::HasAbility for #cap_name #cap_type_generics #cap_where {
                    type Ability = #name #type_generics;
                }
            }
        })
        .collect();

    // Build the variant type for all capabilities (needed by dispatch_impls below)
    let variant_type = build_variant_type(&cap_struct_names, type_generics.clone());

    // Generate dispatch impls for each capability
    // Each capability gets a dispatch method with the correct Provider + Extract bounds
    // This is what perform! calls to execute effects
    let dispatch_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .enumerate()
        .map(|(idx, (m, cap_name))| {
            let return_type = &m.return_type;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (cap_impl_generics, cap_type_generics, _) =
                combined_generics.split_for_impl();

            // Build type-level index for this effect's position
            let idx_type = build_index_type(idx);

            // Build where predicates with Provider<Caps> + Extract bounds
            let mut where_predicates: Vec<TokenStream2> = vec![
                quote! { __FxP: fx::Provider<#variant_type> + Send },
                quote! { <__FxP as fx::Provider<#variant_type>>::Output: fx::Extract<#return_type, #idx_type> + Send },
                quote! { #return_type: Send },
            ];
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

            quote! {
                impl #cap_impl_generics #cap_name #cap_type_generics {
                    /// Dispatch this effect to a provider.
                    ///
                    /// This method has the correct bounds for Provider<Capabilities> + Extract.
                    /// It is called by the `perform!` macro.
                    pub async fn dispatch<__FxP>(self, provider: &mut __FxP) -> #return_type
                    where
                        #(#where_predicates),*
                    {
                        fx::PerformIn::<#variant_type, #idx_type>::perform_in(self, provider).await
                    }
                }
            }
        })
        .collect();

    // Generate Provider impls for each capability
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
                    impl #all_generics fx::provider::Provider<#cap_name #cap_type_generics> for __FxP
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
                    impl #all_generics fx::provider::Provider<#cap_name #cap_type_generics> for __FxP
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

    // Build the output variant type (return types wrapped in Variant)
    let output_variant_type = build_output_variant_type(def);

    // Build Extract bounds for each capability's return type
    let extract_bounds = build_extract_bounds(def);

    // Collect all generic params for Handles impl
    let mut handles_params: Vec<TokenStream2> = vec![quote! { __FxP }];
    for param in &generics.params {
        handles_params.push(quote! { #param });
    }

    // Build Handles where clause with Extract bounds for each effect
    let handles_where = {
        let mut predicates: Vec<TokenStream2> = vec![
            quote! { __FxP: fx::Provider<#variant_type> + Send },
            quote! { <__FxP as fx::Provider<#variant_type>>::Output: #extract_bounds Send },
        ];
        // Add Send bounds for type parameters
        for param in &generics.params {
            if let syn::GenericParam::Type(ty) = param {
                let ident = &ty.ident;
                predicates.push(quote! { #ident: Send });
            }
        }
        if let Some(wc) = where_clause {
            for pred in &wc.predicates {
                predicates.push(quote! { #pred });
            }
        }
        quote! { where #(#predicates),* }
    };

    // Generate Handle impls for each capability using PerformIn
    let handle_impls: Vec<TokenStream2> = def
        .methods
        .iter()
        .zip(cap_struct_names.iter())
        .enumerate()
        .map(|(idx, (m, cap_name))| {
            let return_type = &m.return_type;
            let method_generics = &m.generics;
            let combined_generics = combine_generics(generics, method_generics);
            let (_, cap_type_generics, _) = combined_generics.split_for_impl();

            // Build type-level index for this effect's position
            let idx_type = build_index_type(idx);

            // Build generic params including __FxP
            let mut all_params: Vec<TokenStream2> = vec![quote! { __FxP }];
            for param in &combined_generics.params {
                all_params.push(quote! { #param });
            }

            // Build where clause - using Provider<Caps> + Extract bounds
            let mut where_predicates: Vec<TokenStream2> = vec![
                quote! { __FxP: fx::Provider<#variant_type> + Send },
                quote! { <__FxP as fx::Provider<#variant_type>>::Output: fx::Extract<#return_type, #idx_type> + Send },
                quote! { #return_type: Send },
            ];
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

            quote! {
                impl <#(#all_params),*> fx::capability::Handle<#cap_name #cap_type_generics, __FxP> for #name #type_generics
                where
                    #(#where_predicates),*
                {
                    async fn handle(effect: #cap_name #cap_type_generics, provider: &mut __FxP) -> #return_type {
                        fx::PerformIn::<#variant_type, #idx_type>::perform_in(effect, provider).await
                    }
                }
            }
        })
        .collect();

    quote! {
        // Provider trait
        #vis trait #provider_trait_name #impl_generics #where_clause {
            #(#provider_methods)*
        }

        // Capability structs
        #(#cap_structs)*

        // CapabilityGroup impls
        #(#cap_group_impls)*

        // Effect impls
        #(#effect_impls)*

        // HasAbility impls - link effects to their ability
        #(#has_ability_impls)*

        // Dispatch impls - each capability gets a dispatch method for perform!
        #(#dispatch_impls)*

        // Provider impls (blanket)
        #(#provider_impls)*

        // Handle impls - dispatch effects through ability type
        #(#handle_impls)*

        // Marker struct
        #vis struct #name #impl_generics (::core::marker::PhantomData<#phantom_tuple>) #where_clause;

        impl #impl_generics #name #type_generics #where_clause {
            #(#builder_methods)*
        }

        impl #impl_generics fx::capability::CapabilityGroup for #name #type_generics #where_clause {
            type Capabilities = #variant_type;
        }

        impl #impl_generics fx::capability::OutputVariant for #name #type_generics #where_clause {
            type ExpectedOutput = #output_variant_type;
        }

        // Handles impl - indicates P can handle all effects in this ability group
        // This encapsulates Provider<Caps> + Extract bounds for each effect
        impl <#(#handles_params),*> fx::capability::Handles<__FxP> for #name #type_generics
            #handles_where
        {
            type Output = <__FxP as fx::Provider<#variant_type>>::Output;
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

/// Add Send bounds to all type parameters.
fn add_send_bounds(generics: &Generics, where_clause: Option<WhereClause>) -> TokenStream2 {
    let mut predicates = Vec::new();

    for param in &generics.params {
        if let syn::GenericParam::Type(ty) = param {
            let ident = &ty.ident;
            predicates.push(quote! { #ident: Send });
        }
    }

    if let Some(wc) = where_clause {
        let existing: Vec<_> = wc.predicates.iter().collect();
        quote! {
            where #(#existing,)* #(#predicates),*
        }
    } else if !predicates.is_empty() {
        quote! {
            where #(#predicates),*
        }
    } else {
        quote! {}
    }
}

/// Build a nested Variant type from capability names.
fn build_variant_type(cap_names: &[Ident], type_generics: syn::TypeGenerics) -> TokenStream2 {
    if cap_names.is_empty() {
        quote! { fx::variant::Never }
    } else {
        let mut result = quote! { fx::variant::Never };
        for cap in cap_names.iter().rev() {
            result = quote! { fx::variant::Variant<#cap #type_generics, #result> };
        }
        result
    }
}

/// Build the output variant type from method return types.
/// E.g., for methods returning i32, (), i32 -> Variant<i32, Variant<(), Variant<i32, Never>>>
fn build_output_variant_type(def: &AbilityDef) -> TokenStream2 {
    if def.methods.is_empty() {
        quote! { fx::variant::Never }
    } else {
        let mut result = quote! { fx::variant::Never };
        for m in def.methods.iter().rev() {
            let return_type = &m.return_type;
            result = quote! { fx::variant::Variant<#return_type, #result> };
        }
        result
    }
}

/// Build Extract bounds for each capability's return type.
/// E.g., Extract<i32, Z> + Extract<(), S<Z>> + Extract<i32, S<S<Z>>> +
fn build_extract_bounds(def: &AbilityDef) -> TokenStream2 {
    if def.methods.is_empty() {
        return quote! {};
    }

    let bounds: Vec<TokenStream2> = def
        .methods
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let return_type = &m.return_type;
            let idx_type = build_index_type(idx);
            quote! { fx::Extract<#return_type, #idx_type> }
        })
        .collect();

    quote! { #(#bounds)+* + }
}

/// Build type-level index: Z, S<Z>, S<S<Z>>, etc.
fn build_index_type(idx: usize) -> TokenStream2 {
    let mut result = quote! { fx::Z };
    for _ in 0..idx {
        result = quote! { fx::S<#result> };
    }
    result
}
