//! Time helpers: `180.ms()`, `1.5.secs()`.

pub use std::time::{Duration, Instant};

/// Extension methods to build [`Duration`]s from numeric literals.
///
/// ```
/// use kova_core::DurationExt;
/// assert_eq!(180.ms().as_millis(), 180);
/// assert_eq!(1.5.secs().as_millis(), 1500);
/// ```
pub trait DurationExt {
    fn ms(self) -> Duration;
    fn secs(self) -> Duration;
}

macro_rules! impl_int {
    ($($t:ty),*) => {$(
        impl DurationExt for $t {
            fn ms(self) -> Duration { Duration::from_millis(self.max(0 as $t) as u64) }
            fn secs(self) -> Duration { Duration::from_secs(self.max(0 as $t) as u64) }
        }
    )*};
}
impl_int!(i32, i64, u32, u64, usize);

impl DurationExt for f32 {
    fn ms(self) -> Duration {
        Duration::from_secs_f32(self.max(0.0) / 1000.0)
    }
    fn secs(self) -> Duration {
        Duration::from_secs_f32(self.max(0.0))
    }
}

impl DurationExt for f64 {
    fn ms(self) -> Duration {
        Duration::from_secs_f64(self.max(0.0) / 1000.0)
    }
    fn secs(self) -> Duration {
        Duration::from_secs_f64(self.max(0.0))
    }
}
