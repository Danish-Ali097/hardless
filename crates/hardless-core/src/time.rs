//! Time: [`Millis`] timestamps and the injectable [`Clock`] (FR-05).

use core::fmt;
use core::ops::Add;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A timestamp or duration in milliseconds.
///
/// The engine never reads a clock: whoever drives a device passes `now` in. The epoch is the
/// caller's choice (the server uses time since start-up; tests usually start at zero).
///
/// ```
/// use hardless_core::Millis;
/// assert_eq!(Millis(1_000) + 300, Millis(1_300));
/// assert_eq!(Millis(1_300).since(Millis(1_000)), 300);
/// ```
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct Millis(pub u64);

impl Millis {
    /// The zero timestamp.
    pub const ZERO: Millis = Millis(0);

    /// Milliseconds elapsed from `earlier` to `self`, or zero if `earlier` is later.
    ///
    /// ```
    /// use hardless_core::Millis;
    /// assert_eq!(Millis(5).since(Millis(9)), 0);
    /// ```
    pub const fn since(self, earlier: Millis) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<u64> for Millis {
    type Output = Millis;

    /// Saturating addition, so a deadline far in the future never wraps around.
    fn add(self, ms: u64) -> Millis {
        Millis(self.0.saturating_add(ms))
    }
}

impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

/// A source of time, injected into the [`Registry`](crate::Registry) (FR-05).
///
/// The server passes a system clock; tests pass a [`FakeClock`]. Any
/// `Fn() -> Millis + Send + Sync` closure is also a clock.
///
/// ```
/// use hardless_core::{Clock, Millis};
/// let fixed = || Millis(42);
/// assert_eq!(fixed.now(), Millis(42));
/// ```
pub trait Clock: Send + Sync {
    /// The current time.
    fn now(&self) -> Millis;
}

impl<F> Clock for F
where
    F: Fn() -> Millis + Send + Sync,
{
    fn now(&self) -> Millis {
        self()
    }
}

/// A manually driven clock for tests. Clones share the same time.
///
/// ```
/// use hardless_core::{Clock, FakeClock, Millis};
/// let clock = FakeClock::new();
/// let handle = clock.clone(); // e.g. give `clock` to a Registry and keep `handle`
/// handle.advance(300);
/// assert_eq!(clock.now(), Millis(300));
/// ```
#[derive(Clone, Debug, Default)]
pub struct FakeClock(Arc<AtomicU64>);

impl FakeClock {
    /// A fake clock starting at [`Millis::ZERO`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A fake clock starting at `start`.
    pub fn at(start: Millis) -> Self {
        Self(Arc::new(AtomicU64::new(start.0)))
    }

    /// Sets the time. Setting it backwards is allowed; the registry keeps its own timestamps
    /// monotonic.
    pub fn set(&self, now: Millis) {
        self.0.store(now.0, Ordering::SeqCst);
    }

    /// Moves the time forward by `ms` milliseconds (saturating).
    pub fn advance(&self, ms: u64) {
        let mut current = self.0.load(Ordering::SeqCst);
        loop {
            let next = current.saturating_add(ms);
            match self
                .0
                .compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Millis {
        Millis(self.0.load(Ordering::SeqCst))
    }
}
