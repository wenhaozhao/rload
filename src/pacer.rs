use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Pacer {
    interval: Duration,
    next: Mutex<Instant>,
}

pub struct TimestampPacer {
    speed: f64,
    state: Mutex<TimestampState>,
}

struct TimestampState {
    previous_micros: Option<i64>,
    next: Instant,
}

impl Pacer {
    pub fn new(rate: u64, started: Instant) -> Self {
        debug_assert!(rate > 0);
        Self {
            interval: Duration::from_secs_f64(1.0 / rate as f64),
            next: Mutex::new(started),
        }
    }

    pub fn reserve(&self, now: Instant) -> Instant {
        let mut next = self
            .next
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let scheduled = (*next).max(now);
        *next = scheduled + self.interval;
        scheduled
    }
}

impl TimestampPacer {
    pub fn new(speed: f64, started: Instant) -> Self {
        Self {
            speed,
            state: Mutex::new(TimestampState {
                previous_micros: None,
                next: started,
            }),
        }
    }

    pub fn reserve(&self, timestamp_micros: i64, now: Instant) -> Instant {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let gap = state
            .previous_micros
            .filter(|previous| timestamp_micros >= *previous)
            .map_or(0, |previous| timestamp_micros - previous);
        state.next =
            state.next.max(now) + Duration::from_secs_f64(gap as f64 / 1_000_000.0 / self.speed);
        state.previous_micros = Some(timestamp_micros);
        state.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_evenly_spaced_global_slots() {
        let started = Instant::now();
        let pacer = Pacer::new(4, started);

        let slots: Vec<_> = (0..3).map(|_| pacer.reserve(started)).collect();

        assert_eq!(slots[0], started);
        assert_eq!(
            slots[1].duration_since(slots[0]),
            Duration::from_millis(250)
        );
        assert_eq!(
            slots[2].duration_since(slots[1]),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn does_not_accumulate_a_backlog_after_idle_time() {
        let started = Instant::now();
        let pacer = Pacer::new(10, started);

        pacer.reserve(started);
        let resumed = started + Duration::from_secs(1);

        assert_eq!(pacer.reserve(resumed), resumed);
    }

    #[test]
    fn timestamp_gaps_are_scaled_and_wrap_without_an_invented_gap() {
        let started = Instant::now();
        let pacer = TimestampPacer::new(2.0, started);

        assert_eq!(pacer.reserve(1_000_000, started), started);
        assert_eq!(
            pacer.reserve(2_000_000, started),
            started + Duration::from_millis(500)
        );
        assert_eq!(
            pacer.reserve(1_000_000, started),
            started + Duration::from_millis(500)
        );
    }
}
