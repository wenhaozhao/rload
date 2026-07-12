use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::ReplayStage;

pub struct Pacer {
    interval: Duration,
    next: Mutex<Instant>,
}

pub struct TimestampPacer {
    speed: f64,
    state: Mutex<TimestampState>,
}

pub struct StagePacer {
    stages: Vec<(Duration, u64)>,
    state: Mutex<StageState>,
}

struct StageState {
    started: Option<Instant>,
    next: Option<Instant>,
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

impl StagePacer {
    pub fn new(stages: &[ReplayStage]) -> Self {
        let mut boundary = Duration::ZERO;
        let stages = stages
            .iter()
            .map(|stage| {
                boundary += stage.duration;
                (boundary, stage.rate)
            })
            .collect();
        Self {
            stages,
            state: Mutex::new(StageState {
                started: None,
                next: None,
            }),
        }
    }

    pub fn reserve(&self, now: Instant) -> Instant {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let started = *state.started.get_or_insert(now);
        let scheduled = state.next.unwrap_or(now).max(now);
        let elapsed = scheduled.saturating_duration_since(started);
        let (rate, boundary) = self
            .stages
            .iter()
            .find(|(boundary, _)| elapsed < *boundary)
            .map_or_else(
                || (self.stages.last().expect("stages are validated").1, None),
                |(boundary, rate)| (*rate, Some(started + *boundary)),
            );
        let candidate = scheduled + Duration::from_secs_f64(1.0 / rate as f64);
        state.next = Some(boundary.map_or(candidate, |boundary| candidate.min(boundary)));
        scheduled
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

    #[test]
    fn stage_pacer_switches_rates_at_stage_boundaries() {
        let started = Instant::now();
        let pacer = StagePacer::new(&[
            ReplayStage {
                duration: Duration::from_secs(1),
                rate: 2,
            },
            ReplayStage {
                duration: Duration::from_secs(1),
                rate: 4,
            },
        ]);

        let slots: Vec<_> = (0..7).map(|_| pacer.reserve(started)).collect();
        assert_eq!(slots[0], started);
        assert_eq!(slots[1], started + Duration::from_millis(500));
        assert_eq!(slots[2], started + Duration::from_secs(1));
        assert_eq!(slots[3], started + Duration::from_millis(1_250));
        assert_eq!(slots[6], started + Duration::from_secs(2));
    }
}
