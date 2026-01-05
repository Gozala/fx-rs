//! Procedural macros for the fx effect system.
//!
//! This crate provides these macros:
//!
//! - `ability!{}` - Declaratively define abilities with their capabilities
//! - `effect!{}` - Create inline effectful code blocks with `yield`
//! - `#[effectful]` - Wrap functions to return effects (use `perform!` inside)
//! - `perform!` - Helper macro for performing effects in `#[effectful]` functions

use proc_macro::TokenStream;

mod ability;
mod effect;
mod effectful;

/// Define an ability with its capabilities.
///
/// The `ability!` macro generates:
/// - A provider trait that implementors must satisfy
/// - Capability structs for each method
/// - Effect and CapabilityGroup implementations
/// - A marker struct with builder methods
///
/// # Example
///
/// ```ignore
/// ability! {
///     pub State<T> {
///         fn get() -> T;
///         fn set(value: T) -> ()
///     }
/// }
/// ```
///
/// This generates:
/// - `StateProvider<T>` trait with `get` and `set` methods
/// - `StateGet<T>` and `StateSet<T>` capability structs
/// - Effect implementations for each capability
/// - `State<T>` marker with `State::<T>::get()` and `State::<T>::set(value)` builders
#[proc_macro]
pub fn ability(input: TokenStream) -> TokenStream {
    ability::ability_impl(input)
}

/// Create an inline effectful code block.
///
/// The `effect!{}` macro wraps code containing `yield` expressions
/// into a `Task` that can be performed with a provider.
///
/// # Example
///
/// ```ignore
/// let my_effect = effect! {
///     let value = yield State::<i32>::get();
///     yield Logger::log(value.to_string());
///     value
/// };
///
/// let result = my_effect.perform(&mut provider).await;
/// ```
///
/// The macro transforms `yield expr` into `expr.perform(provider).await`
/// and wraps the body in a Task closure.
#[proc_macro]
pub fn effect(input: TokenStream) -> TokenStream {
    effect::effect_impl(input)
}

/// Mark a function as effectful.
///
/// The `#[effectful]` attribute transforms a function into one that returns
/// an effect struct with a `.perform()` method. Use `perform!()` inside to
/// invoke effects.
///
/// # Example
///
/// ```ignore
/// #[effectful(State<i32>)]
/// fn read_value() -> i32 {
///     perform!(State::<i32>::get())
/// }
///
/// // Usage:
/// let result = read_value().perform(&mut provider).await;
/// ```
///
/// For `yield` syntax, use `effect!{}` blocks instead:
///
/// ```ignore
/// let result = effect!(&mut provider, {
///     yield State::<i32>::get()
/// }).await;
/// ```
#[proc_macro_attribute]
pub fn effectful(attr: TokenStream, item: TokenStream) -> TokenStream {
    effectful::effectful_impl(attr, item)
}

/// Perform an effect inside an effectful context.
///
/// This macro is used inside `#[effectful]` functions to invoke effects.
/// It calls the capability's `dispatch` method, which has the correct
/// Provider + Extract bounds for the specific effect.
///
/// # Example
///
/// ```ignore
/// #[effectful(Counter)]
/// fn increment_twice() -> i32 {
///     perform!(Counter::increment());
///     perform!(Counter::increment());
///     perform!(Counter::get_count())
/// }
/// ```
#[proc_macro]
pub fn perform(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();
    // Call the capability's dispatch method which has the correct bounds
    let output = quote::quote! {
        (#input2).dispatch(__fx_provider).await
    };
    output.into()
}
