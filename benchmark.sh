#!/usr/bin/env bash
#
# benchmark.sh - Comprehensive benchmarking script for Bog
#
# Usage:
#   ./benchmark.sh              # Quick mode (critical benchmarks)
#   ./benchmark.sh --full       # Full mode (all benchmarks)
#   ./benchmark.sh --platform M1  # Override platform detection
#   ./benchmark.sh --no-clean   # Skip cargo clean
#   ./benchmark.sh --help       # Show usage
#

set -euo pipefail

# Script configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARKS_DIR="${SCRIPT_DIR}/benchmarks"
DOCS_DIR="${SCRIPT_DIR}/docs/benchmarks"
RAW_DIR="${BENCHMARKS_DIR}/raw"

# Default options
FULL_MODE=false
PLATFORM=""
DO_CLEAN=true
COMPARE_BASELINE=true

# Benchmark list (all bog-core benchmarks)
QUICK_BENCHMARKS=(
    "engine_bench"
    "conversion_bench"
    "throughput_bench"
)

FULL_BENCHMARKS=(
    "engine_bench"
    "conversion_bench"
    "atomic_bench"
    "fill_processing_bench"
    "inventory_strategy_bench"
    "tls_overhead_bench"
    "multi_tick_bench"
    "circuit_breaker_bench"
    "depth_bench"
    "throughput_bench"
    "order_fsm_bench"
    "reconciliation_bench"
    "resilience_bench"
)

#==============================================================================
# Helper Functions
#==============================================================================

print_header() {
    echo ""
    echo "========================================================================"
    echo "$1"
    echo "========================================================================"
    echo ""
}

print_step() {
    echo "> $1"
}

print_success() {
    echo "✓ $1"
}

print_warning() {
    echo "⚠ $1"
}

print_error() {
    echo "✗ $1"
}

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Comprehensive benchmarking script for Bog that runs benchmarks, processes
results, compares against baselines, and generates consolidated reports.

OPTIONS:
    --full              Run all benchmarks (default: quick mode)
    --platform NAME     Override platform detection (e.g., M1, M2, c6in_xlarge)
    --no-clean          Skip 'cargo clean' before benchmarks
    --no-compare        Skip regression comparison against baseline
    --help              Show this help message

EXAMPLES:
    # Quick benchmark with auto-detected platform
    ./benchmark.sh

    # Full benchmark suite
    ./benchmark.sh --full

    # Override platform name
    ./benchmark.sh --platform c6in_xlarge

OUTPUT:
    Raw data:  benchmarks/raw/YYYY-MM/YYYY-MM-DD_HHmmss_platform/
    Report:    docs/benchmarks/YYYY-MM/YYYY-MM-DD_HHmmss_platform.md

EOF
}

#==============================================================================
# Platform Detection
#==============================================================================

detect_platform() {
    local os_type="$(uname -s)"
    local cpu_brand=""

    if [[ "$os_type" == "Darwin" ]]; then
        cpu_brand="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "")"
        if echo "$cpu_brand" | grep -qi "Apple M1"; then
            echo "M1"
        elif echo "$cpu_brand" | grep -qi "Apple M2"; then
            echo "M2"
        elif echo "$cpu_brand" | grep -qi "Apple M3"; then
            echo "M3"
        else
            echo "macOS"
        fi
    elif [[ "$os_type" == "Linux" ]]; then
        cpu_brand="$(lscpu | grep "Model name" | cut -d':' -f2 | xargs || echo "")"

        if command -v ec2-metadata &>/dev/null || [[ -f /sys/devices/virtual/dmi/id/product_name ]]; then
            local product_name="$(cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null || echo "")"
            if echo "$product_name" | grep -qi "c6in"; then
                echo "c6in_xlarge"
            elif echo "$product_name" | grep -qi "c7g"; then
                echo "c7g_xlarge"
            else
                echo "aws_linux"
            fi
        elif echo "$cpu_brand" | grep -qi "AMD"; then
            echo "x86_64_amd"
        elif echo "$cpu_brand" | grep -qi "Intel"; then
            echo "x86_64_intel"
        else
            echo "linux"
        fi
    else
        echo "unknown"
    fi
}

