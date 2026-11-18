#!/usr/bin/env bash
# Offline fixture tests for the layer check.
# Usage: bash scripts/check-layers-test.sh
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/check-layers.sh
temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
passed=0

expect() {
  local expected=$1 label=$2 status=0
  shift 2
  bash "$script" "$@" > "$temp/output" 2>&1 || status=$?
  if [[ "$status" != "$expected" ]]; then
    printf 'FAIL: %s (expected %s, got %s)\n' "$label" "$expected" "$status" >&2
    cat "$temp/output" >&2
    exit 1
  fi
  passed=$((passed + 1))
  printf 'PASS: %s\n' "$label"
}

contains() {
  if ! grep -Fq -- "$1" "$temp/output"; then
    printf 'FAIL: missing output: %s\n' "$1" >&2
    cat "$temp/output" >&2
    exit 1
  fi
}

reset_case() {
  rm -rf "$temp/src"
  mkdir -p "$temp/src"
  cat > "$temp/layers.md" <<'TABLE'
| Layer | Modules | May import |
| --- | --- | --- |
| L0 | low | none |
| L1 | mid | low |
| L2 | high | low, mid |
| L1 | side | low |
| L2 | pair_a, pair_b | low |

## Allowed exceptions

None.

## Other section
- `low -> high`: outside the exceptions section
TABLE
  printf 'use crate::low::X;\nuse crate::mid::Own;\npub use crate::low::X;\n' > "$temp/src/mid.rs"
  printf 'pub(crate) use crate::mid::Y;\n' > "$temp/src/high.rs"
}

check() {
  local expected=$1 label=$2
  shift 2
  expect "$expected" "$label" --root "$temp/src" --table "$temp/layers.md" "$@"
}

reset_case
check 0 'clean report-only tree'
contains 'Layer check: no violations (report-only)'
check 0 'clean enforcing tree' --enforce
contains 'Layer check: no violations (enforcing)'

printf 'use crate::high::Z;\n' > "$temp/src/low.rs"
check 1 'upward enforcing' --enforce
contains 'low.rs:1: low (L0) imports high (L2): upward'
check 0 'upward report-only'
contains 'low.rs:1: low (L0) imports high (L2): upward'
contains '1 violations (report-only)'

reset_case
printf 'use crate::side::X;\n' >> "$temp/src/mid.rs"
check 1 'sideways import' --enforce
contains 'mid.rs:4: mid (L1) imports side (L1): sideways'

reset_case
printf 'use crate::side::X;\n' >> "$temp/src/high.rs"
check 1 'lower module not listed' --enforce
contains 'high.rs:2: high (L2) imports side (L1): not listed'

reset_case
printf 'use crate::{low::A, high::B};\n' > "$temp/src/mid.rs"
check 1 'brace list checks only disallowed targets' --enforce
contains 'mid.rs:1: mid (L1) imports high (L2): upward'
contains '1 violations (enforcing)'
if grep -Fq 'imports low' "$temp/output"; then exit 1; fi
cat > "$temp/src/mid.rs" <<'RUST'

pub(crate) use crate::{
  low::{A, B},
  high::C,
};
RUST
check 1 'multi-line nested brace list reports starting line' --enforce
contains 'mid.rs:2: mid (L1) imports high (L2): upward'
contains '1 violations (enforcing)'
printf 'use crate::high::\n  Z;\n' > "$temp/src/mid.rs"
check 1 'multi-line path reports starting line' --enforce
contains 'mid.rs:1: mid (L1) imports high (L2): upward'

reset_case
printf 'use crate::{low::A, /* explanation */ high::B};\n' > "$temp/src/mid.rs"
check 1 'block comment before a brace-list target' --enforce
contains 'mid.rs:1: mid (L1) imports high (L2): upward'
contains '1 violations (enforcing)'
cat > "$temp/src/mid.rs" <<'RUST'
/* preceding comment
   use crate::high::Ignored;
*/
use crate::{low::A, /* explanation with ; } , // and a nested comment
  /* nested */ still inside the outer comment
*/ high::B};
RUST
check 1 'multi-line nested block comments preserve statement starting line' --enforce
contains 'mid.rs:4: mid (L1) imports high (L2): upward'
contains '1 violations (enforcing)'
cat > "$temp/src/mid.rs" <<'RUST'
use crate::{low::A, /* high::Ignored, */ low::B};
/*
use crate::high::Ignored;
*/
// /* line comments must not open a block comment
use crate::low::C;
RUST
check 0 'imports inside block comments remain ignored' --enforce
contains 'no violations'

