use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use hdrhistogram::Histogram;

use crate::Method;

const MAX_LATENCY_US: u64 = 60 * 60 * 1_000_000;
const URI_TOP_CAPACITY: usize = 20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SocketErrors {
    pub connect: u64,
    pub read: u64,
    pub write: u64,
    pub timeout: u64,
}

impl SocketErrors {
    pub fn total(self) -> u64 {
        self.connect + self.read + self.write + self.timeout
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.connect += other.connect;
        self.read += other.read;
        self.write += other.write;
        self.timeout += other.timeout;
    }
}

#[derive(Debug, Default)]
pub struct LatencyHistogram {
    histogram: Option<Histogram<u64>>,
    overflow_count: u64,
}

impl LatencyHistogram {
    pub fn len(&self) -> u64 {
        self.histogram.as_ref().map_or(0, Histogram::len)
    }

    pub fn is_empty(&self) -> bool {
        self.histogram.as_ref().is_none_or(Histogram::is_empty)
    }

    pub fn mean(&self) -> Duration {
        Duration::from_secs_f64(self.histogram.as_ref().map_or(0.0, Histogram::mean) / 1_000_000.0)
    }

    pub fn percentile(&self, percentile: f64) -> Option<Duration> {
        if !percentile.is_finite() || !(0.0..=100.0).contains(&percentile) {
            return None;
        }
        Some(Duration::from_micros(
            self.histogram.as_ref().map_or(0, |histogram| {
                histogram.value_at_quantile(percentile / 100.0)
            }),
        ))
    }

    pub fn overflow_count(&self) -> u64 {
        self.overflow_count
    }

    pub(crate) fn record(&mut self, latency: Duration) {
        let raw_micros = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        if raw_micros > MAX_LATENCY_US {
            self.overflow_count += 1;
        }
        let micros = raw_micros.clamp(1, MAX_LATENCY_US);
        self.histogram_mut()
            .record(micros)
            .expect("clamped latency fits histogram bounds");
    }

    fn merge(&mut self, other: &Self) {
        match (&mut self.histogram, &other.histogram) {
            (Some(histogram), Some(other)) => histogram
                .add(other)
                .expect("latency histograms have identical bounds"),
            (None, Some(other)) => self.histogram = Some(other.clone()),
            _ => {}
        }
        self.overflow_count += other.overflow_count;
    }

    pub(crate) fn correct_for_coordinated_omission(&mut self, expected: Duration) {
        let interval = u64::try_from(expected.as_micros())
            .unwrap_or(u64::MAX)
            .max(1);
        let Some(source) = self.histogram.clone() else {
            return;
        };
        for value in source.iter_recorded() {
            let count = value.count_at_value();
            let mut missing = value.value_iterated_to().saturating_sub(interval);
            while missing > interval {
                self.histogram_mut()
                    .record_n(missing, count)
                    .expect("corrected latency fits histogram bounds");
                missing -= interval;
            }
        }
    }

