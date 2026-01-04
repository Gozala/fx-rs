//! Task wrapper for effectful closures.
//!
//! The `Task` type wraps a closure that performs effects, allowing it
//! to be composed and executed through the effect system.

use crate::capability::CapabilityGroup;
use crate::provider::Provider;
use core::future::Future;
use core::marker::PhantomData;

/// A wrapper for effectful closures.
///
/// `Task` encapsulates a closure that takes a provider reference and returns
/// a future. This allows effectful code to be represented as a value that
/// can be composed and passed around.
///
/// # Type Parameters
///
/// - `F`: The closure type.
/// - `C`: The capabilities required by this task (a `Variant` type).
/// - `O`: The output type of the task.
///
/// # Example
///
/// ```ignore
/// let task = Task::new(|provider: &mut P| async move {
///     let value = State::<i32>::get().perform(provider).await;
///     value * 2
/// });
/// ```
pub struct Task<F, C, O> {
    f: F,
    _marker: PhantomData<fn() -> (C, O)>,
}

impl<F, C, O> Task<F, C, O> {
    /// Create a new task from a closure.
    ///
    /// The closure should take a mutable reference to a provider and
    /// return a future that produces the output.
    #[inline]
    pub const fn new(f: F) -> Self {
        Task {
            f,
            _marker: PhantomData,
        }
    }

    /// Get the inner closure.
    #[inline]
    pub fn into_inner(self) -> F {
        self.f
    }

    /// Execute the task with a provider.
    ///
    /// This method invokes the wrapped closure with the provider
    /// and returns the resulting future.
    #[inline]
    pub fn perform<P, Fut>(self, provider: &mut P) -> Fut
    where
        F: FnOnce(&mut P) -> Fut,
        Fut: Future<Output = O>,
        P: Provider<C>,
    {
        (self.f)(provider)
    }
}

impl<F, C, O> CapabilityGroup for Task<F, C, O> {
    type Capabilities = C;
}

/// Helper trait for creating tasks with inferred capability types.
///
/// This trait enables ergonomic task creation when the capability
/// requirements can be inferred from context.
pub trait IntoTask<C, O> {
    /// The closure type.
    type Closure;

    /// Convert into a task.
    fn into_task(self) -> Task<Self::Closure, C, O>;
}

impl<F, C, O> IntoTask<C, O> for Task<F, C, O> {
    type Closure = F;

    #[inline]
    fn into_task(self) -> Task<F, C, O> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::Never;
    use core::future::ready;

    struct TestProvider {
        value: i32,
    }

    // Note: Provider<Never> is automatically implemented via blanket impl

    #[tokio::test]
    async fn test_task_basic() {
        // Use ready() future to avoid lifetime issues with async blocks
        let task: Task<_, Never, i32> = Task::new(|provider: &mut TestProvider| {
            let result = provider.value * 2;
            ready(result)
        });

        let mut provider = TestProvider { value: 21 };
        let result = task.perform(&mut provider).await;
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_task_with_mutation() {
        let task: Task<_, Never, ()> = Task::new(|provider: &mut TestProvider| {
            provider.value += 10;
            ready(())
        });

        let mut provider = TestProvider { value: 32 };
        task.perform(&mut provider).await;
        assert_eq!(provider.value, 42);
    }
}
