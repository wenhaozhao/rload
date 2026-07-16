use std::time::Duration;

use crate::RunSummary;

pub fn evaluate(summary: &RunSummary, expression: &str) -> Result<(), String> {
    let mut parts = expression.split_whitespace();
    let metric = parts.next().ok_or_else(|| invalid(expression))?;
    let operator = parts.next().ok_or_else(|| invalid(expression))?;
    let expected = parts.next().ok_or_else(|| invalid(expression))?;
    if parts.next().is_some() {
        return Err(invalid(expression));
    }
    let actual = metric_value(summary, metric)?;
    let expected = expected_value(metric, expected)?;
    let passed = match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        "==" => actual == expected,
        "!=" => actual != expected,
        ">=" => actual >= expected,
        ">" => actual > expected,
        _ => {
            return Err(format!(
                "invalid assertion operator: {operator}; expected <, <=, ==, !=, >=, or >"
            ));
        }
    };
    if passed {
        Ok(())
    } else {
        Err(format!("assertion failed: {expression} (actual {actual})"))
    }
}

fn invalid(expression: &str) -> String {
    format!("invalid assertion: {expression}; expected METRIC OPERATOR VALUE")
}

fn metric_value(summary: &RunSummary, metric: &str) -> Result<f64, String> {
    let latency = |percentile| {
        summary
            .latencies
            .percentile(percentile)
            .map(duration_us)
            .map(|value| value as f64)
            .ok_or_else(|| format!("assertion metric unavailable: {metric}"))
    };
    match metric {
        "rps" => Ok(rate(summary.completed, summary.load_runtime)),
        "mean" => summary
            .latencies
            .average()
            .map(duration_us)
            .map(|value| value as f64)
            .ok_or_else(|| format!("assertion metric unavailable: {metric}")),
        "p50" => latency(50.0),
        "p90" => latency(90.0),
        "p95" => latency(95.0),
        "p99" => latency(99.0),
        "error_rate" => {
            let attempts = summary.completed + summary.socket_errors.total();
            Ok(if attempts == 0 {
                0.0
            } else {
                (summary.status_errors + summary.socket_errors.total()) as f64 / attempts as f64
            })
        }
        "status_errors" => Ok(summary.status_errors as f64),
        "socket_errors" => Ok(summary.socket_errors.total() as f64),
        "completed" => Ok(summary.completed as f64),
        _ => Err(format!("unknown assertion metric: {metric}")),
    }
}

fn expected_value(metric: &str, value: &str) -> Result<f64, String> {
    let latency_metric = matches!(metric, "mean" | "p50" | "p90" | "p95" | "p99");
    if latency_metric {
        return parse_duration_us(value);
    }
    if value.ends_with("us") || value.ends_with("ms") || value.ends_with('s') {
        return Err(format!(
            "assertion value for {metric} must not include a duration unit"
        ));
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("invalid assertion value: {value}"))
}

fn parse_duration_us(value: &str) -> Result<f64, String> {
    let (number, multiplier) = if let Some(value) = value.strip_suffix("us") {
        (value, 1.0)
    } else if let Some(value) = value.strip_suffix("ms") {
        (value, 1_000.0)
    } else if let Some(value) = value.strip_suffix('s') {
        (value, 1_000_000.0)
    } else {
        return Err(format!(
            "assertion value for latency metrics requires us, ms, or s: {value}"
        ));
    };
    number
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value * multiplier)
        .ok_or_else(|| format!("invalid assertion value: {value}"))
}

fn duration_us(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
fn rate(completed: u64, runtime: Duration) -> f64 {
    if runtime.is_zero() {
        0.0
    } else {
        completed as f64 / runtime.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_count_and_error_metrics() {
        let mut summary = RunSummary::default();
        summary.completed = 10;
        summary.status_errors = 1;
        summary.socket_errors.connect = 1;
        assert!(evaluate(&summary, "completed == 10").is_ok());
        assert!(evaluate(&summary, "error_rate <= 0.19").is_ok());
        assert!(
            evaluate(&summary, "status_errors == 0")
                .unwrap_err()
                .contains("assertion failed")
        );
    }

    #[test]
    fn validates_latency_units() {
        assert_eq!(parse_duration_us("1.5ms"), Ok(1500.0));
        assert!(parse_duration_us("10").is_err());
    }
}