    fn histogram_mut(&mut self) -> &mut Histogram<u64> {
        self.histogram.get_or_insert_with(|| {
            Histogram::new_with_bounds(1, MAX_LATENCY_US, 3)
                .expect("latency histogram bounds are valid")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_percentiles() {
        let histogram = LatencyHistogram::default();

        assert_eq!(histogram.percentile(f64::NAN), None);
        assert_eq!(histogram.percentile(-1.0), None);
        assert_eq!(histogram.percentile(101.0), None);
    }

    #[test]
    fn socket_error_total_is_derived_after_merge() {
        let mut first = SocketErrors {
            connect: 1,
            read: 2,
            write: 0,
            timeout: 0,
        };
        first.merge(SocketErrors {
            connect: 0,
            read: 0,
            write: 3,
            timeout: 4,
        });

        assert_eq!(first.total(), 10);
        assert_eq!(first.connect, 1);
        assert_eq!(first.read, 2);
        assert_eq!(first.write, 3);
        assert_eq!(first.timeout, 4);
    }

    #[test]
    fn method_histograms_allocate_only_after_the_method_is_observed() {
        let mut summary = RunSummary::default();

        assert!(summary.latencies.histogram.is_none());
        assert!(summary.method(Method::Get).latencies.histogram.is_none());
        assert!(summary.method(Method::Head).latencies.histogram.is_none());

        summary.record_response(Method::Get, "/", 200, Duration::from_micros(10));

        assert!(summary.latencies.histogram.is_some());
        assert!(summary.method(Method::Get).latencies.histogram.is_some());
        assert!(summary.method(Method::Head).latencies.histogram.is_none());
    }

    #[test]
    fn merges_samples_and_overflow_counts() {
        let mut first = LatencyHistogram::default();
        first.record(Duration::from_micros(100));
        let mut second = LatencyHistogram::default();
        second.record(Duration::from_micros(200));
        second.record(Duration::from_secs(60 * 60 + 1));

        first.merge(&second);

        assert_eq!(first.len(), 3);
        assert_eq!(first.overflow_count(), 1);
        assert!(first.percentile(50.0).unwrap() >= Duration::from_micros(100));
    }

    #[test]
    fn correction_matches_wrk_strict_interval_rule() {
        let mut histogram = LatencyHistogram::default();
        histogram.record(Duration::from_micros(300));

        histogram.correct_for_coordinated_omission(Duration::from_micros(100));

        assert_eq!(histogram.len(), 2);
        assert_eq!(histogram.percentile(0.0), Some(Duration::from_micros(200)));
        assert_eq!(
            histogram.percentile(100.0),
            Some(Duration::from_micros(300))
        );
    }

    #[test]
    fn merges_method_and_status_groups_consistently() {
        let mut first = RunSummary::default();
        first.record_response(Method::Get, "/a", 200, Duration::from_micros(100));
        first.record_response(Method::Head, "/b", 404, Duration::from_micros(200));
        let mut second = RunSummary::default();
        second.record_response(Method::Get, "/a", 200, Duration::from_micros(300));

        first.merge(second);

        assert_eq!(first.completed, 3);
        assert_eq!(first.status_errors, 1);
        assert_eq!(first.method(Method::Get).completed, 2);
        assert_eq!(first.method(Method::Head).completed, 1);
        assert_eq!(first.method(Method::Head).status_errors, 1);
        assert_eq!(first.status_count(200), 2);
        assert_eq!(first.status_count(404), 1);
        assert_eq!(
            first.observed_statuses().collect::<Vec<_>>(),
            vec![(200, 2), (404, 1)]
        );
    }

    #[test]
    fn uri_top_is_bounded_and_retains_the_heavy_hitter() {
        let mut top = UriTop::new(2);
        for _ in 0..10 {
            top.record("/popular");
        }
        top.record("/one");
        top.record("/two");
        top.record("/three");

        let entries = top.sorted();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].uri.as_ref(), "/popular");
        assert!(entries[0].estimated_requests >= 10);
    }

    #[test]
    fn merged_uri_top_preserves_one_sided_error_bounds() {
        let mut first = UriTop::new(2);
        for _ in 0..5 {
            first.record("/popular");
        }
        first.record("/first-only");
        let mut second = UriTop::new(2);
        second.record("/popular");
        second.record("/second-a");
        second.record("/second-b");

        first.merge(&second);

        assert_eq!(first.entries.len(), 2);
        let popular = first
            .entries
            .iter()
            .find(|entry| entry.uri.as_ref() == "/popular")
            .unwrap();
        let lower_bound = popular.estimated_requests - popular.maximum_error;
        assert!(lower_bound <= 6);
        assert!(6 <= popular.estimated_requests);
    }
}

#[derive(Debug)]
pub struct RunSummary {
    pub completed: u64,
    pub read_bytes: u64,
    pub response_body_bytes: u64,
    pub status_errors: u64,
    pub socket_errors: SocketErrors,
    pub latencies: LatencyHistogram,
    pub runtime: Duration,
    pub load_runtime: Duration,
    pub drain_runtime: Duration,
    pub coordinated_omission_interval: Option<Duration>,
    pub filtered_replay_entries: u64,
    pub skipped_access_log_methods: BTreeMap<String, u64>,
    pub configured_replay_rate: Option<u64>,
    pub replay_entries: u64,
    pub configured_replay_rounds: Option<u64>,
    pub completed_replay_rounds: Option<u64>,
    method_summaries: [MethodSummary; Method::ALL.len()],
    status_counts: StatusCounts,
    uri_top: UriTop,
}

impl Default for RunSummary {
    fn default() -> Self {
        Self {
            completed: 0,
            read_bytes: 0,
            response_body_bytes: 0,
            status_errors: 0,
            socket_errors: SocketErrors::default(),
            latencies: LatencyHistogram::default(),
            runtime: Duration::ZERO,
            load_runtime: Duration::ZERO,
            drain_runtime: Duration::ZERO,
            coordinated_omission_interval: None,
            filtered_replay_entries: 0,
            skipped_access_log_methods: BTreeMap::new(),
            configured_replay_rate: None,
            replay_entries: 0,
            configured_replay_rounds: None,
            completed_replay_rounds: None,
            method_summaries: std::array::from_fn(|_| MethodSummary::default()),
            status_counts: StatusCounts::default(),
            uri_top: UriTop::default(),
        }
    }
}

impl RunSummary {
    pub(crate) fn merge(&mut self, other: Self) {
        self.completed += other.completed;
        self.read_bytes += other.read_bytes;
        self.response_body_bytes += other.response_body_bytes;
        self.status_errors += other.status_errors;
        self.socket_errors.merge(other.socket_errors);
        self.filtered_replay_entries += other.filtered_replay_entries;
        for (method, count) in other.skipped_access_log_methods {
            *self.skipped_access_log_methods.entry(method).or_default() += count;
        }
        self.latencies.merge(&other.latencies);
        for (method, other) in self
            .method_summaries
            .iter_mut()
            .zip(&other.method_summaries)
        {
            method.merge(other);
        }
        self.status_counts.merge(&other.status_counts);
        self.uri_top.merge(&other.uri_top);
    }

