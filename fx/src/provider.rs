//! Provider trait for handling capabilities.
//!
//! The `Provider` trait defines how a type can handle capability invocations.
//! Providers are typically application-specific types that implement multiple
//! capability handler traits.

use crate::variant::{Never, Variant};
use core::future::Future;

/// A trait for types that can handle capability invocations.
///
/// Each capability type has a corresponding provider implementation that
/// specifies the output type and how to invoke the capability.
///
/// # Type Parameters
///
/// - `Capability`: The capability type being handled.
///
/// # Associated Types
///
/// - `Output`: The type returned when invoking this capability.
pub trait Provider<Capability> {
    /// The type returned when invoking this capability.
    type Output;

    /// Invoke the capability and return the result.
    ///
    /// This method is async to support both synchronous and asynchronous
    /// capability implementations.
    fn invoke(&mut self, capability: Capability) -> impl Future<Output = Self::Output> + Send;
}

// Base case: any provider can handle Never (vacuously true)
impl<P> Provider<Never> for P
where
    P: Send,
{
    type Output = Never;

    async fn invoke(&mut self, never: Never) -> Never {
        match never {}
    }
}

// Recursive case: provider for variant delegates to component providers
impl<P, H, T> Provider<Variant<H, T>> for P
where
    P: Provider<H> + Provider<T> + Send,
    H: Send,
    T: Send,
{
    type Output = Variant<<P as Provider<H>>::Output, <P as Provider<T>>::Output>;

    async fn invoke(&mut self, capability: Variant<H, T>) -> Self::Output {
        match capability {
            Variant::Here(h) => Variant::Here(self.invoke(h).await),
            Variant::There(t) => Variant::There(self.invoke(t).await),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test capabilities
    struct GetValue;
    struct SetValue(i32);

    // Test provider
    struct TestProvider {
        value: i32,
    }

    impl Provider<GetValue> for TestProvider {
        type Output = i32;

        async fn invoke(&mut self, _: GetValue) -> i32 {
            self.value
        }
    }

    impl Provider<SetValue> for TestProvider {
        type Output = ();

        async fn invoke(&mut self, cap: SetValue) {
            self.value = cap.0;
        }
    }

    #[tokio::test]
    async fn test_provider_single_capability() {
        let mut provider = TestProvider { value: 42 };
        let result = provider.invoke(GetValue).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_provider_variant_capability() {
        let mut provider = TestProvider { value: 42 };

        type Caps = Variant<GetValue, Variant<SetValue, Never>>;

        // Test Here variant
        let cap: Caps = Variant::Here(GetValue);
        let result = provider.invoke(cap).await;
        assert!(matches!(result, Variant::Here(42)));

        // Test There variant
        let cap: Caps = Variant::There(Variant::Here(SetValue(100)));
        let _ = provider.invoke(cap).await;
        assert_eq!(provider.value, 100);
    }
}
