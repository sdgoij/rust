//! Time via the PM server's `CLOCK_GETTIME` call.
//!
//! `Instant` is backed by `CLOCK_MONOTONIC`, `SystemTime` by
//! `CLOCK_REALTIME` (seconds since the Unix epoch).

use crate::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::from_secs(0));

fn clock_gettime(clock_id: i32) -> Duration {
    match minix_std::time::clock_gettime(clock_id) {
        Ok(ts) => Duration::new(ts.tv_sec.max(0) as u64, ts.tv_nsec.max(0) as u32),
        Err(_) => panic!("clock_gettime failed"),
    }
}

impl Instant {
    pub fn now() -> Instant {
        Instant(clock_gettime(minix_std::time::CLOCK_MONOTONIC))
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_sub(*other)?))
    }
}

impl SystemTime {
    pub const MAX: SystemTime = SystemTime(Duration::MAX);

    pub const MIN: SystemTime = SystemTime(Duration::ZERO);

    pub fn now() -> SystemTime {
        SystemTime(clock_gettime(minix_std::time::CLOCK_REALTIME))
    }

    pub fn from_secs(secs: u64) -> SystemTime {
        SystemTime(Duration::from_secs(secs))
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.0.checked_sub(other.0).ok_or_else(|| other.0 - self.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_sub(*other)?))
    }
}
