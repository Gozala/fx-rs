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
#![allow(unused_extern_crates)]

// Self-alias to allow internal tests to use `fx::` paths
extern crate self as fx;

pub mod capability;
pub mod effect;
pub mod provider;
pub mod task;
pub mod variant;

// Re-export main types at crate root
pub use capability::{CanProvide, CapabilityGroup, FlattenGroups, Handle};
pub use effect::{CapabilityRouter, Effect, EffectExt, PerformIn};
pub use provider::Provider;
pub use task::{Effectful, PerformExt, Task};
pub use variant::{Concat, Extract, Never, Variant, VariantOf, S, Z};

/// Re-export of procedural macros.
pub use fx_macros::{ability, effect, effectful, perform};

/// Convenient prelude module for common imports.
pub mod prelude {
    pub use crate::capability::{CapabilityGroup, FlattenGroups};
    pub use crate::effect::{CapabilityRouter, Effect, EffectExt, PerformIn};
    pub use crate::provider::Provider;
    pub use crate::task::{Effectful, PerformExt, Task};
    pub use crate::variant::{Concat, Extract, Never, S, Variant, VariantOf, Z};
    pub use fx_macros::{ability, effect, effectful, perform};
}

#[cfg(test)]
#[allow(clippy::type_complexity)]
mod macro_tests {
    use crate::prelude::*;

    // ===========================================
    // Test the ability! macro
    // ===========================================

    ability! {
        pub Counter {
            fn get_count() -> i32;
            fn increment() -> ();
            fn add(amount: i32) -> i32
        }
    }

    // Provider implementation
    struct CounterApp {
        count: i32,
    }

    impl CounterProvider for CounterApp {
        fn get_count(&mut self) -> i32 {
            self.count
        }

        fn increment(&mut self) {
            self.count += 1;
        }

        fn add(&mut self, amount: i32) -> i32 {
            self.count += amount;
            self.count
        }
    }

    #[tokio::test]
    async fn test_ability_macro_get() {
        let mut app = CounterApp { count: 42 };
        let result = Counter::get_count().perform(&mut app).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_ability_macro_increment() {
        let mut app = CounterApp { count: 10 };
        Counter::increment().perform(&mut app).await;
        assert_eq!(app.count, 11);
    }

    #[tokio::test]
    async fn test_ability_macro_with_args() {
        let mut app = CounterApp { count: 10 };
        let result = Counter::add(5).perform(&mut app).await;
        assert_eq!(result, 15);
        assert_eq!(app.count, 15);
    }

    // ===========================================
    // Test ability! with generics
    // ===========================================

    ability! {
        pub State<T> {
            fn get() -> T;
            fn set(value: T) -> ()
        }
    }

    struct StateApp {
        value: String,
    }

    impl StateProvider<String> for StateApp {
        fn get(&mut self) -> String {
            self.value.clone()
        }

        fn set(&mut self, value: String) {
            self.value = value;
        }
    }

    #[tokio::test]
    async fn test_ability_macro_generic_get() {
        let mut app = StateApp {
            value: "hello".to_string(),
        };
        let result = State::<String>::get().perform(&mut app).await;
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_ability_macro_generic_set() {
        let mut app = StateApp {
            value: "hello".to_string(),
        };
        State::<String>::set("world".to_string())
            .perform(&mut app)
            .await;
        assert_eq!(app.value, "world");
    }

    // ===========================================
    // Test effect composition using direct provider calls
    // ===========================================

    #[tokio::test]
    async fn test_effect_composition() {
        let mut app = CounterApp { count: 10 };

        // Manually compose effects
        let current = Counter::get_count().perform(&mut app).await;
        Counter::add(current).perform(&mut app).await;
        let result = current * 2;

        assert_eq!(result, 20); // 10 * 2
        assert_eq!(app.count, 20); // 10 + 10
    }

    #[tokio::test]
    async fn test_state_composition() {
        let mut app = StateApp {
            value: "initial".to_string(),
        };

        let old = State::<String>::get().perform(&mut app).await;
        State::<String>::set(format!("{} modified", old))
            .perform(&mut app)
            .await;
        let result = State::<String>::get().perform(&mut app).await;

        assert_eq!(result, "initial modified");
    }

    // ===========================================
    // Test the effect! macro
    // ===========================================

    #[tokio::test]
    async fn test_effect_macro_simple() {
        let mut app = CounterApp { count: 10 };

        // effect! takes provider and body, returns an async block
        let result = effect!(&mut app, {
            let current = yield Counter::get_count();
            yield Counter::add(current);
            current * 2
        }).await;

        assert_eq!(result, 20); // 10 * 2
        assert_eq!(app.count, 20); // 10 + 10
    }

    #[tokio::test]
    async fn test_effect_macro_with_state() {
        let mut app = StateApp {
            value: "initial".to_string(),
        };

        let result = effect!(&mut app, {
            let old = yield State::<String>::get();
            yield State::<String>::set(format!("{} modified", old));
            yield State::<String>::get()
        }).await;

        assert_eq!(result, "initial modified");
    }

    // ===========================================
    // Test the #[effectful] attribute macro
    // ===========================================

    // #[effectful] generates a struct with .perform() method
    #[effectful(Counter)]
    fn double_count() -> i32 {
        let count = perform!(Counter::get_count());
        perform!(Counter::add(count));
        count * 2
    }

    #[tokio::test]
    async fn test_effectful_macro() {
        let mut app = CounterApp { count: 5 };
        // Returns a struct, call .perform(&mut provider).await
        let result = double_count().perform(&mut app).await;
        assert_eq!(result, 10); // 5 * 2
        assert_eq!(app.count, 10); // 5 + 5
    }

    // Test effectful with additional arguments
    #[effectful(State<String>)]
    fn append_suffix(suffix: String) -> String {
        let current = perform!(State::<String>::get());
        let new_value = format!("{}{}", current, suffix);
        perform!(State::<String>::set(new_value.clone()));
        new_value
    }

    #[tokio::test]
    async fn test_effectful_macro_with_args() {
        let mut app = StateApp {
            value: "hello".to_string(),
        };
        // Pass args to fn, then .perform() with provider
        let result = append_suffix(" world".to_string()).perform(&mut app).await;
        assert_eq!(result, "hello world");
        assert_eq!(app.value, "hello world");
    }

    // ===========================================
    // Test complex effectful operations
    // ===========================================

    #[effectful(Counter)]
    fn complex_operation() -> i32 {
        let x = perform!(Counter::get_count());
        perform!(Counter::add(x));
        perform!(Counter::add(x));
        x * 3
    }

    #[tokio::test]
    async fn test_effectful_complex() {
        let mut app = CounterApp { count: 10 };
        let result = complex_operation().perform(&mut app).await;
        assert_eq!(result, 30); // 10 * 3
        assert_eq!(app.count, 30); // 10 + 10 + 10
    }

}
