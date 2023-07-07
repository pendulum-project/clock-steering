//! Logic for steering OS clocks, aimed at NTP and PTP.
//!
//! This code is used in our implementations of NTP [ntpd-rs](https://github.com/pendulum-project/ntpd-rs) and PTP [statime](https://github.com/pendulum-project/statime).
use std::time::Duration;

#[cfg(unix)]
pub mod unix;

/// A moment in time.
///
/// The format makes it easy to convert into libc data structures, and supports subnanoseconds that
/// certain hardware can provide for additional precision. The value is an offset from the [unix epoch](https://en.wikipedia.org/wiki/Unix_time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp {
    pub seconds: libc::time_t,
    pub nanos: u32,
    pub subnanos: u32,
}

/// Indicate whether a leap second must be applied
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum LeapIndicator {
    /// No leap second warning
    #[default]
    NoWarning,
    /// Last minute of the day has 61 seconds
    Leap61,
    /// Last minute of the day has 59 seconds
    Leap59,
    /// Unknown leap second status (the clock is unsynchronized)
    Unknown,
}

/// Trait for reading information from and modifying an OS clock
pub trait Clock {
    type Error: std::error::Error;

    /// Get the current time.
    fn now(&self) -> Result<Timestamp, Self::Error>;

    /// Get the clock's resolution.
    ///
    /// The output [`Timestamp`] will be all zeros when the resolution is
    /// unavailable.
    fn resolution(&self) -> Result<Timestamp, Self::Error>;

    /// Change the frequency of the clock.
    /// Returns the time at which the change was applied.
    ///
    /// The unit of the input is seconds (of drift) per second.
    fn set_frequency(&self, frequency: f64) -> Result<Timestamp, Self::Error>;

    /// Change the current time of the clock by an offset.
    /// Returns the time at which the change was applied.
    fn step_clock(&self, offset: Duration) -> Result<Timestamp, Self::Error>;

    /// Change the indicators for upcoming leap seconds.
    fn set_leap_seconds(&self, leap_status: LeapIndicator) -> Result<(), Self::Error>;

    /// Provide the system with the current best estimates for the statistical
    /// error of the clock, and the maximum deviation due to frequency error and
    /// distance to the root clock.
    fn error_estimate_update(
        &self,
        estimated_error: Duration,
        maximum_error: Duration,
    ) -> Result<(), Self::Error>;
}
