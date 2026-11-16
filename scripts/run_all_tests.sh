#!/usr/bin/env bash
# Run every test suite that can run here and print a scoreboard table.
#
# Usage: scripts/run_all_tests.sh [output-file]
#   Full output goes to the output file (default: target/run_all_tests.log); the
#   table goes to stdout. Exit status is 1 if any suite has failures, else 0.
#
# What is counted: one row per cargo test binary, from its "test result:" line.
# A suite whose test ROMs are absent skips instead of failing (CLAUDE.md section 6);
# those show up in the SKIPPED column so a missing ROM cannot hide as a pass.
# Per-ROM detail printed by the recorded sweeps (Mooneye CGB-SUMMARY, ...) is
# appended below the table. Run it from WSL/Linux with the test ROMs fetched.

set -u
cd "$(dirname "$0")/.."
LOG="${1:-target/run_all_tests.log}"
mkdir -p "$(dirname "$LOG")"

# --nocapture so SKIP lines and per-ROM summaries are visible; --test-threads=1 keeps
# those lines from interleaving between tests of one binary.
cargo test --workspace --no-fail-fast -- --nocapture --test-threads=1 >"$LOG" 2>&1
CARGO_STATUS=$?

awk '
  /^ *Running / {
    # "Running tests/mooneye.rs (path)" or "Running unittests src/lib.rs (path)"
    name = $2; if (name == "unittests") {
      # name the crate: the binary is target/.../deps/gb_core-<hash>
      crate = $4; sub(/.*\//, "", crate); sub(/-[0-9a-f]+\)?$/, "", crate)
      name = "unit: " crate
    }
    sub(/^tests\//, "", name); sub(/\.rs$/, "", name)
    suite = name; skipped[suite] = 0
    next
  }
  /^ *Doc-tests / { suite = "doc-tests " $2; next }
  /SKIP/ { if (suite != "") skipped[suite]++ }
  /^test result:/ {
    p = $4; f = $6; ig = $8
    printf "%-34s %7d %7d %7d %8d\n", suite, p, f, ig, skipped[suite]
    tp += p; tf += f; ti += ig; ts += skipped[suite]
  }
  END { printf "%-34s %7d %7d %7d %8d\n", "TOTAL", tp, tf, ti, ts; if (tf > 0) exit 1 }
' "$LOG" >"$LOG.table"
AWK_STATUS=$?

printf "%-34s %7s %7s %7s %8s\n" "suite" "passed" "failed" "ignored" "SKIPPED"
cat "$LOG.table"
rm -f "$LOG.table"
echo
echo "per-ROM summaries printed by the sweeps:"
grep -E "SUMMARY|: [0-9]+ passed, [0-9]+ failed|pokemon_[a-z0-9.]*: [0-9]+ hashes|^instr_timing:|^mem_timing|^[0-9]+-.*: (PASS|FAIL)" "$LOG" | sed 's/^/  /'
echo
echo "full log: $LOG (cargo exit $CARGO_STATUS)"
[ "$CARGO_STATUS" -eq 0 ] && [ "$AWK_STATUS" -eq 0 ]
