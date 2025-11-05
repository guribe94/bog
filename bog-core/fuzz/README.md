# Fuzz Testing for bog-core

This directory contains fuzz targets for testing critical bog-core functionality with arbitrary inputs.

## Prerequisites

Fuzzing requires nightly Rust:

```bash
# Install nightly toolchain
rustup install nightly

# Set nightly as default (or use +nightly with commands)
rustup default nightly
```

## Fuzz Targets

### 1. `fuzz_fixed_point_conversion`

**Tests**: `fixed_point::from_f64_checked()`

**Input**: Arbitrary f64 values (8 bytes)

**Checks**:
- No panics on any f64 value (including NaN, infinity)
- Correct error handling for out-of-range values
- Round-trip precision within bounds
- Consistent behavior between checked and unchecked versions

**Command**:
```bash
cargo fuzz run fuzz_fixed_point_conversion -- -max_total_time=300
```

### 2. `fuzz_position_overflow`

**Tests**: `Position` arithmetic operations

**Input**: Two i64 values (16 bytes) - initial position and delta

**Checks**:
- `update_quantity_checked()` correctly detects overflow
- `update_quantity_saturating()` never panics
- Checked vs saturating consistency
- PnL operations (realized_pnl, daily_pnl) behave correctly

**Command**:
```bash
cargo fuzz run fuzz_position_overflow -- -max_total_time=300
```

### 3. `fuzz_u64_conversion`

**Tests**: `fixed_point::from_u64_checked()` and `to_u64()`

**Input**: u64 values (8 bytes)

**Checks**:
- Correct boundary detection at i64::MAX
- to_u64() clamps negative values to 0
- Round-trip correctness for valid values
- No panics on any input

**Command**:
```bash
cargo fuzz run fuzz_u64_conversion -- -max_total_time=300
```

## Running Fuzzing

### Quick Test (5 minutes each)

```bash
# Test all targets for 5 minutes each
cargo fuzz run fuzz_fixed_point_conversion -- -max_total_time=300
cargo fuzz run fuzz_position_overflow -- -max_total_time=300
cargo fuzz run fuzz_u64_conversion -- -max_total_time=300
```

### Production Fuzzing Campaign (24 hours)

```bash
# Run overnight on CI/dedicated machine
cargo fuzz run fuzz_fixed_point_conversion -- -max_total_time=86400 &
cargo fuzz run fuzz_position_overflow -- -max_total_time=86400 &
cargo fuzz run fuzz_u64_conversion -- -max_total_time=86400 &
```

### Continuous Fuzzing

For continuous fuzzing in CI:

```bash
# Run with max_len to control input size
cargo fuzz run fuzz_fixed_point_conversion -- \
  -max_len=256 \
  -rss_limit_mb=2048 \
  -max_total_time=3600
```

## Interpreting Results

### Success

No output or:
```
#1234567: cov: 456 ft: 123 corp: 89 exec/s: 12345
```

### Crash Found

```
==12345==ERROR: AddressSanitizer: heap-buffer-overflow
SUMMARY: AddressSanitizer: heap-buffer-overflow
```

Crashes are saved to `fuzz/artifacts/fuzz_target_name/crash-*`.

### Reproducing Crashes

```bash
# Replay a specific crash
cargo fuzz run fuzz_fixed_point_conversion \
  fuzz/artifacts/fuzz_fixed_point_conversion/crash-abc123
```

## Corpus Management

### Location

Fuzz corpuses are stored in:
- `fuzz/corpus/fuzz_target_name/` - Interesting inputs
- `fuzz/artifacts/fuzz_target_name/` - Crashes/hangs

### Minimizing Corpus

```bash
# Reduce corpus to minimal interesting set
cargo fuzz cmin fuzz_fixed_point_conversion
```

### Trimming Inputs

```bash
# Minimize individual inputs
cargo fuzz tmin fuzz_fixed_point_conversion \
  fuzz/artifacts/fuzz_target_name/crash-abc123
```

## Integration with CI

### GitHub Actions Example

```yaml
name: Fuzz Testing

on:
  schedule:
    - cron: '0 0 * * *'  # Daily

jobs:
  fuzz:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - fuzz_fixed_point_conversion
          - fuzz_position_overflow
          - fuzz_u64_conversion

    steps:
      - uses: actions/checkout@v2

      - name: Install Rust nightly
        run: rustup toolchain install nightly

      - name: Install cargo-fuzz
        run: cargo +nightly install cargo-fuzz

      - name: Run fuzzer
        run: |
          cargo +nightly fuzz run ${{ matrix.target }} -- \
            -max_total_time=3600 \
            -rss_limit_mb=2048

      - name: Upload artifacts
        if: failure()
        uses: actions/upload-artifact@v2
        with:
          name: fuzz-artifacts-${{ matrix.target }}
          path: fuzz/artifacts/${{ matrix.target }}/
```

## Coverage Analysis

### Generate coverage report

```bash
# Run with coverage instrumentation
cargo fuzz coverage fuzz_fixed_point_conversion

# View coverage
cargo cov -- show \
  fuzz/target/*/release/fuzz_fixed_point_conversion \
  --format=html > coverage.html
```

## Expected Bugs

These fuzzing targets are designed to catch:

1. **Panics**: Any `panic!()`, `unwrap()`, or `expect()` in hot paths
2. **Integer overflows**: Arithmetic that wraps instead of returning errors
3. **Precision loss**: Float conversions that lose too much precision
4. **Undefined behavior**: Invalid memory access, data races
5. **Logic errors**: Incorrect overflow detection

## Performance

Typical fuzzing speeds:
- **fuzz_fixed_point_conversion**: ~100k exec/sec
- **fuzz_position_overflow**: ~80k exec/sec
- **fuzz_u64_conversion**: ~120k exec/sec

On modern hardware (M-series Mac, Ryzen), expect 500k-1M executions/second.

## Troubleshooting

### "error: needs nightly"

```bash
rustup install nightly
cargo +nightly fuzz run target_name
```

### "ASAN: out of memory"

Reduce RSS limit:
```bash
cargo fuzz run target -- -rss_limit_mb=1024
```

### "Slow fuzzing (<1000 exec/s)"

Check CPU governor:
```bash
# Linux
sudo cpupower frequency-set -g performance

# macOS
# Ensure power adapter is connected
```

## Security

Fuzz targets are safe to run:
- No network access
- No file system access (except corpus)
- Sandboxed by AddressSanitizer
- Time and memory limited

## Further Reading

- [The Fuzzing Book](https://www.fuzzingbook.org/)
- [cargo-fuzz guide](https://rust-fuzz.github.io/book/cargo-fuzz.html)
- [libFuzzer documentation](https://llvm.org/docs/LibFuzzer.html)