#==============================================================================
# System Information Collection
#==============================================================================

collect_system_info() {
    local platform="$1"
    local info_file="$2"

    print_step "Collecting system information..."

    {
        echo "PLATFORM=$platform"
        echo "DATE=$(date +%Y-%m-%d)"
        echo "TIMESTAMP=$(date +%s)"

        echo "OS=$(uname -s)"
        echo "OS_VERSION=$(uname -r)"
        echo "ARCH=$(uname -m)"

        if [[ "$(uname -s)" == "Darwin" ]]; then
            echo "CPU=$(sysctl -n machdep.cpu.brand_string)"
            echo "CPU_CORES_PHYSICAL=$(sysctl -n hw.physicalcpu)"
            echo "CPU_CORES_LOGICAL=$(sysctl -n hw.logicalcpu)"
            echo "CPU_FREQ=$(sysctl -n hw.cpufrequency 2>/dev/null || echo 'N/A')"
            echo "RAM=$(sysctl -n hw.memsize | awk '{printf "%.0f GB", $1/1024/1024/1024}')"
        elif [[ "$(uname -s)" == "Linux" ]]; then
            echo "CPU=$(lscpu | grep 'Model name' | cut -d':' -f2 | xargs)"
            echo "CPU_CORES_PHYSICAL=$(lscpu | grep '^CPU(s):' | awk '{print $2}')"
            echo "CPU_CORES_LOGICAL=$(nproc)"
            echo "CPU_FREQ=$(lscpu | grep 'MHz' | head -1 | awk '{print $3}' || echo 'N/A')"
            echo "RAM=$(free -h | grep Mem | awk '{print $2}')"
        fi

        echo "RUST_VERSION=$(rustc --version)"
        echo "CARGO_VERSION=$(cargo --version)"

        echo "GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo 'N/A')"
        echo "GIT_BRANCH=$(git branch --show-current 2>/dev/null || echo 'N/A')"
        echo "GIT_DIRTY=$(git diff --quiet && echo 'clean' || echo 'dirty')"

        echo "BOG_VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)"
    } > "$info_file"

    print_success "System information collected"
}

#==============================================================================
# Benchmark Execution
#==============================================================================

run_benchmark() {
    local bench_name="$1"
    local output_file="$2"

    print_step "Running benchmark: ${bench_name}..."

    if cargo bench --bench "$bench_name" > "$output_file" 2>&1; then
        print_success "Benchmark completed: ${bench_name}"
        return 0
    else
        print_error "Benchmark failed: ${bench_name}"
        return 1
    fi
}

#==============================================================================
# Report Generation
#==============================================================================

generate_consolidated_report() {
    local platform="$1"
    local raw_run_dir="$2"
    local output_file="$3"
    local baseline_file="$4"

    print_step "Generating consolidated report..."

    local compare_args=""
    if [[ -n "$baseline_file" && -f "$baseline_file" ]]; then
        compare_args="--compare-with \"$baseline_file\""
        print_step "Comparing against baseline: $(basename "$baseline_file")"
    fi

    if python3 "${BENCHMARKS_DIR}/process_benchmarks.py" \
        "$raw_run_dir" \
        "$output_file" \
        --platform "$platform" \
        $compare_args; then
        print_success "Report generated: $output_file"
        return 0
    else
        print_error "Report generation failed"
        return 1
    fi
}

