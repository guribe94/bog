#!/usr/bin/env python3
"""
Process benchmark results into consolidated markdown format for Bog.

Usage: python3 process_benchmarks.py <raw_dir> <output_file> [--platform PLATFORM] [--compare-with FILE]

Arguments:
    raw_dir       Directory containing raw Criterion benchmark outputs (.txt files)
    output_file   Path for consolidated markdown output
    --platform    Platform name (e.g., M1, c6in_xlarge) - auto-detected if not provided
    --compare-with Path to previous consolidated markdown file for regression analysis
"""

import sys
import os
import re
import glob
from datetime import datetime
from typing import Dict, List, Optional, Tuple
import argparse

# All bog benchmarks
ALL_BENCHMARKS = [
    "engine_bench",
    "conversion_bench",
    "atomic_bench",
    "fill_processing_bench",
    "inventory_strategy_bench",
    "tls_overhead_bench",
    "multi_tick_bench",
    "circuit_breaker_bench",
    "depth_bench",
    "throughput_bench",
    "order_fsm_bench",
    "reconciliation_bench",
    "resilience_bench",
]

def parse_time_to_ns(value_str: str, unit_str: str) -> float:
    """Convert time value to nanoseconds."""
    value = float(value_str)
    if unit_str == 'ns':
        return value
    elif unit_str == 'µs':
        return value * 1000
    elif unit_str == 'ms':
        return value * 1000000
    elif unit_str == 's':
        return value * 1000000000
    return value

def extract_benchmarks(input_file: str) -> List[Dict]:
    """Extract all benchmark results from criterion output."""
    results = []

    with open(input_file, 'r') as f:
        content = f.read()

    # Pattern: benchmark_name/variant   time:   [lower unit mean unit upper unit]
    time_pattern = r'([a-z_][a-z_0-9]*(?:/[a-z_0-9]+)*)\s+time:\s+\[([0-9.]+)\s+(ns|µs|ms|s)\s+([0-9.]+)\s+(ns|µs|ms|s)\s+([0-9.]+)\s+(ns|µs|ms|s)\]'

    for match in re.finditer(time_pattern, content):
        bench_full = match.group(1)
        parts = bench_full.split('/')

        bench_name = bench_full
        impl = 'unknown'
        if len(parts) > 1:
            impl = '/'.join(parts[1:])
            bench_name = parts[0]

        lower_ns = parse_time_to_ns(match.group(2), match.group(3))
        mean_ns = parse_time_to_ns(match.group(4), match.group(5))
        upper_ns = parse_time_to_ns(match.group(6), match.group(7))

        # Find outlier data
        outliers_count = 0
        outliers_pct = 0.0
        samples = 100
        throughput = ''

        search_start = match.end()
        search_end = min(search_start + 500, len(content))
        context = content[search_start:search_end]

        outlier_match = re.search(r'Found ([0-9]+) outliers among ([0-9]+) measurements \(([0-9.]+)%\)', context)
        if outlier_match:
            outliers_count = int(outlier_match.group(1))
            samples = int(outlier_match.group(2))
            outliers_pct = float(outlier_match.group(3))

        thrpt_match = re.search(r'thrpt:\s+\[(?:[0-9.]+ (?:Melem|elem)/s )?([0-9.]+) (?:Melem|elem)/s', context)
        if thrpt_match:
            throughput = thrpt_match.group(1)

        results.append({
            'benchmark_name': bench_name,
            'implementation': impl,
            'mean_ns': mean_ns,
            'lower_ns': lower_ns,
            'upper_ns': upper_ns,
            'outliers_count': outliers_count,
            'outliers_pct': outliers_pct,
            'samples': samples,
            'throughput_melem_s': throughput
        })

    return results

def format_latency(ns: float) -> str:
    """Format nanoseconds into appropriate unit."""
    if ns < 1000:
        return f"{ns:.2f} ns"
    elif ns < 1000000:
        return f"{ns/1000:.2f} µs"
    else:
        return f"{ns/1000000:.2f} ms"

def read_system_info(raw_dir: str) -> Dict[str, str]:
    """Read system_info.env file if it exists."""
    info_file = os.path.join(raw_dir, 'system_info.env')
    info = {}

    if os.path.exists(info_file):
        with open(info_file, 'r') as f:
            for line in f:
                line = line.strip()
                if '=' in line and not line.startswith('#'):
                    key, value = line.split('=', 1)
                    info[key] = value

    return info

