//! Effect trait for effectful operations.
//!
//! This module defines the core `Effect` trait that all effects implement,
//! providing the `perform` method to execute effects with a provider.

use crate::capability::Capability;
use crate::provider::Provider;
use crate::variant::{Extract, VariantOf};
use core::future::Future;

/// The main trait for effectful operations.
///
/// Effects are operations that require external capabilities to execute.
/// They extend `Capability` (defining `Yield`/`Resume` variants) and add
/// the `Outcome` type and `perform` method.
///
/// # Associated Types
///
/// - `Outcome`: The unwrapped type returned when the effect is performed.
///
/// # Example
///
/// ```ignore
/// // For an individual effect:
/// impl Capability for CounterGetCount {
///     type Yield = Variant<CounterGetCount>;
///     type Resume = Variant<i32>;
/// }
///
/// impl Effect for CounterGetCount {
///     type Outcome = i32;
///     // perform method is implemented by the macro
/// }
/// ```
pub trait Effect: Capability + Sized {
    /// The unwrapped type returned when this effect is performed.
    type Outcome;

    /// Perform this effect using the given provider.
    ///
    /// The effect is yielded to the provider (via the `Yield` variant),
    /// and the result is extracted from the provider's response (the `Resume` variant).
    fn perform<P>(self, provider: &mut P) -> impl Future<Output = Self::Outcome> + Send
    where
        Self: Send,
        P: Provider<Self::Yield, Output = Self::Resume> + Send,
        Self::Yield: Send,
        Self::Resume: Send,
        Self::Outcome: Send;
}

/// Helper function to perform an effect by injecting it into a variant,
/// invoking the provider, and extracting the result.
///
/// This is used internally by `Effect::perform` implementations.
///
/// # Type Parameters
///
/// - `E`: The effect type
/// - `Caps`: The capabilities variant type (same as `E::Yield`)
/// - `Resume`: The resume variant type (same as `E::Resume`)
/// - `Outcome`: The unwrapped output type (same as `E::Outcome`)
/// - `Idx`: Type-level index for the effect's position in the variant
/// - `P`: The provider type
pub async fn perform_effect<E, Caps, Resume, Outcome, Idx, P>(
    effect: E,
    provider: &mut P,
) -> Outcome
where
    E: Send,
    Caps: VariantOf<E, Idx> + Send,
    P: Provider<Caps, Output = Resume> + Send,
    Resume: Extract<Outcome, Idx> + Send,
    Outcome: Send,
{
    // Inject this effect into the Capabilities variant
    let caps: Caps = Caps::of(effect);
    // Invoke the provider with the variant
    let result = provider.invoke(caps).await;
    // Extract our output from the result variant
    match result.extract() {
        Ok(output) => output,
        Err(_) => unreachable!("effect was injected at the same index we extract from"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::{Never, Variant, Z};

    // Test effect
    struct GetCounter;

    impl Capability for GetCounter {
        type Yield = Variant<GetCounter, Never>;
        type Resume = Variant<i32, Never>;
    }

    impl Effect for GetCounter {
        type Outcome = i32;

        async fn perform<P>(self, provider: &mut P) -> i32
        where
            Self: Send,
            P: Provider<Self::Yield, Output = Self::Resume> + Send,
            Self::Yield: Send,
            Self::Resume: Send,
            Self::Outcome: Send,
        {
            perform_effect::<Self, Self::Yield, Self::Resume, Self::Outcome, Z, P>(
                self, provider,
            )
            .await
        }
    }

    // Test provider - implements Provider<Yield> directly
    struct TestCounterProvider {
        count: i32,
    }

    impl Provider<Variant<GetCounter, Never>> for TestCounterProvider {
        type Output = Variant<i32, Never>;

        async fn invoke(&mut self, _effect: Variant<GetCounter, Never>) -> Self::Output {
            Variant::Here(self.count)
        }
    }

    #[tokio::test]
    async fn test_effect_perform() {
        let mut provider = TestCounterProvider { count: 42 };
        let result = GetCounter.perform(&mut provider).await;
        assert_eq!(result, 42);
    }
}
