//! Variant (coproduct) types for type-safe capability composition.
//!
//! This module provides a heterogeneous sum type (`Variant`) that can hold
//! one of several types, along with type-level indices for compile-time
//! type selection.

use core::marker::PhantomData;

/// An uninhabited type representing an impossible case.
///
/// Used as the base case for `Variant` chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Never {}

impl Never {
    /// Eliminate a `Never` value - this can never actually be called
    /// since `Never` has no inhabitants.
    #[inline]
    pub fn absurd<T>(self) -> T {
        match self {}
    }
}

/// A heterogeneous sum type (coproduct) that can hold either a value
/// of type `H` or a value wrapped in type `T`.
///
/// Used to compose multiple capability types into a single type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant<H, T> {
    /// Contains a value of the head type `H`.
    Here(H),
    /// Contains a value wrapped in the tail type `T`.
    There(T),
}

// --- Type-level indices ---

/// Type-level zero, representing the first position in a `Variant` chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Z;

/// Type-level successor, representing position `N + 1` in a `Variant` chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct S<N>(PhantomData<N>);

// --- VariantOf trait: inject value into variant ---

/// Trait for injecting a value of type `T` into a `Variant` at a type-level index.
///
/// The `Index` parameter determines which position in the variant chain
/// the value will be placed.
pub trait VariantOf<T, Index> {
    /// Inject a value into this variant type.
    fn of(value: T) -> Self;
}

impl<T, Tail> VariantOf<T, Z> for Variant<T, Tail> {
    #[inline]
    fn of(value: T) -> Self {
        Variant::Here(value)
    }
}

impl<Head, Tail, T, N> VariantOf<T, S<N>> for Variant<Head, Tail>
where
    Tail: VariantOf<T, N>,
{
    #[inline]
    fn of(value: T) -> Self {
        Variant::There(Tail::of(value))
    }
}

// --- Extract trait: extract value from variant ---

/// Trait for extracting a value of type `T` from a `Variant`.
///
/// On success, returns the extracted value. On failure (the variant
/// holds a different type), returns the remainder variant.
pub trait Extract<T, Index> {
    /// The variant type remaining after extracting `T`.
    type Remainder;

    /// Attempt to extract a value of type `T`.
    ///
    /// Returns `Ok(T)` if successful, or `Err(Remainder)` if the variant
    /// holds a different type.
    fn extract(self) -> Result<T, Self::Remainder>;
}

impl<T, Tail> Extract<T, Z> for Variant<T, Tail> {
    type Remainder = Tail;

    #[inline]
    fn extract(self) -> Result<T, Tail> {
        match self {
            Variant::Here(t) => Ok(t),
            Variant::There(tail) => Err(tail),
        }
    }
}

impl<Head, Tail, T, N> Extract<T, S<N>> for Variant<Head, Tail>
where
    Tail: Extract<T, N>,
{
    type Remainder = Variant<Head, Tail::Remainder>;

    #[inline]
    fn extract(self) -> Result<T, Self::Remainder> {
        match self {
            Variant::Here(head) => Err(Variant::Here(head)),
            Variant::There(tail) => tail.extract().map_err(Variant::There),
        }
    }
}

// --- Concat trait: concatenate two variants ---

/// Trait for concatenating two variant types.
///
/// This is used to combine capability sets from multiple effects.
pub trait Concat<Other> {
    /// The resulting variant type after concatenation.
    type Output;

    /// Concatenate this variant with another, wrapping the contained value.
    fn concat(self) -> Self::Output;
}

impl<Other> Concat<Other> for Never {
    type Output = Other;

    #[inline]
    fn concat(self) -> Other {
        self.absurd()
    }
}

impl<H, T, Other> Concat<Other> for Variant<H, T>
where
    T: Concat<Other>,
{
    type Output = Variant<H, T::Output>;

    #[inline]
    fn concat(self) -> Self::Output {
        match self {
            Variant::Here(h) => Variant::Here(h),
            Variant::There(t) => Variant::There(t.concat()),
        }
    }
}

// --- Inherent methods on Variant ---

impl<H, T> Variant<H, T> {
    /// Inject a value into this variant type.
    ///
    /// The type-level index is inferred from the target type.
    #[inline]
    pub fn of<V, Index>(value: V) -> Self
    where
        Self: VariantOf<V, Index>,
    {
        <Self as VariantOf<V, Index>>::of(value)
    }

    /// Attempt to extract a value of type `V` from this variant.
    ///
    /// Returns `Ok(V)` if successful, or `Err(Remainder)` if the variant
    /// holds a different type.
    #[inline]
    pub fn extract<V, Index>(self) -> Result<V, <Self as Extract<V, Index>>::Remainder>
    where
        Self: Extract<V, Index>,
    {
        <Self as Extract<V, Index>>::extract(self)
    }

    /// Map over the head type of this variant.
    #[inline]
    pub fn map_here<F, H2>(self, f: F) -> Variant<H2, T>
    where
        F: FnOnce(H) -> H2,
    {
        match self {
            Variant::Here(h) => Variant::Here(f(h)),
            Variant::There(t) => Variant::There(t),
        }
    }

    /// Map over the tail type of this variant.
    #[inline]
    pub fn map_there<F, T2>(self, f: F) -> Variant<H, T2>
    where
        F: FnOnce(T) -> T2,
    {
        match self {
            Variant::Here(h) => Variant::Here(h),
            Variant::There(t) => Variant::There(f(t)),
        }
    }
}

impl<H> Variant<H, Never> {
    /// Unwrap a single-element variant.
    ///
    /// This is safe because the tail is `Never`, so it must be `Here`.
    #[inline]
    pub fn unwrap(self) -> H {
        match self {
            Variant::Here(h) => h,
            Variant::There(never) => never.absurd(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type V3 = Variant<i32, Variant<String, Variant<bool, Never>>>;

    #[test]
    fn test_variant_of_first() {
        let v: V3 = Variant::of(42i32);
        assert!(matches!(v, Variant::Here(42)));
    }

    #[test]
    fn test_variant_of_second() {
        let v: V3 = Variant::of(String::from("hello"));
        assert!(matches!(v, Variant::There(Variant::Here(_))));
    }

    #[test]
    fn test_variant_of_third() {
        let v: V3 = Variant::of(true);
        assert!(matches!(
            v,
            Variant::There(Variant::There(Variant::Here(true)))
        ));
    }

    #[test]
    fn test_extract_success() {
        let v: V3 = Variant::of(42i32);
        let result: Result<i32, _> = v.extract();
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_extract_failure() {
        let v: V3 = Variant::of(String::from("hello"));
        let result: Result<i32, _> = v.extract();
        assert!(result.is_err());
    }

    #[test]
    fn test_single_variant_unwrap() {
        let v: Variant<i32, Never> = Variant::of(42i32);
        assert_eq!(v.unwrap(), 42);
    }

    #[test]
    fn test_concat() {
        type A = Variant<i32, Never>;
        type B = Variant<String, Never>;
        type AB = <A as Concat<B>>::Output;

        let a: A = Variant::of(42i32);
        let ab: AB = a.concat();
        assert!(matches!(ab, Variant::Here(42)));
    }
}
