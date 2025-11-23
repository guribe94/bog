#!/usr/bin/env python3
"""
Migrate historical benchmark data to new consolidated format for Bog.

This script:
1. Finds all benchmark runs in docs/benchmarks/YYYY-MM/YYYY-MM-DD/
2. Copies raw data to benchmarks/raw/YYYY-MM/YYYY-MM-DD_120000_platform/
3. Processes into consolidated markdown: docs/benchmarks/YYYY-MM/YYYY-MM-DD_120000_platform.md
4. Preserves original REPORT.md files for reference

Usage: python3 migrate_historical_data.py [--dry-run]
"""

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
import argparse

SCRIPT_DIR = Path(__file__).parent.absolute()
PROJECT_DIR = SCRIPT_DIR.parent
BENCHMARKS_DIR = SCRIPT_DIR
RAW_DIR = BENCHMARKS_DIR / "raw"
DOCS_DIR = PROJECT_DIR / "docs" / "benchmarks"

def find_historical_runs() -> list:
    """Find all existing benchmark runs in docs/benchmarks/."""
    runs = []

    if not DOCS_DIR.exists():
        return runs

    # Look for YYYY-MM/YYYY-MM-DD structure
    for year_month_dir in DOCS_DIR.glob("????-??"):
        if not year_month_dir.is_dir():
            continue

        for date_dir in year_month_dir.glob("????-??-??"):
            if not date_dir.is_dir():
                continue

            # Check if it has benchmark data (full_suite.txt or REPORT.md)
            has_data = (date_dir / "full_suite.txt").exists() or \
                      (date_dir / "REPORT.md").exists() or \
                      list(date_dir.glob("*_bench.txt"))

            if has_data:
                runs.append({
                    'date_dir': date_dir,
                    'date_str': date_dir.name,
                    'year_month': year_month_dir.name
                })

    return sorted(runs, key=lambda x: x['date_str'])

def extract_platform_from_report(report_path: Path) -> str:
    """Try to extract platform from REPORT.md."""
    if not report_path.exists():
        return "unknown"

    try:
        with open(report_path, 'r') as f:
            content = f.read()

            # Look for CPU information
            if "Apple M1" in content or "M1" in content:
                return "M1"
            elif "Apple M2" in content or "M2" in content:
                return "M2"
            elif "Apple M3" in content or "M3" in content:
                return "M3"
            elif "c6in" in content.lower():
                return "c6in_xlarge"
            elif "c7g" in content.lower():
                return "c7g_xlarge"
            elif "Intel" in content or "AMD" in content:
                return "x86_64"

    except Exception:
        pass

    return "unknown"

def create_system_info(run_dir: Path, date_str: str, platform: str, report_path: Path):
    """Create system_info.env file from available information."""
    info = {
        'PLATFORM': platform,
        'DATE': date_str,
        'TIMESTAMP': 'unknown',
        'OS': 'unknown',
        'OS_VERSION': 'unknown',
        'ARCH': 'unknown',
        'CPU': 'unknown',
        'CPU_CORES_PHYSICAL': 'unknown',
        'CPU_CORES_LOGICAL': 'unknown',
        'CPU_FREQ': 'unknown',
        'RAM': 'unknown',
        'RUST_VERSION': 'unknown',
        'CARGO_VERSION': 'unknown',
        'GIT_COMMIT': 'unknown',
        'GIT_BRANCH': 'unknown',
        'GIT_DIRTY': 'unknown',
        'BOG_VERSION': 'unknown',
    }

    # Try to extract info from REPORT.md
    if report_path.exists():
        try:
            with open(report_path, 'r') as f:
                content = f.read()

                # Extract CPU
                cpu_match = re.search(r'CPU[:\s]+(.+?)(?:\n|\||$)', content, re.IGNORECASE)
                if cpu_match:
                    info['CPU'] = cpu_match.group(1).strip()

                # Extract RAM
                ram_match = re.search(r'RAM[:\s]+(.+?)(?:\n|\||$)', content, re.IGNORECASE)
                if ram_match:
                    info['RAM'] = ram_match.group(1).strip()

                # Extract OS
                os_match = re.search(r'OS[:\s]+(.+?)(?:\n|\||$)', content, re.IGNORECASE)
                if os_match:
                    info['OS'] = os_match.group(1).strip()

                # Extract Rust version
                rust_match = re.search(r'Rust[:\s]+(.+?)(?:\n|\||$)', content, re.IGNORECASE)
                if rust_match:
                    info['RUST_VERSION'] = rust_match.group(1).strip()

        except Exception as e:
            print(f"  Warning: Could not extract info from REPORT.md: {e}")

    # Write system_info.env
    info_file = run_dir / "system_info.env"
    with open(info_file, 'w') as f:
        for key, value in info.items():
            f.write(f"{key}={value}\n")

    print(f"  Created system_info.env")

