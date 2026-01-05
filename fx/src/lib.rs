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
pub use capability::{CapabilityGroup, FlattenGroups};
pub use effect::{CapabilityRouter, Effect, EffectExt};
pub use provider::Provider;
pub use task::{Effectful, PerformExt, Task};
pub use variant::{Concat, Extract, Never, Variant, VariantOf, S, Z};

/// Re-export of procedural macros.
pub use fx_macros::{ability, effect, effectful, perform};

/// Convenient prelude module for common imports.
pub mod prelude {
    pub use crate::capability::{CapabilityGroup, FlattenGroups};
    pub use crate::effect::{CapabilityRouter, Effect, EffectExt};
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

        // Manually compose effects - this is what effect! would expand to
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

        // Manually compose state effects
        let old = State::<String>::get().perform(&mut app).await;
        State::<String>::set(format!("{} modified", old))
            .perform(&mut app)
            .await;
        let result = State::<String>::get().perform(&mut app).await;

        assert_eq!(result, "initial modified");
    }

    // ===========================================
    // Test helper functions that use perform!
    // ===========================================

    // Note: The #[effectful] macro works best for simple cases.
    // For functions with captures, use async fn directly.

    async fn double_count_manual<P: CounterProvider + Send>(provider: &mut P) -> i32 {
        let count = Counter::get_count().perform(provider).await;
        Counter::add(count).perform(provider).await;
        count * 2
    }

    #[tokio::test]
    async fn test_manual_effectful() {
        let mut app = CounterApp { count: 5 };
        let result = double_count_manual(&mut app).await;
        assert_eq!(result, 10); // 5 * 2
        assert_eq!(app.count, 10); // 5 + 5
    }

    async fn append_suffix_manual<P: StateProvider<String> + Send>(
        provider: &mut P,
        suffix: String,
    ) -> String {
        let current = State::<String>::get().perform(provider).await;
        let new_value = format!("{}{}", current, suffix);
        State::<String>::set(new_value.clone())
            .perform(provider)
            .await;
        new_value
    }

    #[tokio::test]
    async fn test_manual_effectful_with_args() {
        let mut app = StateApp {
            value: "hello".to_string(),
        };
        let result = append_suffix_manual(&mut app, " world".to_string()).await;
        assert_eq!(result, "hello world");
        assert_eq!(app.value, "hello world");
    }
}