    pub fn method(&self, method: Method) -> &MethodSummary {
        &self.method_summaries[method.index()]
    }

    pub fn status_count(&self, status: u16) -> u64 {
        self.status_counts.get(status)
    }

    pub fn observed_statuses(&self) -> impl Iterator<Item = (u16, u64)> + '_ {
        self.status_counts
            .0
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(status, count)| (status as u16, *count))
    }

    pub fn top_uris(&self) -> Vec<UriStatistic> {
        self.uri_top.sorted()
    }

    pub(crate) fn record_response(
        &mut self,
        method: Method,
        uri: &str,
        status: u16,
        latency: Duration,
    ) {
        self.completed += 1;
        self.status_errors += u64::from(status >= 400);
        self.latencies.record(latency);
        self.method_mut(method).record(status, latency);
        self.status_counts.record(status);
        self.uri_top.record(uri);
    }

    pub(crate) fn correct_method_histograms(&mut self, expected: Duration) {
        for method in &mut self.method_summaries {
            method.latencies.correct_for_coordinated_omission(expected);
        }
    }

    fn method_mut(&mut self, method: Method) -> &mut MethodSummary {
        &mut self.method_summaries[method.index()]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UriStatistic {
    pub uri: Arc<str>,
    pub estimated_requests: u64,
    pub maximum_error: u64,
}

#[derive(Debug)]
struct UriTop {
    capacity: usize,
    entries: Vec<UriStatistic>,
}

impl Default for UriTop {
    fn default() -> Self {
        Self::new(URI_TOP_CAPACITY)
    }
}

impl UriTop {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Vec::with_capacity(capacity),
        }
    }

    fn record(&mut self, uri: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.uri.as_ref() == uri)
        {
            entry.estimated_requests += 1;
            return;
        }
        self.record_weight(Arc::from(uri), 1, 0);
    }

    fn merge(&mut self, other: &Self) {
        let self_minimum = self.minimum_count();
        let other_minimum = other.minimum_count();
        let mut merged = Vec::with_capacity(self.entries.len() + other.entries.len());
        for entry in &self.entries {
            let other_entry = other.entries.iter().find(|other| other.uri == entry.uri);
            merged.push(UriStatistic {
                uri: Arc::clone(&entry.uri),
                estimated_requests: entry.estimated_requests
                    + other_entry.map_or(other_minimum, |other| other.estimated_requests),
                maximum_error: entry.maximum_error
                    + other_entry.map_or(other_minimum, |other| other.maximum_error),
            });
        }
        for entry in &other.entries {
            if self.entries.iter().any(|current| current.uri == entry.uri) {
                continue;
            }
            merged.push(UriStatistic {
                uri: Arc::clone(&entry.uri),
                estimated_requests: entry.estimated_requests + self_minimum,
                maximum_error: entry.maximum_error + self_minimum,
            });
        }
        merged.sort_unstable_by(|left, right| {
            right
                .estimated_requests
                .cmp(&left.estimated_requests)
                .then_with(|| left.uri.cmp(&right.uri))
        });
        merged.truncate(self.capacity);
        self.entries = merged;
    }

    fn record_weight(&mut self, uri: Arc<str>, count: u64, error: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.uri == uri) {
            entry.estimated_requests += count;
            entry.maximum_error += error;
            return;
        }
        if self.entries.len() < self.capacity {
            self.entries.push(UriStatistic {
                uri,
                estimated_requests: count,
                maximum_error: error,
            });
            return;
        }
        let entry = self
            .entries
            .iter_mut()
            .min_by_key(|entry| entry.estimated_requests)
            .expect("URI top capacity is non-zero");
        let minimum = entry.estimated_requests;
        *entry = UriStatistic {
            uri,
            estimated_requests: minimum + count,
            maximum_error: minimum + error,
        };
    }

    fn sorted(&self) -> Vec<UriStatistic> {
        let mut entries = self.entries.clone();
        entries.sort_unstable_by(|left, right| {
            right
                .estimated_requests
                .cmp(&left.estimated_requests)
                .then_with(|| left.uri.cmp(&right.uri))
        });
        entries
    }

    fn minimum_count(&self) -> u64 {
        if self.entries.len() < self.capacity {
            0
        } else {
            self.entries
                .iter()
                .map(|entry| entry.estimated_requests)
                .min()
                .unwrap_or(0)
        }
    }
}

#[derive(Debug, Default)]
pub struct MethodSummary {
    pub completed: u64,
    pub status_errors: u64,
    pub latencies: LatencyHistogram,
}

impl MethodSummary {
    fn record(&mut self, status: u16, latency: Duration) {
        self.completed += 1;
        self.status_errors += u64::from(status >= 400);
        self.latencies.record(latency);
    }

    fn merge(&mut self, other: &Self) {
        self.completed += other.completed;
        self.status_errors += other.status_errors;
        self.latencies.merge(&other.latencies);
    }
}

#[derive(Debug)]
struct StatusCounts([u64; 1000]);

impl Default for StatusCounts {
    fn default() -> Self {
        Self([0; 1000])
    }
}

impl StatusCounts {
    fn record(&mut self, status: u16) {
        if let Some(count) = self.0.get_mut(status as usize) {
            *count += 1;
        }
    }

    fn get(&self, status: u16) -> u64 {
        self.0.get(status as usize).copied().unwrap_or(0)
    }

    fn merge(&mut self, other: &Self) {
        for (count, other) in self.0.iter_mut().zip(other.0) {
            *count += other;
        }
    }
}
