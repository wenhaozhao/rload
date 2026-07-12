#!/usr/bin/env python3
"""Compare paired wrk and rload benchmark outputs."""

from __future__ import annotations

import argparse
import math
import re
import statistics
import sys
from pathlib import Path


METRICS = ("rps", "avg", "p50", "p75", "p90", "p99")
LIMITS = {"rps": 3.0, "avg": 3.0, "p50": 3.0, "p75": 3.0, "p90": 5.0, "p99": 5.0}
T_CRITICAL_95 = {2: 12.706, 3: 4.303, 4: 3.182, 5: 2.776, 6: 2.571,
                 7: 2.447, 8: 2.365, 9: 2.306, 10: 2.262, 11: 2.228,
                 12: 2.201, 13: 2.179, 14: 2.160, 15: 2.145, 16: 2.131,
                 17: 2.120, 18: 2.110, 19: 2.101, 20: 2.093, 21: 2.086,
                 22: 2.080, 23: 2.074, 24: 2.069, 25: 2.064, 26: 2.060,
                 27: 2.056, 28: 2.052, 29: 2.048, 30: 2.045}


def duration_us(value: str, unit: str) -> float:
    factors = {"ns": 0.001, "us": 1.0, "µs": 1.0, "ms": 1_000.0, "s": 1_000_000.0}
    return float(value) * factors[unit]


def required(pattern: str, text: str, path: Path) -> re.Match[str]:
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        raise ValueError(f"{path}: missing expected field matching {pattern!r}")
    return match


def parse_result(path: Path, client: str) -> dict[str, float]:
    text = path.read_text(encoding="utf-8")
    if client == "wrk":
        avg = required(r"^\s*Latency\s+([0-9.]+)(ns|us|ms|s)\b", text, path)
        percentile = r"^\s*{percent}%\s+([0-9.]+)(ns|us|ms|s)\b"
    else:
        avg = required(r"^Average latency:\s*([0-9.]+)(ns|us|µs|ms|s)\b", text, path)
        percentile = r"^\s*{percent}%\s+([0-9.]+)(ns|us|µs|ms|s)\b"
    result = {"avg": duration_us(*avg.groups())}
    for percent in (50, 75, 90, 99):
        match = required(percentile.format(percent=percent), text, path)
        result[f"p{percent}"] = duration_us(*match.groups())
    result["rps"] = float(required(r"^Requests/sec:\s*([0-9.]+)", text, path).group(1))
    return result


def paired_files(directory: Path) -> list[tuple[Path, Path]]:
    wrk = {int(match.group(1)): path for path in directory.glob("wrk-*.txt")
           if (match := re.fullmatch(r"wrk-(\d+)\.txt", path.name))}
    rust = {int(match.group(1)): path for path in directory.glob("rload-*.txt")
            if (match := re.fullmatch(r"rload-(\d+)\.txt", path.name))}
    if wrk.keys() != rust.keys():
        missing_wrk = sorted(rust.keys() - wrk.keys())
        missing_rust = sorted(wrk.keys() - rust.keys())
        raise ValueError(
            f"{directory}: unpaired results; missing wrk runs {missing_wrk}, "
            f"missing rload runs {missing_rust}"
        )
    return [(wrk[index], rust[index]) for index in sorted(wrk)]


def confidence_interval(values: list[float]) -> tuple[float, float]:
    mean = statistics.mean(values)
    if len(values) < 2:
        return mean, mean
    critical = T_CRITICAL_95.get(len(values), 1.96)
    margin = critical * statistics.stdev(values) / math.sqrt(len(values))
    return mean - margin, mean + margin


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", nargs="+", type=Path)
    parser.add_argument("--no-check", action="store_true", help="report without enforcing thresholds")
    args = parser.parse_args()

    resolved_directories = [directory.resolve() for directory in args.directories]
    if len(set(resolved_directories)) != len(resolved_directories):
        print("error: duplicate result directories are not allowed", file=sys.stderr)
        return 2

    errors = {metric: [] for metric in METRICS}
    pair_count = 0
    try:
        for directory in resolved_directories:
            pairs = paired_files(directory)
            if not pairs:
                raise ValueError(f"{directory}: no paired wrk-N.txt and rload-N.txt files")
            for wrk_path, rust_path in pairs:
                baseline = parse_result(wrk_path, "wrk")
                candidate = parse_result(rust_path, "rload")
                for metric in METRICS:
                    errors[metric].append((candidate[metric] / baseline[metric] - 1.0) * 100.0)
                pair_count += 1
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    print(f"Paired runs: {pair_count}")
    print("| Metric | Bias | MAE | Std dev | 95% CI | Range | Gate | Result |")
    print("|---|---:|---:|---:|---:|---:|---:|:---:|")
    failed = False
    for metric in METRICS:
        values = errors[metric]
        bias = statistics.mean(values)
        mae = statistics.mean(abs(value) for value in values)
        stddev = statistics.stdev(values) if len(values) > 1 else 0.0
        low, high = confidence_interval(values)
        observed = statistics.median(abs(value) for value in values) if metric == "p99" else mae
        passed = pair_count >= 3 and observed <= LIMITS[metric]
        failed |= not passed
        print(
            f"| {metric} | {bias:+.3f}% | {mae:.3f}% | {stddev:.3f}% | "
            f"[{low:+.3f}%, {high:+.3f}%] | [{min(values):+.3f}%, {max(values):+.3f}%] | "
            f"{observed:.3f}% ≤ {LIMITS[metric]:.1f}% | {'PASS' if passed else 'FAIL'} |"
        )
    print("\nP99 gate uses median absolute paired error; other gates use MAE. At least 3 pairs are required.")
    return 0 if args.no_check or not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