def parse_previous_results(previous_file: str) -> Dict[str, Dict[str, float]]:
    """Parse previous consolidated markdown to extract baseline metrics."""
    baselines = {}

    if not os.path.exists(previous_file):
        return baselines

    with open(previous_file, 'r') as f:
        content = f.read()

    # Extract metrics from markdown tables
    pattern = r'\|\s*`([^`]+)`\s*\|\s*([^|]+)\|\s*([0-9.]+)\s*(ns|µs|ms)'

    for match in re.finditer(pattern, content):
        bench_name = match.group(1).strip()
        impl = match.group(2).strip()
        value = float(match.group(3))
        unit = match.group(4)

        # Convert to ns
        if unit == 'µs':
            value *= 1000
        elif unit == 'ms':
            value *= 1000000

        key = f"{bench_name}/{impl}" if impl and impl != 'unknown' else bench_name
        baselines[key] = {'mean_ns': value}

    return baselines

def compare_with_baseline(current_results: Dict[str, List[Dict]],
                         baseline_file: Optional[str]) -> Tuple[List[Dict], List[Dict]]:
    """Compare current results with baseline and identify regressions/improvements."""
    if not baseline_file or not os.path.exists(baseline_file):
        return [], []

    baselines = parse_previous_results(baseline_file)
    regressions = []
    improvements = []

    for benchmark_category, results in current_results.items():
        for result in results:
            bench_name = result['benchmark_name']
            impl = result['implementation']
            current_ns = result['mean_ns']

            key = f"{bench_name}/{impl}"

            if key in baselines:
                baseline_ns = baselines[key]['mean_ns']
                change_pct = ((current_ns - baseline_ns) / baseline_ns) * 100

                if abs(change_pct) > 5.0:  # 5% threshold
                    entry = {
                        'name': key,
                        'baseline_ns': baseline_ns,
                        'current_ns': current_ns,
                        'change_pct': change_pct
                    }

                    if change_pct > 0:
                        regressions.append(entry)
                    else:
                        improvements.append(entry)

    return regressions, improvements

