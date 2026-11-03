#!/usr/bin/env bash
# Offline integration tests; each case uses disposable Git history.
# Usage: bash scripts/test-integrity-test.sh
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/test-integrity
temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
git -c init.defaultBranch=base -c init.templateDir= init -q "$temp/repo"
cd "$temp/repo"
git config user.name 'Test integrity test'
git config user.email 'test-integrity@example.invalid'
git config commit.gpgsign false
git config core.hooksPath /dev/null
git config core.autocrlf false
cat > existing.rs <<'EOF'
#[test]
fn existing() {
    assert_eq!(1, 1);
}
EOF
printf '// Destination for moved tests.\n' > destination.rs
printf 'assert_eq!(1, 1);\n' > notes.md
git add .
git commit -q -m 'Initial test fixtures'
base=$(git rev-parse HEAD)
passed=0

reset_case() {
  git checkout -q --detach "$base"
  git clean -fdq
}

commit_case() {
  git add .
  printf '%s\n' "${1-Change fixtures}" | git -c commit.cleanup=verbatim commit -q -F -
}

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

expect 0 'no change' "$base"
cat >> existing.rs <<'EOF'
#[test]
fn added() {
    assert_ne!(1, 2);
}
EOF
commit_case
expect 0 'added test' "$base"

reset_case
sed '/assert_eq!/d' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case
expect 1 'removed assert' "$base"
grep -Fq 'existing.rs:3: -    assert_eq!(1, 1);' "$temp/output"
grep -Fq 'Test-change: <non-empty reason>' "$temp/output"
git commit -q --allow-empty -m $'Explain removal\n\nTest-change: Redundant assertion covered elsewhere'
expect 0 'removed assert with trailer in another Git commit' "$base"
grep -Fq 'Test-change: Redundant assertion covered elsewhere' "$temp/output"

reset_case
sed 's/assert_eq!(1, 1)/assert_eq!(2, 2)/' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case
expect 1 'edited assert' "$base"
git commit -q --allow-empty -m $'Explain edit\n\nTest-change: Update expected behaviour'
expect 0 'edited assert with trailer' "$base"
grep -Fq 'Test-change: Update expected behaviour' "$temp/output"
expect 1 'explicit head excludes later trailer' "$base" HEAD~1

reset_case
cat existing.rs >> destination.rs
printf '// Test moved.\n' > existing.rs
commit_case
expect 0 'moved test between existing files' "$base"

reset_case
mv existing.rs new.rs
printf '// Test moved.\n' > existing.rs
commit_case
expect 0 'moved test into a new Rust file' "$base"

reset_case
printf 'changed\n' > notes.md
commit_case
expect 0 'non-Rust change' "$base"

reset_case
printf '#[test]\nfn new() { assert!(true); }\n' > new.rs
commit_case
expect 0 'new Rust file' "$base"

reset_case
sed 's/fn existing()/fn renamed()/' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case
expect 1 'edited test function after unchanged test attribute' "$base"

reset_case
sed '/#\[test\]/d' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case $'Change attribute\n\nTest-change:   '
expect 1 'removed test attribute and empty trailer' "$base"
git commit -q --allow-empty -m $'Explanation in body\n\nTest-change: not a trailer\n\nFollowing paragraph.'
expect 1 'body text is not a trailer' "$base"

for line in '    assert!(true);' '    assert_ne!(1, 2);' \
  '    debug_assert!(true);' '#[should_panic]'; do
  reset_case
  printf '%s\n' "$line" >> existing.rs
  commit_case
  fixture=$(git rev-parse HEAD)
  git checkout "$base" -- existing.rs
  commit_case
  expect 1 "removed $line" "$fixture"
done

reset_case
git rm -q existing.rs
commit_case
expect 0 'deleted file is outside common Rust files' "$base"

# Changes exclusive to a diverged PR base must not appear in the findings.
reset_case
printf 'head change\n' > notes.md
commit_case
head=$(git rev-parse HEAD)
reset_case
printf '    assert!(true);\n' >> existing.rs
commit_case
diverged_base=$(git rev-parse HEAD)
expect 0 'merge base excludes base-only assertion' "$diverged_base" "$head"

expect 2 'missing arguments'
expect 2 'too many arguments' "$base" HEAD extra
expect 2 'empty base argument' ''
expect 2 'empty head argument' "$base" ''
expect 2 'unavailable ref' missing-ref
grep -Fq 'Usage: scripts/test-integrity <base-ref> [<head-ref>]' "$temp/output"
printf '%s checks passed\n' "$passed"
