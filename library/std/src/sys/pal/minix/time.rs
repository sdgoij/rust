//! Time via the PM server's `CLOCK_GETTIME` call.
//!
//! `Instant` is backed by `CLOCK_MONOTONIC`, `SystemTime` by
//! `CLOCK_REALTIME` (seconds since the Unix epoch).

use crate::sys::syscall;
use crate::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::from_secs(0));

fn clock_gettime(clock_id: i32) -> Duration {
    let mut msg = [0u8; 64];
    // PM_CLOCK_GETTIME (34) uses the C `mess_lc_pm_time` layout: m_type@4,
    // clk_id @ payload 8 (message byte 16). The reply `mess_pm_lc_time` puts
    // sec @ payload 0 (message byte 8), nsec @ payload 8 (byte 16); the
    // payload starts at message byte 8 (`m_source` + `m_type`).
    syscall::msg_set_i32(&mut msg, 4, syscall::PM_CLOCK_GETTIME);
    syscall::msg_set_i32(&mut msg, 16, clock_id);
    // SAFETY: `msg` is a valid message buffer.
    match unsafe { syscall::pm_call(&mut msg) } {
        Ok(_) => {
            let sec = syscall::msg_i64(&msg, 8);
            let nsec = syscall::msg_i64(&msg, 16);
            Duration::new(sec.max(0) as u64, nsec.max(0) as u32)
        }
        Err(_) => panic!("clock_gettime failed"),
    }
}

impl Instant {
    pub fn now() -> Instant {
        Instant(clock_gettime(syscall::CLOCK_MONOTONIC))
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
        SystemTime(clock_gettime(syscall::CLOCK_REALTIME))
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