find_latest_baseline() {
    local platform="$1"

    local baseline=""
    local latest_timestamp=0

    for md_file in "${DOCS_DIR}"/*/*_${platform}.md; do
        if [[ -f "$md_file" ]]; then
            local timestamp=$(basename "$md_file" | grep -oP '\d{4}-\d{2}-\d{2}_\d{6}' || echo "")
            if [[ -n "$timestamp" ]]; then
                local comparable=$(echo "$timestamp" | tr -d '-_')
                if [[ "$comparable" -gt "$latest_timestamp" ]]; then
                    latest_timestamp="$comparable"
                    baseline="$md_file"
                fi
            fi
        fi
    done

    echo "$baseline"
}

#==============================================================================
# Main Execution
#==============================================================================

main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --full)
                FULL_MODE=true
                shift
                ;;
            --platform)
                PLATFORM="$2"
                shift 2
                ;;
            --no-clean)
                DO_CLEAN=false
                shift
                ;;
            --no-compare)
                COMPARE_BASELINE=false
                shift
                ;;
            --help)
                usage
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    print_header "BOG BENCHMARK SUITE"

    # Detect platform
    if [[ -z "$PLATFORM" ]]; then
        PLATFORM=$(detect_platform)
        print_step "Auto-detected platform: ${PLATFORM}"
    else
        print_step "Using specified platform: ${PLATFORM}"
    fi

    # Determine benchmarks
    local benchmarks=()
    if [[ "$FULL_MODE" == true ]]; then
        benchmarks=("${FULL_BENCHMARKS[@]}")
        print_step "Mode: FULL (${#benchmarks[@]} benchmarks)"
    else
        benchmarks=("${QUICK_BENCHMARKS[@]}")
        print_step "Mode: QUICK (${#benchmarks[@]} benchmarks)"
    fi

    # Generate timestamp and directory structure
    local timestamp=$(date +%Y-%m-%d_%H%M%S)
    local date_path=$(date +%Y-%m)
    local run_name="${timestamp}_${PLATFORM}"

    local raw_run_dir="${RAW_DIR}/${date_path}/${run_name}"
    mkdir -p "$raw_run_dir"

    local output_md_dir="${DOCS_DIR}/${date_path}"
    mkdir -p "$output_md_dir"
    local output_md="${output_md_dir}/${run_name}.md"

    print_step "Raw data directory: ${raw_run_dir}"
    print_step "Output report: ${output_md}"

    # Collect system information
    local system_info="${raw_run_dir}/system_info.env"
    collect_system_info "$PLATFORM" "$system_info"

    # Clean build if requested
    if [[ "$DO_CLEAN" == true ]]; then
        print_step "Cleaning previous build artifacts..."
        cargo clean
        print_success "Clean complete"
    fi

    # Run benchmarks
    print_header "RUNNING BENCHMARKS"

    local completed_benchmarks=()
    local failed_benchmarks=()

    for bench in "${benchmarks[@]}"; do
        local raw_file="${raw_run_dir}/${bench}.txt"

        if run_benchmark "$bench" "$raw_file"; then
            completed_benchmarks+=("$bench")
        else
            failed_benchmarks+=("$bench")
        fi

        echo ""
    done

    # Generate consolidated report
    if [[ ${#completed_benchmarks[@]} -gt 0 ]]; then
        print_header "GENERATING CONSOLIDATED REPORT"

        local baseline_file=""
        if [[ "$COMPARE_BASELINE" == true ]]; then
            baseline_file=$(find_latest_baseline "$PLATFORM")
            if [[ -n "$baseline_file" ]]; then
                print_step "Found baseline: $(basename "$baseline_file")"
            else
                print_step "No baseline found for platform: $PLATFORM"
            fi
        fi

        generate_consolidated_report "$PLATFORM" "$raw_run_dir" "$output_md" "$baseline_file"
    fi

    # Summary
    print_header "BENCHMARK SUMMARY"

    echo "Completed: ${#completed_benchmarks[@]}/${#benchmarks[@]} benchmarks"

    if [[ ${#failed_benchmarks[@]} -gt 0 ]]; then
        echo "Failed: ${#failed_benchmarks[@]}"
        for failed in "${failed_benchmarks[@]}"; do
            echo "  - $failed"
        done
    fi

    echo ""
    echo "Results saved to:"
    echo "  Report: ${output_md}"
    echo "  Raw data: ${raw_run_dir}/"
    echo ""

    if [[ ${#failed_benchmarks[@]} -gt 0 ]]; then
        exit 1
    fi
}

main "$@"
