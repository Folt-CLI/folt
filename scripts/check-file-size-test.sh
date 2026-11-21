#!/usr/bin/env bash
# Offline fixture tests for the file size check.
# Usage: bash scripts/check-file-size-test.sh
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/check-file-size.sh
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
  if ! grep -Fxq -- "$1" "$temp/output"; then
    printf 'FAIL: missing output line: %s\n' "$1" >&2
    cat "$temp/output" >&2
    exit 1
  fi
}

lines() {
  local n=$1 i
  for ((i = 0; i < n; i++)); do printf '// line\n'; done
}

reset_case() {
  rm -rf "$temp/src"
  mkdir -p "$temp/src"
}

check() {
  local expected=$1 label=$2
  shift 2
  expect "$expected" "$label" --root "$temp/src" "$@"
}

reset_case
lines 200 > "$temp/src/exact.rs"
check 0 'file at exactly the limit passes'
contains 'File size check: no files over 200'

lines 201 > "$temp/src/big.rs"
check 1 'one line over fails'
contains 'big.rs: 201 lines (limit 200)'
contains 'File size check: 1 files over 200'

reset_case
mkdir -p "$temp/src/tests" "$temp/src/test_support" "$temp/src/mid/tests"
for path in tests.rs mid/tests.rs tests/case.rs mid/tests/case.rs test_support/mod.rs; do
  lines 300 > "$temp/src/$path"
done
check 0 'tests.rs, tests directory and test_support are skipped'
contains 'File size check: no files over 200'

reset_case
mkdir -p "$temp/src/a/b"
lines 250 > "$temp/src/a/b/deep.rs"
check 1 'nested file is reported with its relative path'
contains 'a/b/deep.rs: 250 lines (limit 200)'

reset_case
mkdir -p "$temp/src/z"
lines 201 > "$temp/src/z/last.rs"
lines 202 > "$temp/src/first.rs"
check 1 'two offenders print sorted and counted'
cat > "$temp/expected" <<'OUTPUT'
first.rs: 202 lines (limit 200)
z/last.rs: 201 lines (limit 200)
File size check: 2 files over 200
OUTPUT
diff -u "$temp/expected" "$temp/output"

reset_case
lines 11 > "$temp/src/small.rs"
check 1 'limit override fails' --limit 10
contains 'small.rs: 11 lines (limit 10)'
check 0 'limit override passes' --limit 11

check 2 'unknown option' --unknown
expect 2 'root without value' --root
expect 2 'limit without value' --limit
expect 2 'limit not a number' --limit abc
check 2 'missing root' --root "$temp/missing"

printf '%s checks passed\n' "$passed"
