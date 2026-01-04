# FX - Rust Effect System

A Rust effect system with declarative abilities and type-safe capability composition.

## Features

- **Declarative abilities** via the `ability!` macro
- **Type-safe capability composition** using `Variant` (coproduct types)
- **Async-ready** effects with `.perform(provider)` method
- **Inline effectful code** with the `effect!{}` block macro
- **Function-level effects** with the `#[effectful]` attribute macro

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
fx = "0.1"
```

## Quick Start

```rust
use fx::prelude::*;

// Define abilities
ability! {
    pub State<T> {
        fn get() -> T;
        fn set(value: T) -> ()
    }
}

// Implement the provider trait
struct MyApp { value: i32 }

impl StateProvider<i32> for MyApp {
    fn get(&mut self) -> i32 { self.value }
    fn set(&mut self, value: i32) { self.value = value; }
}

// Use effects
async fn example() {
    let mut app = MyApp { value: 42 };
    let x = State::<i32>::get().perform(&mut app).await;
    println!("Value: {}", x);
}
```

## Core Concepts

### Effects

Effects are operations that require external capabilities to execute. Each effect specifies its required capabilities and output type.

### Capabilities

Capabilities are individual operations that a provider can perform. Multiple capabilities are composed using `Variant` types.

### Providers

Providers implement the actual logic for handling capabilities. A single provider can implement multiple capability handlers.

### Tasks

Tasks wrap effectful closures, allowing complex effect compositions to be represented as values.

## Macros

### `ability!`

Declaratively define abilities with their capabilities:

```rust
ability! {
    pub Logger {
        fn log<M: Into<String>>(msg: M) -> ()
    }
}
```

### `effect!`

Create inline effectful code blocks:

```rust
let my_effect = effect! {
    let value = yield State::<i32>::get();
    yield Logger::log(value.to_string());
    value
};
```

### `#[effectful]`

Mark async functions as effectful:

```rust
#[effectful(State<i32>, Logger)]
async fn read_and_log() -> i32 {
    let value = yield State::<i32>::get();
    yield Logger::log(value.to_string());
    value
}
```

## Development

### Prerequisites

- Rust 1.85 or later (for edition 2024)
- Nix (optional, for reproducible development environment)

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Using Nix

Enter the development shell:

```bash
nix develop
```

Build with Nix:

```bash
nix build
```

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