def migrate_run(run: dict, dry_run: bool = False):
    """Migrate a single benchmark run."""
    date_dir = run['date_dir']
    date_str = run['date_str']
    year_month = run['year_month']

    print(f"\nMigrating run: {date_str}")

    # Detect platform
    report_path = date_dir / "REPORT.md"
    platform = extract_platform_from_report(report_path)
    print(f"  Platform: {platform}")

    if dry_run:
        raw_files = list(date_dir.glob("*.txt"))
        print(f"  Files to migrate: {len(raw_files)}")
        for f in raw_files:
            print(f"    - {f.name}")
        return True

    # Create run directory (use 12:00:00 as default time)
    run_name = f"{date_str}_120000_{platform}"
    run_dir = RAW_DIR / year_month / run_name
    run_dir.mkdir(parents=True, exist_ok=True)
    print(f"  Run directory: {run_dir}")

    # Copy raw benchmark files
    raw_files = list(date_dir.glob("*.txt"))
    for raw_file in raw_files:
        dst = run_dir / raw_file.name
        shutil.copy2(raw_file, dst)
        print(f"  Copied: {raw_file.name}")

    # Create system_info.env
    create_system_info(run_dir, date_str, platform, report_path)

    # Generate consolidated markdown
    output_dir = DOCS_DIR / year_month
    output_dir.mkdir(parents=True, exist_ok=True)
    output_file = output_dir / f"{run_name}.md"

    print(f"  Generating consolidated report: {output_file}")

    cmd = [
        'python3',
        str(BENCHMARKS_DIR / 'process_benchmarks.py'),
        str(run_dir),
        str(output_file),
        '--platform', platform
    ]

    try:
        result = subprocess.run(cmd, check=True, capture_output=True, text=True)
        print(f"  {result.stdout.strip()}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"  ERROR: Failed to generate report")
        print(f"  {e.stderr}")
        return False

def main():
    parser = argparse.ArgumentParser(description='Migrate historical Bog benchmark data')
    parser.add_argument('--dry-run', action='store_true',
                       help='Show what would be done without making changes')
    args = parser.parse_args()

    print("=" * 70)
    print("BOG HISTORICAL BENCHMARK DATA MIGRATION")
    print("=" * 70)

    if args.dry_run:
        print("\nDRY RUN MODE - No changes will be made\n")

    # Find all historical runs
    print("\nScanning for benchmark runs...")
    runs = find_historical_runs()
    print(f"Found {len(runs)} benchmark runs")

    if not runs:
        print("\nNo runs to migrate!")
        return 0

    # Migrate each run
    print("\n" + "=" * 70)
    print("MIGRATING RUNS")
    print("=" * 70)

    success_count = 0
    fail_count = 0

    for run in runs:
        if migrate_run(run, args.dry_run):
            success_count += 1
        else:
            fail_count += 1

    # Summary
    print("\n" + "=" * 70)
    print("MIGRATION SUMMARY")
    print("=" * 70)
    print(f"Total runs processed: {len(runs)}")
    print(f"Successful: {success_count}")
    print(f"Failed: {fail_count}")

    if args.dry_run:
        print("\nDRY RUN COMPLETE - No actual changes were made")
        print("Run without --dry-run to perform the migration")
    else:
        print("\nMIGRATION COMPLETE!")
        print(f"\nConsolidated reports saved to: {DOCS_DIR}")
        print(f"Raw data organized in: {RAW_DIR}")
        print("\nNOTE: Original REPORT.md files preserved in original locations")

    return 0 if fail_count == 0 else 1

if __name__ == '__main__':
    sys.exit(main())