def generate_consolidated_markdown(raw_dir: str,
                                  output_file: str,
                                  platform: str,
                                  baseline_file: Optional[str] = None):
    """Generate consolidated markdown file with all benchmark results."""

    # Collect all raw benchmark files
    raw_files = {}
    for benchmark in ALL_BENCHMARKS:
        pattern = os.path.join(raw_dir, f"{benchmark}.txt")
        if os.path.exists(pattern):
            raw_files[benchmark] = pattern

    # Also check for full_suite.txt (bog convention)
    full_suite = os.path.join(raw_dir, "full_suite.txt")
    if os.path.exists(full_suite):
        raw_files['full_suite'] = full_suite

    # Extract metrics from all files
    all_results = {}
    for benchmark, filepath in raw_files.items():
        all_results[benchmark] = extract_benchmarks(filepath)

    # Read system info
    system_info = read_system_info(raw_dir)

    # Compare with baseline
    regressions, improvements = compare_with_baseline(all_results, baseline_file)

    # Get timestamp
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    # Start writing markdown
    with open(output_file, 'w') as f:
        # Header
        f.write(f"# Bog Benchmark Results: {timestamp} ({platform})\n\n")

        # Metadata
        f.write("## Metadata\n\n")
        f.write(f"- **Platform**: {platform}\n")
        f.write(f"- **Date**: {timestamp}\n")
        f.write(f"- **CPU**: {system_info.get('CPU', 'N/A')}\n")
        f.write(f"- **RAM**: {system_info.get('RAM', 'N/A')}\n")
        f.write(f"- **OS**: {system_info.get('OS', 'N/A')} {system_info.get('OS_VERSION', '')}\n")
        f.write(f"- **Architecture**: {system_info.get('ARCH', 'N/A')}\n")
        f.write(f"- **Rust Version**: {system_info.get('RUST_VERSION', 'N/A')}\n")
        f.write(f"- **Bog Version**: {system_info.get('BOG_VERSION', 'N/A')}\n")
        f.write(f"- **Git Commit**: {system_info.get('GIT_COMMIT', 'N/A')}\n")
        f.write(f"- **Git Branch**: {system_info.get('GIT_BRANCH', 'N/A')}\n")
        f.write("\n---\n\n")

        # Benchmark sections
        for benchmark in ALL_BENCHMARKS:
            title = benchmark.replace('_', ' ').title()
            f.write(f"## {title}\n\n")

            if benchmark in all_results and all_results[benchmark]:
                results = all_results[benchmark]

                # Summary
                if results:
                    mean_vals = [r['mean_ns'] for r in results]
                    avg_mean = sum(mean_vals) / len(mean_vals)
                    min_mean = min(mean_vals)
                    max_mean = max(mean_vals)
                    f.write(f"**Summary**: Average: {format_latency(avg_mean)} | Range: [{format_latency(min_mean)} - {format_latency(max_mean)}]\n\n")

                f.write("| Benchmark | Variant | Mean Latency | Range | Outliers |\n")
                f.write("|-----------|---------|--------------|-------|----------|\n")

                for result in results:
                    bench = result['benchmark_name']
                    impl = result['implementation']
                    mean = format_latency(result['mean_ns'])
                    lower = format_latency(result['lower_ns'])
                    upper = format_latency(result['upper_ns'])
                    outliers = f"{result['outliers_pct']:.1f}%"

                    f.write(f"| `{bench}` | {impl} | {mean} | [{lower} - {upper}] | {outliers} |\n")
            else:
                f.write("**NOT RUN**\n")

            f.write("\n---\n\n")

        # Regression Analysis
        if regressions or improvements:
            f.write("## Regression Analysis\n\n")

            if regressions:
                f.write("### Regressions (>5% slower)\n\n")
                for r in sorted(regressions, key=lambda x: x['change_pct'], reverse=True):
                    f.write(f"- **{r['name']}**: {format_latency(r['baseline_ns'])} → {format_latency(r['current_ns'])} ({r['change_pct']:+.1f}%)\n")
                f.write("\n")

            if improvements:
                f.write("### Improvements (>5% faster)\n\n")
                for r in sorted(improvements, key=lambda x: x['change_pct']):
                    f.write(f"- **{r['name']}**: {format_latency(r['baseline_ns'])} → {format_latency(r['current_ns'])} ({r['change_pct']:+.1f}%)\n")
                f.write("\n")

            if not regressions:
                f.write("No regressions detected.\n\n")
        else:
            f.write("## Regression Analysis\n\n")
            f.write("No baseline comparison available or all changes within ±5% threshold.\n\n")

        f.write("---\n\n")

        # Footer
        f.write("## Notes\n\n")
        f.write(f"- Generated: {timestamp}\n")
        f.write(f"- Script: process_benchmarks.py\n")
        f.write(f"- Benchmarks run: {len(raw_files)}/{len(ALL_BENCHMARKS)}\n")
        f.write(f"- Raw data: `{os.path.basename(raw_dir)}/`\n")

    print(f"Consolidated markdown generated: {output_file}")
    print(f"Benchmarks processed: {len(raw_files)}/{len(ALL_BENCHMARKS)}")
    if regressions:
        print(f"WARNING: {len(regressions)} regressions detected")
    if improvements:
        print(f"INFO: {len(improvements)} improvements detected")

def main():
    parser = argparse.ArgumentParser(
        description='Process Bog benchmark results into consolidated markdown format'
    )
    parser.add_argument('raw_dir', help='Directory containing raw Criterion outputs')
    parser.add_argument('output_file', help='Path for consolidated markdown output')
    parser.add_argument('--platform', help='Platform name (e.g., M1, c6in_xlarge)')
    parser.add_argument('--compare-with', dest='baseline_file',
                       help='Previous consolidated markdown file for regression analysis')

    args = parser.parse_args()

    # Auto-detect platform if not provided
    platform = args.platform
    if not platform:
        system_info = read_system_info(args.raw_dir)
        platform = system_info.get('PLATFORM', 'unknown')

    generate_consolidated_markdown(
        args.raw_dir,
        args.output_file,
        platform,
        args.baseline_file
    )

if __name__ == '__main__':
    main()
