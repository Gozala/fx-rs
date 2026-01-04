//! Capability group traits for organizing effects.
//!
//! This module provides traits for grouping capabilities and flattening
//! nested capability groups into flat variant types.

use crate::variant::{Concat, Never, Variant};

/// A trait that links a capability marker type to its individual capabilities.
///
/// Types implementing this trait define a set of capabilities that they require
/// or provide. The `Capabilities` associated type is typically a `Variant` of
/// all the individual capability types.
///
/// # Example
///
/// ```ignore
/// struct MyEffect;
///
/// impl CapabilityGroup for MyEffect {
///     type Capabilities = Variant<ReadCapability, Variant<WriteCapability, Never>>;
/// }
/// ```
pub trait CapabilityGroup {
    /// The variant type containing all capabilities in this group.
    type Capabilities;
}

/// A trait for flattening nested capability groups into a flat variant.
///
/// When composing multiple capability groups, this trait expands each group
/// to its individual capabilities and concatenates them into a single flat
/// variant type.
///
/// # Example
///
/// Given:
/// - `State<T>` with capabilities `Variant<StateGet<T>, Variant<StateSet<T>, Never>>`
/// - `Logger` with capabilities `Variant<LoggerLog, Never>`
///
/// `FlattenGroups` on `Variant<State<T>, Variant<Logger, Never>>` produces:
/// `Variant<StateGet<T>, Variant<StateSet<T>, Variant<LoggerLog, Never>>>`
pub trait FlattenGroups {
    /// The flattened variant type containing all individual capabilities.
    type Flattened;
}

impl FlattenGroups for Never {
    type Flattened = Never;
}

impl<H, T> FlattenGroups for Variant<H, T>
where
    H: CapabilityGroup,
    T: FlattenGroups,
    H::Capabilities: Concat<T::Flattened>,
{
    type Flattened = <H::Capabilities as Concat<T::Flattened>>::Output;
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;

    // Test capability types
    struct CapA;
    struct CapB;
    struct CapC;

    // Test groups
    struct GroupAB;
    struct GroupC;

    impl CapabilityGroup for GroupAB {
        type Capabilities = Variant<CapA, Variant<CapB, Never>>;
    }

    impl CapabilityGroup for GroupC {
        type Capabilities = Variant<CapC, Never>;
    }

    #[test]
    fn test_flatten_single_group() {
        // Variant<GroupAB, Never> should flatten to Variant<CapA, Variant<CapB, Never>>
        type Groups = Variant<GroupAB, Never>;
        type Flattened = <Groups as FlattenGroups>::Flattened;

        // Type-level assertion - if this compiles, the types match
        fn _assert_type(_: Flattened) -> Variant<CapA, Variant<CapB, Never>> {
            unreachable!()
        }
    }

    #[test]
    fn test_flatten_multiple_groups() {
        // Variant<GroupAB, Variant<GroupC, Never>> should flatten to
        // Variant<CapA, Variant<CapB, Variant<CapC, Never>>>
        type Groups = Variant<GroupAB, Variant<GroupC, Never>>;
        type Flattened = <Groups as FlattenGroups>::Flattened;

        // Type-level assertion
        fn _assert_type(_: Flattened) -> Variant<CapA, Variant<CapB, Variant<CapC, Never>>> {
            unreachable!()
        }
    }
}
