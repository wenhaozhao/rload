use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::pacer::Pacer;
use crate::{Method, ReplayOrder};

pub(crate) struct EncodedRequest {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) method_uri_start: u32,
    pub(crate) uri_end: u32,
}

pub(crate) struct RequestSequence {
    requests: Arc<[EncodedRequest]>,
    selection: Selection,
    pacer: Option<Arc<Pacer>>,
}

enum Selection {
    Sequential(AtomicUsize),
    Random { next: AtomicUsize, seed: u64 },
    Shuffle(Mutex<ShuffleState>),
}

impl RequestSequence {
    pub(crate) fn new(requests: Vec<EncodedRequest>, order: ReplayOrder, seed: u64) -> Self {
        debug_assert!(!requests.is_empty());
        let selection = match order {
            ReplayOrder::Sequential => Selection::Sequential(AtomicUsize::new(0)),
            ReplayOrder::Random => Selection::Random {
                next: AtomicUsize::new(0),
                seed,
            },
            ReplayOrder::Shuffle => {
                let mut shuffle = ShuffleState::new(requests.len(), seed);
                shuffle.reshuffle();
                Selection::Shuffle(Mutex::new(shuffle))
            }
        };
        Self {
            requests: requests.into(),
            selection,
            pacer: None,
        }
    }

    pub(crate) fn with_rate(mut self, rate: Option<u64>) -> Self {
        self.pacer = rate.map(|rate| Arc::new(Pacer::new(rate, std::time::Instant::now())));
        self
    }

    pub(crate) fn next(
        &self,
    ) -> (
        Method,
        Arc<[u8]>,
        std::ops::Range<usize>,
        Option<std::time::Instant>,
    ) {
        let scheduled = self
            .pacer
            .as_ref()
            .map(|pacer| pacer.reserve(std::time::Instant::now()));
        let index = match &self.selection {
            Selection::Sequential(next) => {
                next.fetch_add(1, Ordering::Relaxed) % self.requests.len()
            }
            Selection::Random { next, seed } => {
                let position = next.fetch_add(1, Ordering::Relaxed) as u64;
                bounded(splitmix64(seed.wrapping_add(position)), self.requests.len())
            }
            Selection::Shuffle(shuffle) => shuffle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .next(),
        };
        let request = &self.requests[index];
        let method = Method::ALL[(request.method_uri_start >> 29) as usize];
        (
            method,
            Arc::clone(&request.bytes),
            (request.method_uri_start & 0x1fff_ffff) as usize..request.uri_end as usize,
            scheduled,
        )
    }
}

pub(crate) fn method_uri_start(method: Method, uri_start: usize) -> u32 {
    debug_assert!(uri_start < 0x2000_0000);
    ((method.index() as u32) << 29) | uri_start as u32
}

struct ShuffleState {
    indices: Vec<usize>,
    offset: usize,
    random_state: u64,
}

impl ShuffleState {
    fn new(length: usize, seed: u64) -> Self {
        Self {
            indices: (0..length).collect(),
            offset: 0,
            random_state: seed,
        }
    }

    fn next(&mut self) -> usize {
        if self.offset == self.indices.len() {
            self.reshuffle();
        }
        let index = self.indices[self.offset];
        self.offset += 1;
        index
    }

    fn reshuffle(&mut self) {
        for index in (1..self.indices.len()).rev() {
            let selected = self.bounded(index + 1);
            self.indices.swap(index, selected);
        }
        self.offset = 0;
    }

    fn bounded(&mut self, bound: usize) -> usize {
        loop {
            self.random_state = splitmix64(self.random_state);
            let threshold = (bound as u64).wrapping_neg() % bound as u64;
            if self.random_state >= threshold {
                return (self.random_state % bound as u64) as usize;
            }
        }
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn bounded(mut random: u64, bound: usize) -> usize {
    let threshold = (bound as u64).wrapping_neg() % bound as u64;
    while random < threshold {
        random = splitmix64(random);
    }
    (random % bound as u64) as usize
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn shares_one_cyclic_cursor_across_threads() {
        let sequence = Arc::new(RequestSequence::new(
            vec![encoded(b"A"), encoded(b"B"), encoded(b"C")],
            ReplayOrder::Sequential,
            0,
        ));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let sequence = Arc::clone(&sequence);
                thread::spawn(move || {
                    let mut counts = [0; 3];
                    for _ in 0..300 {
                        let (_, bytes, _, _) = sequence.next();
                        counts[usize::from(bytes[0] - b'A')] += 1;
                    }
                    counts
                })
            })
            .collect();
        let totals = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .fold([0; 3], |mut totals, counts| {
                for (total, count) in totals.iter_mut().zip(counts) {
                    *total += count;
                }
                totals
            });

        assert_eq!(totals, [400, 400, 400]);
    }

    #[test]
    fn shuffle_covers_each_entry_once_per_round_and_repeats_with_seed() {
        let sequence = RequestSequence::new(
            vec![encoded(b"A"), encoded(b"B"), encoded(b"C"), encoded(b"D")],
            ReplayOrder::Shuffle,
            42,
        );

        let first = take(&sequence, 4);
        let second = take(&sequence, 4);

        assert_eq!(sorted(&first), b"ABCD");
        assert_eq!(sorted(&second), b"ABCD");
        let same_seed = RequestSequence::new(
            vec![encoded(b"A"), encoded(b"B"), encoded(b"C"), encoded(b"D")],
            ReplayOrder::Shuffle,
            42,
        );
        assert_eq!([first, second].concat(), take(&same_seed, 8));
    }

    #[test]
    fn random_order_is_reproducible_with_a_seed() {
        let first = RequestSequence::new(
            vec![encoded(b"A"), encoded(b"B"), encoded(b"C")],
            ReplayOrder::Random,
            99,
        );
        let second = RequestSequence::new(
            vec![encoded(b"A"), encoded(b"B"), encoded(b"C")],
            ReplayOrder::Random,
            99,
        );

        assert_eq!(take(&first, 20), take(&second, 20));
    }

    fn take(sequence: &RequestSequence, count: usize) -> Vec<u8> {
        (0..count).map(|_| sequence.next().1[0]).collect()
    }

    fn sorted(values: &[u8]) -> Vec<u8> {
        let mut values = values.to_vec();
        values.sort_unstable();
        values
    }

    fn encoded(bytes: &'static [u8]) -> EncodedRequest {
        EncodedRequest {
            bytes: Arc::from(bytes),
            method_uri_start: method_uri_start(Method::Get, 4),
            uri_end: 5,
        }
    }
}
