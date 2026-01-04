//! # FX - A Rust Effect System
//!
//! `fx` provides an effect system for Rust with:
//!
//! - **Declarative abilities** via the `ability!` macro
//! - **Type-safe capability composition** using `Variant` (coproduct types)
//! - **Async-ready** effects with `.perform(provider)` method
//! - **Inline effectful code** with the `effect!{}` block macro
//! - **Function-level effects** with the `#[effectful]` attribute macro
//!
//! ## Quick Start
//!
//! ```ignore
//! use fx::prelude::*;
//!
//! // Define abilities
//! ability! {
//!     pub State<T> {
//!         fn get() -> T;
//!         fn set(value: T) -> ()
//!     }
//! }
//!
//! // Implement the provider trait
//! struct MyApp { value: i32 }
//!
//! impl StateProvider<i32> for MyApp {
//!     fn get(&mut self) -> i32 { self.value }
//!     fn set(&mut self, value: i32) { self.value = value; }
//! }
//!
//! // Use effects
//! async fn example() {
//!     let mut app = MyApp { value: 42 };
//!     let x = State::<i32>::get().perform(&mut app).await;
//!     println!("Value: {}", x);
//! }
//! ```
//!
//! ## Core Concepts
//!
//! ### Effects
//!
//! Effects are operations that require external capabilities to execute.
//! Each effect specifies its required capabilities and output type.
//!
//! ### Capabilities
//!
//! Capabilities are individual operations that a provider can perform.
//! Multiple capabilities are composed using `Variant` types.
//!
//! ### Providers
//!
//! Providers implement the actual logic for handling capabilities.
//! A single provider can implement multiple capability handlers.
//!
//! ### Tasks
//!
//! Tasks wrap effectful closures, allowing complex effect compositions
//! to be represented as values.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod capability;
pub mod effect;
pub mod provider;
pub mod task;
pub mod variant;

/// Re-export of procedural macros.
pub use fx_macros::{ability, effect, effectful};

/// Convenient prelude module for common imports.
pub mod prelude {
    pub use crate::capability::{CapabilityGroup, FlattenGroups};
    pub use crate::effect::{CapabilityRouter, Effect, EffectExt};
    pub use crate::provider::Provider;
    pub use crate::task::Task;
    pub use crate::variant::{Concat, Extract, Never, S, Variant, VariantOf, Z};
    pub use fx_macros::{ability, effect, effectful};
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod integration_tests {
    use crate::prelude::*;

    // Manual implementation of State ability for testing
    // (before the macro is implemented)

    // Provider trait
    trait StateProvider<T> {
        fn get(&mut self) -> T;
        fn set(&mut self, value: T);
    }

    // Capability structs
    struct StateGet<T>(core::marker::PhantomData<T>);
    struct StateSet<T> {
        value: T,
    }

    // CapabilityGroup impls
    impl<T> CapabilityGroup for StateGet<T> {
        type Capabilities = Variant<StateGet<T>, Never>;
    }

    impl<T> CapabilityGroup for StateSet<T> {
        type Capabilities = Variant<StateSet<T>, Never>;
    }

    // Effect impls
    impl<T: Clone + Send> Effect for StateGet<T> {
        type Output = T;
    }

    impl<T: Send> Effect for StateSet<T> {
        type Output = ();
    }

    // Provider impls
    impl<T: Clone + Send, P: StateProvider<T> + Send> Provider<StateGet<T>> for P {
        type Output = T;

        async fn invoke(&mut self, _: StateGet<T>) -> T {
            self.get()
        }
    }

    impl<T: Send, P: StateProvider<T> + Send> Provider<StateSet<T>> for P {
        type Output = ();

        async fn invoke(&mut self, effect: StateSet<T>) {
            self.set(effect.value)
        }
    }

    // Capability group marker + builders
    struct State<T>(core::marker::PhantomData<T>);

    impl<T> State<T> {
        fn get() -> StateGet<T> {
            StateGet(core::marker::PhantomData)
        }

        fn set(value: T) -> StateSet<T> {
            StateSet { value }
        }
    }

    impl<T> CapabilityGroup for State<T> {
        type Capabilities = Variant<StateGet<T>, Variant<StateSet<T>, Never>>;
    }

    // Test provider implementation
    struct Counter {
        value: i32,
    }

    impl StateProvider<i32> for Counter {
        fn get(&mut self) -> i32 {
            self.value
        }

        fn set(&mut self, value: i32) {
            self.value = value;
        }
    }

    #[tokio::test]
    async fn test_state_get() {
        let mut counter = Counter { value: 42 };
        let result = State::<i32>::get().perform(&mut counter).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_state_set() {
        let mut counter = Counter { value: 0 };
        State::<i32>::set(42).perform(&mut counter).await;
        assert_eq!(counter.value, 42);
    }

    #[tokio::test]
    async fn test_state_get_set_sequence() {
        let mut counter = Counter { value: 10 };

        let value = State::<i32>::get().perform(&mut counter).await;
        assert_eq!(value, 10);

        State::<i32>::set(value + 5).perform(&mut counter).await;

        let new_value = State::<i32>::get().perform(&mut counter).await;
        assert_eq!(new_value, 15);
    }

    #[tokio::test]
    async fn test_task_with_effects() {
        use core::future::ready;

        // Use ready() future to avoid lifetime issues with async blocks
        // In practice, the effect! and #[effectful] macros handle this properly
        let task: Task<_, Variant<StateGet<i32>, Variant<StateSet<i32>, Never>>, i32> =
            Task::new(|counter: &mut Counter| {
                // Perform effects synchronously for this test
                let value = counter.value;
                counter.value = value * 2;
                ready(counter.value)
            });

        let mut counter = Counter { value: 21 };
        let result = task.perform(&mut counter).await;
        assert_eq!(result, 42);
    }
}
