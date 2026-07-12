#!/usr/bin/env python3
"""Analyze static versus access-log replay benchmark results."""

from __future__ import annotations

import argparse
import re
import statistics
import sys
from pathlib import Path


def required(pattern: str, text: str, source: Path) -> re.Match[str]:
    match = re.search(pattern, text, re.MULTILINE | re.IGNORECASE)
    if not match:
        raise ValueError(f"{source}: missing field matching {pattern!r}")
    return match


def rps(path: Path) -> float:
    return float(required(r"^Requests/sec:\s*([0-9.]+)", path.read_text(), path).group(1))


def rss_bytes(path: Path) -> int:
    text = path.read_text()
    darwin = re.search(r"^\s*(\d+)\s+maximum resident set size", text, re.MULTILINE)
    if darwin:
        return int(darwin.group(1))
    linux = required(r"Maximum resident set size \(kbytes\):\s*(\d+)", text, path)
    return int(linux.group(1)) * 1024


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result_directories", nargs="+", type=Path)
    parser.add_argument("--no-check", action="store_true")
    args = parser.parse_args()
    measurements = []
    try:
        for directory in args.result_directories:
            environment_path = directory / "environment.txt"
            environment = environment_path.read_text()
            entries = int(required(r"\bentries=(\d+)", environment, environment_path).group(1))
            runs = int(required(r"\bruns=(\d+)", environment, environment_path).group(1))
            if runs < 3:
                raise ValueError(f"{directory}: at least 3 paired runs are required")
            static_rps = [rps(directory / f"static-{run}.txt") for run in range(1, runs + 1)]
            replay_rps = [rps(directory / f"replay-{run}.txt") for run in range(1, runs + 1)]
            static_rss = [rss_bytes(directory / f"static-{run}.time") for run in range(1, runs + 1)]
            replay_rss = [rss_bytes(directory / f"replay-{run}.time") for run in range(1, runs + 1)]
            measurements.append((directory, entries, static_rps, replay_rps, static_rss, replay_rss))
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    passed = True
    rss_points = []
    for directory, entries, static_rates, replay_rates, static_memory, replay_memory in measurements:
        losses = [(1.0 - replay / static) * 100.0 for static, replay in zip(static_rates, replay_rates)]
        increments = [replay - static for static, replay in zip(static_memory, replay_memory)]
        throughput_loss = statistics.median(losses)
        rss_increment = statistics.median(increments)
        bytes_per_entry = rss_increment / entries
        throughput_pass = throughput_loss <= 10.0
        memory_pass = 0.0 < bytes_per_entry <= 256.0
        passed &= throughput_pass and memory_pass
        median_replay_rss = statistics.median(replay_memory)
        rss_points.append((entries, median_replay_rss))
        print(f"{directory} ({entries} entries, {len(losses)} pairs)")
        print(f"  Throughput loss median: {throughput_loss:+.2f}% (range {min(losses):+.2f}%..{max(losses):+.2f}%, gate <= 10.00%) {'PASS' if throughput_pass else 'FAIL'}")
        print(f"  Replay RSS increment median: {rss_increment / 1048576:+.2f} MiB, {bytes_per_entry:+.1f} B/entry (gate > 0 and <= 256.0) {'PASS' if memory_pass else 'FAIL'}")

    if len(rss_points) > 1:
        low, high = min(rss_points), max(rss_points)
        slope = (high[1] - low[1]) / (high[0] - low[0])
        slope_pass = 0.0 < slope <= 256.0
        passed &= slope_pass
        print(f"RSS scaling slope: {slope:+.1f} B/entry (gate > 0 and <= 256.0) {'PASS' if slope_pass else 'FAIL'}")
    return 0 if args.no_check or passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
