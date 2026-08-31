//! Timing primitives shared by native capture and synthetic black frames.

use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub(super) const TICKS_PER_SECOND_100NS: i64 = 10_000_000;
pub(super) const BLACK_HEARTBEAT_INTERVAL_100NS: i64 = 5_000_000;

#[derive(Clone, Copy)]
pub(super) struct QpcClock {
    frequency: i64,
}

impl QpcClock {
    pub(super) fn new() -> Result<Self, String> {
        let mut frequency = 0i64;
        if unsafe { QueryPerformanceFrequency(&mut frequency) } == 0 || frequency <= 0 {
            return Err("QueryPerformanceFrequency failed".to_string());
        }
        Ok(Self { frequency })
    }

    pub(super) fn now_100ns(&self) -> Result<i64, String> {
        let mut counter = 0i64;
        if unsafe { QueryPerformanceCounter(&mut counter) } == 0 {
            return Err("QueryPerformanceCounter failed".to_string());
        }
        qpc_ticks_to_100ns(counter, self.frequency)
    }
}

pub(super) fn qpc_ticks_to_100ns(ticks: i64, frequency: i64) -> Result<i64, String> {
    if frequency <= 0 {
        return Err("QPC frequency must be positive".to_string());
    }
    let converted = i128::from(ticks)
        .checked_mul(i128::from(TICKS_PER_SECOND_100NS))
        .ok_or_else(|| "QPC conversion overflowed".to_string())?
        / i128::from(frequency);
    i64::try_from(converted).map_err(|_| "QPC timestamp does not fit in i64".to_string())
}

#[derive(Default)]
pub(super) struct TimestampReservation {
    last_timestamp: Option<i64>,
}

impl TimestampReservation {
    pub(super) fn reserve(&mut self, timestamp: i64) -> Option<i64> {
        if self.last_timestamp.is_some_and(|last| timestamp <= last) {
            return None;
        }
        self.last_timestamp = Some(timestamp);
        Some(timestamp)
    }
}

pub(super) struct FrameGate {
    target_interval_100ns: i64,
    jitter_tolerance_100ns: i64,
    next_due_timestamp: Option<i64>,
    last_accepted_timestamp: Option<i64>,
}

impl FrameGate {
    pub(super) fn new(frame_rate: u32) -> Self {
        let target_interval_100ns = (TICKS_PER_SECOND_100NS / i64::from(frame_rate.max(1))).max(1);
        Self {
            target_interval_100ns,
            jitter_tolerance_100ns: target_interval_100ns / 2,
            next_due_timestamp: None,
            last_accepted_timestamp: None,
        }
    }

    pub(super) fn accept(&mut self, timestamp: i64) -> bool {
        if self
            .last_accepted_timestamp
            .is_some_and(|last| timestamp <= last)
        {
            return false;
        }

        let Some(next_due) = self.next_due_timestamp else {
            self.last_accepted_timestamp = Some(timestamp);
            self.next_due_timestamp = Some(timestamp.saturating_add(self.target_interval_100ns));
            return true;
        };
        if timestamp.saturating_add(self.jitter_tolerance_100ns) < next_due {
            return false;
        }

        let intervals_elapsed = timestamp
            .saturating_sub(next_due)
            .max(0)
            .saturating_div(self.target_interval_100ns)
            .saturating_add(1);
        self.next_due_timestamp = Some(
            next_due.saturating_add(self.target_interval_100ns.saturating_mul(intervals_elapsed)),
        );
        self.last_accepted_timestamp = Some(timestamp);
        true
    }
}

pub(super) fn black_heartbeat_due(last_timestamp: i64, now_timestamp: i64) -> bool {
    now_timestamp.saturating_sub(last_timestamp) >= BLACK_HEARTBEAT_INTERVAL_100NS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpc_conversion_uses_wide_arithmetic() {
        assert_eq!(qpc_ticks_to_100ns(15_000_000, 3_000_000), Ok(50_000_000));
        assert_eq!(
            qpc_ticks_to_100ns(i64::MAX / 2, i64::MAX / 4),
            Ok(20_000_000)
        );
        assert!(qpc_ticks_to_100ns(1, 0).is_err());
    }

    #[test]
    fn timestamp_reservation_rejects_duplicates_and_backwards_values() {
        let mut reservation = TimestampReservation::default();
        assert_eq!(reservation.reserve(10), Some(10));
        assert_eq!(reservation.reserve(10), None);
        assert_eq!(reservation.reserve(9), None);
        assert_eq!(reservation.reserve(11), Some(11));
    }

    #[test]
    fn frame_gate_limits_average_cadence() {
        let mut gate = FrameGate::new(30);
        let accepted = (0..=100)
            .filter(|step| gate.accept(i64::from(*step) * 100_000))
            .count();
        assert!(accepted <= 31);
        assert!(!gate.accept(10_000_000));
    }

    #[test]
    fn frame_gate_tolerates_bursty_delivery_below_target_average() {
        let mut gate = FrameGate::new(60);
        let timestamps = [0, 100_000, 360_000, 460_000, 720_000];
        assert!(timestamps
            .into_iter()
            .all(|timestamp| gate.accept(timestamp)));
    }

    #[test]
    fn black_heartbeat_is_due_after_half_a_second() {
        assert!(!black_heartbeat_due(10, 5_000_009));
        assert!(black_heartbeat_due(10, 5_000_010));
    }
}
