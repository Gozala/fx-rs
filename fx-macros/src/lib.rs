//! Procedural macros for the fx effect system.
//!
//! This crate provides three main macros:
//!
//! - `ability!{}` - Declaratively define abilities with their capabilities
//! - `effect!{}` - Create inline effectful code blocks with `yield`
//! - `#[effectful]` - Wrap async functions to return effects

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
/// # Syntax
///
/// ```ignore
/// ability! {
///     $vis:vis $Name:ident $(<$($T:ident),+>)? {
///         $(fn $method:ident $(<$($M:ident $(: $bound:path)*)?),+>)? ($($arg:ident: $ArgTy:ty),*) -> $Ret:ty);* $(;)?
///     }
/// }
/// ```
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
/// # Syntax
///
/// ```ignore
/// effect! {
///     // Code with yield expressions
///     let value = yield SomeEffect::method();
///     yield AnotherEffect::do_something(value);
///     value
/// }
/// ```
///
/// # Expansion
///
/// The macro transforms `yield expr` into `expr.perform(provider).await`
/// and wraps the body in a Task closure.
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
#[proc_macro]
pub fn effect(input: TokenStream) -> TokenStream {
    effect::effect_impl(input)
}

/// Mark an async function as effectful.
///
/// The `#[effectful]` attribute macro transforms an async function
/// that uses `yield` expressions into a function that returns an effect.
///
/// # Syntax
///
/// ```ignore
/// #[effectful(CapabilityGroup1, CapabilityGroup2, ...)]
/// async fn name(args...) -> ReturnType {
///     // body with yield expressions
/// }
/// ```
///
/// # Example
///
/// ```ignore
/// #[effectful(State<i32>, Logger)]
/// async fn read_and_log() -> i32 {
///     let value = yield State::<i32>::get();
///     yield Logger::log(value.to_string());
///     value
/// }
///
/// // Usage:
/// let result = read_and_log().perform(&mut provider).await;
/// ```
///
/// # Expansion
///
/// The macro transforms the function to return `impl Effect<Output = ReturnType>`
/// with the appropriate capability bounds.
#[proc_macro_attribute]
pub fn effectful(attr: TokenStream, item: TokenStream) -> TokenStream {
    effectful::effectful_impl(attr, item)
}
