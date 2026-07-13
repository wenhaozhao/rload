#!/usr/bin/env python3
"""Summarize wrk, published rload, and development rload benchmark results."""

from __future__ import annotations

import re
import statistics
import sys
from pathlib import Path

from accuracy import LIMITS, METRICS, parse_result

THROUGHPUT_REGRESSION_LIMIT = 3.0
READ_BYTES_MAE_LIMIT = 0.1


def required(pattern: str, text: str, path: Path) -> re.Match[str]:
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        raise ValueError(f"{path}: missing {pattern!r}")
    return match


def request_and_read_bytes(path: Path, client: str) -> tuple[int, float] | None:
    text = path.read_text()
    if client == "release":
        return None
    if client == "wrk":
        requests = int(required(r"^\s*(\d+) requests in ", text, path).group(1))
        value, unit = required(r",\s*([0-9.]+)(B|KB|MB|GB) read$", text, path).groups()
        factor = {"B": 1, "KB": 1024, "MB": 1024**2, "GB": 1024**3}[unit]
        return requests, float(value) * factor
    requests = int(required(r"^(\d+) requests completed$", text, path).group(1))
    read_bytes = float(required(r"^(\d+) B read$", text, path).group(1))
    return requests, read_bytes


def peak_rss(path: Path) -> int:
    text = path.read_text()
    patterns = [
        (r"^\s*(\d+)\s+maximum resident set size$", 1),
        (r"Maximum resident set size \(kbytes\):\s*(\d+)", 1024),
    ]
    for pattern, factor in patterns:
        if match := re.search(pattern, text, re.MULTILINE):
            return int(match.group(1)) * factor
    raise ValueError(f"{path}: peak RSS not found")


def main() -> int:
    directory = Path(sys.argv[1])
    clients = ("wrk", "release", "dev")
    files = {client: sorted(directory.glob(f"{client}-*.txt")) for client in clients}
    runs = len(files["wrk"])
    if runs < 5 or any(len(paths) != runs for paths in files.values()):
        raise ValueError("three-way validation requires at least five complete runs")

    parsed = {
        client: [parse_result(path, "wrk" if client == "wrk" else "rload") for path in paths]
        for client, paths in files.items()
    }
    failed = False
    print(f"Runs per client: {runs}")
    print("| Client | Mean RPS | Mean peak RSS |")
    print("|---|---:|---:|")
    for client in clients:
        rps = statistics.mean(result["rps"] for result in parsed[client])
        rss = statistics.mean(peak_rss(Path(f"{path}.time")) for path in files[client])
        print(f"| {client} | {rps:.2f} | {rss:.0f} B |")

    labels = [f"{metric} MAE" if metric != "p99" else "p99 median abs" for metric in METRICS]
    print("\n| Candidate | " + " | ".join(labels) + " |")
    print("|---|" + "---:|" * len(METRICS))
    observed_by_candidate = {}
    for candidate in ("release", "dev"):
        values = []
        observed_by_candidate[candidate] = {}
        for metric in METRICS:
            errors = [
                abs(current[metric] / baseline[metric] - 1) * 100
                for baseline, current in zip(parsed["wrk"], parsed[candidate])
            ]
            observed = statistics.median(errors) if metric == "p99" else statistics.mean(errors)
            observed_by_candidate[candidate][metric] = observed
            values.append(f"{observed:.3f}%")
        print(f"| {candidate} | " + " | ".join(values) + " |")

    for metric, observed in observed_by_candidate["dev"].items():
        if observed > LIMITS[metric]:
            print(f"FAIL: dev {metric} error {observed:.3f}% exceeds {LIMITS[metric]:.1f}%")
            failed = True

    release_rps = statistics.mean(result["rps"] for result in parsed["release"])
    dev_rps = statistics.mean(result["rps"] for result in parsed["dev"])
    throughput_change = (dev_rps / release_rps - 1) * 100
    if throughput_change < -THROUGHPUT_REGRESSION_LIMIT:
        print(
            f"FAIL: dev throughput change {throughput_change:.3f}% exceeds "
            f"-{THROUGHPUT_REGRESSION_LIMIT:.1f}% regression limit"
        )
        failed = True

    wrk_bytes = [request_and_read_bytes(path, "wrk") for path in files["wrk"]]
    dev_bytes = [request_and_read_bytes(path, "dev") for path in files["dev"]]
    errors = [
        abs((dev_total / dev_requests) / (wrk_total / wrk_requests) - 1) * 100
        for (wrk_requests, wrk_total), (dev_requests, dev_total) in zip(wrk_bytes, dev_bytes)
    ]
    read_bytes_mae = statistics.mean(errors)
    print(f"\nread_bytes per-request MAE versus wrk: {read_bytes_mae:.4f}%")
    if read_bytes_mae > READ_BYTES_MAE_LIMIT:
        print(
            f"FAIL: read_bytes MAE {read_bytes_mae:.4f}% exceeds "
            f"{READ_BYTES_MAE_LIMIT:.1f}%"
        )
        failed = True
    print("Gate: " + ("FAIL" if failed else "PASS"))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
