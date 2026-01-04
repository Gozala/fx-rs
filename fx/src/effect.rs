//! Effect trait and capability routing.
//!
//! This module defines the core `Effect` trait that all effects implement,
//! and the internal `CapabilityRouter` trait that handles dispatching
//! effects to providers.

use crate::capability::CapabilityGroup;
use crate::provider::Provider;
use crate::variant::{Never, Variant};
use core::future::Future;

/// The main trait for effectful operations.
///
/// Effects are operations that require external capabilities to execute.
/// They define their output type and the capabilities they need, and can
/// be performed using a provider that supplies those capabilities.
///
/// # Associated Types
///
/// - `Output`: The type returned when the effect is performed.
///
/// # Example
///
/// ```ignore
/// struct ReadFile { path: String }
///
/// impl CapabilityGroup for ReadFile {
///     type Capabilities = Variant<ReadFile, Never>;
/// }
///
/// impl Effect for ReadFile {
///     type Output = Result<String, io::Error>;
/// }
/// ```
pub trait Effect: CapabilityGroup + Sized {
    /// The type returned when this effect is performed.
    type Output;
}

/// Internal trait for routing effects to providers.
///
/// This trait handles the actual dispatch of effects through the capability
/// system. It is implemented for effects that can be routed through a
/// specific provider.
///
/// Users typically don't need to implement this trait directly; it is
/// implemented via helper methods and macros.
pub trait CapabilityRouter<P>: Effect {
    /// Execute this effect using the provider.
    fn execute(self, provider: &mut P) -> impl Future<Output = Self::Output> + Send;
}

/// Extension trait providing the `perform` method for effects.
///
/// This is automatically implemented for any type that implements both
/// `Effect` and `CapabilityRouter<P>`.
pub trait EffectExt: Effect + Sized {
    /// Perform this effect using the given provider.
    ///
    /// This method routes the effect through the capability system and
    /// returns the result.
    fn perform<P>(self, provider: &mut P) -> impl Future<Output = Self::Output> + Send
    where
        Self: CapabilityRouter<P> + Send,
        P: Send,
    {
        async move { self.execute(provider).await }
    }
}

impl<E: Effect + Sized> EffectExt for E {}

// Implement CapabilityRouter for simple single-capability effects at position Z (first position)
impl<E, P> CapabilityRouter<P> for E
where
    E: Effect + Send,
    P: Provider<Variant<E, Never>, Output = Variant<E::Output, Never>> + Send,
    E::Output: Send,
{
    async fn execute(self, provider: &mut P) -> Self::Output {
        let capability: Variant<E, Never> = Variant::Here(self);
        let result = provider.invoke(capability).await;
        result.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test effect
    struct GetCounter;

    impl CapabilityGroup for GetCounter {
        type Capabilities = Variant<GetCounter, Never>;
    }

    impl Effect for GetCounter {
        type Output = i32;
    }

    // Test provider
    struct CounterProvider {
        count: i32,
    }

    impl Provider<GetCounter> for CounterProvider {
        type Output = i32;

        async fn invoke(&mut self, _: GetCounter) -> i32 {
            self.count
        }
    }

    #[tokio::test]
    async fn test_effect_perform() {
        let mut provider = CounterProvider { count: 42 };
        let result = GetCounter.perform(&mut provider).await;
        assert_eq!(result, 42);
    }

    // Test with multiple effects
    struct Increment;
    struct Decrement;

    impl CapabilityGroup for Increment {
        type Capabilities = Variant<Increment, Never>;
    }

    impl Effect for Increment {
        type Output = i32;
    }

    impl CapabilityGroup for Decrement {
        type Capabilities = Variant<Decrement, Never>;
    }

    impl Effect for Decrement {
        type Output = i32;
    }

    impl Provider<Increment> for CounterProvider {
        type Output = i32;

        async fn invoke(&mut self, _: Increment) -> i32 {
            self.count += 1;
            self.count
        }
    }

    impl Provider<Decrement> for CounterProvider {
        type Output = i32;

        async fn invoke(&mut self, _: Decrement) -> i32 {
            self.count -= 1;
            self.count
        }
    }

    #[tokio::test]
    async fn test_multiple_effects() {
        let mut provider = CounterProvider { count: 10 };

        let result = Increment.perform(&mut provider).await;
        assert_eq!(result, 11);

        let result = Increment.perform(&mut provider).await;
        assert_eq!(result, 12);

        let result = Decrement.perform(&mut provider).await;
        assert_eq!(result, 11);
    }
}