reset_case
printf 'use crate::pair_b::X;\n' > "$temp/src/pair_a.rs"
printf 'use crate::pair_a::X;\n' > "$temp/src/pair_b.rs"
check 0 'imports in both directions within a unit' --enforce
contains 'no violations'

reset_case
printf 'use crate::low::X;\n' > "$temp/src/unknown.rs"
check 1 'importer not in table' --enforce
contains 'unknown.rs:1: unknown (L?) imports low (L0): not in table'
reset_case
printf 'use crate::unknown::X;\n' > "$temp/src/low.rs"
check 1 'target not in table' --enforce
contains 'low.rs:1: low (L0) imports unknown (L?): not in table'

reset_case
cat > "$temp/src/low.rs" <<'RUST'
#[cfg(test)]
mod tests { use crate::high::Z; }
#[cfg(test)]
mod tests;
#[cfg(test)]
use crate::high::Z;
RUST
check 0 'test module block, declaration and use are skipped' --enforce
contains 'no violations'
printf 'use crate::high::After;\n' >> "$temp/src/low.rs"
check 1 'violation after skipped items' --enforce
contains 'low.rs:7: low (L0) imports high (L2): upward'
contains '1 violations (enforcing)'
cat > "$temp/src/low.rs" <<'RUST'
#[cfg(test)]
#[allow(
  unused
)]
mod tests {
  // } brace in a comment must not end the item
  fn nested() { use crate::high::Z; }
  use crate::high::Z;
}
use crate::high::After;
RUST
check 1 'attributes and nested braces preserve test-item boundary' --enforce
contains 'low.rs:10: low (L0) imports high (L2): upward'
contains '1 violations (enforcing)'

reset_case
printf '#[cfg(any(test, feature = "test-support"))]\nuse crate::high::Z;\n' > "$temp/src/low.rs"
check 1 'mixed test and feature cfg remains checked' --enforce
contains 'low.rs:2: low (L0) imports high (L2): upward'

reset_case
mkdir -p "$temp/src/mid/tests" "$temp/src/test_support"
for path in tests.rs mid/tests.rs mid/tests/case.rs test_support/mod.rs test_support.rs lib.rs; do
  printf 'use crate::high::Z;\n' > "$temp/src/$path"
done
check 0 'test files, test_support and lib.rs are skipped' --enforce
contains 'no violations'
mkdir -p "$temp/src/mid"
printf 'use crate::high::Z;\n' > "$temp/src/mid/part.rs"
check 1 'directory file belongs to its top-level module' --enforce
contains 'mid/part.rs:1: mid (L1) imports high (L2): upward'

reset_case
printf 'use crate::high::Z;\n' > "$temp/src/low.rs"
sed 's/None\./- `low -> high`: fixture/' "$temp/layers.md" > "$temp/exception.md"
mv "$temp/exception.md" "$temp/layers.md"
check 0 'exception permits an upward import' --enforce
contains 'no violations'
sed 's/^- `low -> high`: fixture/None./' "$temp/layers.md" > "$temp/exception.md"
mv "$temp/exception.md" "$temp/layers.md"
check 1 'None restores upward failure' --enforce
contains 'low.rs:1: low (L0) imports high (L2): upward'

reset_case
printf '# No rows\n' > "$temp/layers.md"
check 2 'table with no rows' --enforce
contains 'Layer table error: no table rows'
reset_case
printf '| L3 | low | mid |\n' >> "$temp/layers.md"
check 2 'module listed twice' --enforce
contains 'Layer table error: duplicate module low'

reset_case
check 2 'unknown option' --unknown
expect 2 'root without value' --root
expect 2 'table without value' --table
check 2 'missing root' --root "$temp/missing"
check 2 'missing table' --table "$temp/missing.md"

reset_case
printf '\nuse crate::high::Z;\n' > "$temp/src/low.rs"
printf 'use crate::side::Z;\n' >> "$temp/src/mid.rs"
check 1 'two violations are printed and counted' --enforce
contains 'low.rs:2: low (L0) imports high (L2): upward'
contains 'mid.rs:4: mid (L1) imports side (L1): sideways'
contains '2 violations (enforcing)'
# Compare complete output to verify sorting and absence of extra diagnostics.
cat > "$temp/expected" <<'OUTPUT'
low.rs:2: low (L0) imports high (L2): upward
mid.rs:4: mid (L1) imports side (L1): sideways
Layer check: 2 violations (enforcing)
OUTPUT
diff -u "$temp/expected" "$temp/output"

printf '%s checks passed\n' "$passed"
