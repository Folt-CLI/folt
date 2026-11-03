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
printf '#[cfg(test)]\n// Destination for moved tests.\n' > destination.rs
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
mv existing.rs new_test.rs
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
expect 1 'deleted test file requires a trailer' "$base"
grep -Fq 'existing.rs:1: -#[test]' "$temp/output"
grep -Fq 'existing.rs:2: -fn existing() {' "$temp/output"
grep -Fq 'existing.rs:3: -    assert_eq!(1, 1);' "$temp/output"
git commit -q --allow-empty -m $'Explain deletion\n\nTest-change: Remove obsolete test file'
expect 0 'deleted test file with trailer' "$base"

reset_case
mv existing.rs production.rs
commit_case
expect 1 'renamed-away test file into production requires a trailer' "$base"

reset_case
mkdir tests
mv existing.rs tests/renamed.rs
commit_case
expect 0 'renamed-away test file into test context' "$base"

# Only additions in test context can excuse a removal, including for new files.
for destination in production.rs tests/moved.rs tests.rs moved_test.rs cfg.rs; do
  reset_case
  mkdir -p tests
  if [[ "$destination" == cfg.rs ]]; then
    # The cfg attribute is well outside the addition diff hunk.
    printf '#[cfg(test)]\n\n\n\n\n\n' > "$destination"
    commit_case
    fixture=$(git rev-parse HEAD)
  else
    fixture=$base
  fi
  cat existing.rs >> "$destination"
  printf '// Test moved.\n' > existing.rs
  commit_case
  expected=0
  [[ "$destination" != production.rs ]] || expected=1
  expect "$expected" "move into $destination" "$fixture"
done

reset_case
cat existing.rs > before_cfg.rs
printf '#[cfg(test)]\n' >> before_cfg.rs
printf '// Test moved.\n' > existing.rs
commit_case
expect 1 'production addition before cfg test does not excuse removal' "$base"

reset_case
printf '    assert_eq!(1, 1);\n' >> existing.rs
commit_case
fixture=$(git rev-parse HEAD)
printf '// Tests moved.\n' > existing.rs
# One added assertion must not excuse two removed assertions.
cat > destination.rs <<'EOF'
#[cfg(test)]
#[test]
fn existing() {
    assert_eq!(1, 1);
}
EOF
commit_case
expect 1 'each added test line excuses only one removal' "$fixture"
grep -Fq -- '-    assert_eq!(1, 1);' "$temp/output"

for macro in assert assert_eq assert_ne debug_assert debug_assert_eq debug_assert_ne; do
  reset_case
  cat > existing.rs <<EOF
#[test]
fn existing() {
    $macro!(
        calculate(
            1,
            2,
            3,
            4,
            5,
        ),
        6,
    );
    let after = 7;
}
EOF
  commit_case
  fixture=$(git rev-parse HEAD)
  sed 's/            5,/            9,/' existing.rs > edited.rs
  mv edited.rs existing.rs
  commit_case
  expect 1 "edited argument inside multi-line $macro outside opening hunk" "$fixture"
  grep -Fq 'existing.rs:9: -            5,' "$temp/output"
  # The outer closing parenthesis must end the assertion span.
  git checkout "$fixture" -- existing.rs
  sed 's/let after = 7/let after = 8/' existing.rs > edited.rs
  mv edited.rs existing.rs
  commit_case
  expect 0 "line after multi-line $macro is outside assertion" "$fixture"
done

reset_case
cat > existing.rs <<'EOF'
#[test]
fn existing() {
    assert_eq!(
        actual,
        expected,
    );
}
EOF
commit_case
fixture=$(git rev-parse HEAD)
sed '/        expected,/d' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case
expect 1 'removed argument in multi-line assertion with context opening' "$fixture"
grep -Fq 'existing.rs:5: -        expected,' "$temp/output"

reset_case
cat > existing.rs <<'EOF'
#[test]
fn existing() {
    assert_eq!(
        actual,
        expected,
    );
}
EOF
commit_case
fixture=$(git rev-parse HEAD)
printf '// Assertion removed.\n' > existing.rs
commit_case
expect 1 'removed multi-line assertion includes all argument and closing lines' "$fixture"
for removed in '3: -    assert_eq!(' '4: -        actual,' '5: -        expected,' '6: -    );'; do
  grep -Fq "existing.rs:$removed" "$temp/output"
done

for change in remove edit; do
  reset_case
  cat > existing.rs <<'EOF'
#[test]
#[should_panic(expected = "failure")]
fn existing() {
    panic!("failure");
}
EOF
  commit_case
  fixture=$(git rev-parse HEAD)
  if [[ "$change" == remove ]]; then
    sed '/#\[should_panic/d' existing.rs > edited.rs
  else
    sed 's/expected = "failure"/expected = "other"/' existing.rs > edited.rs
  fi
  mv edited.rs existing.rs
  commit_case
  expect 1 "$change should_panic attribute with arguments" "$fixture"
  grep -Fq 'existing.rs:2: -#[should_panic(expected = "failure")]' "$temp/output"
done

for change in remove edit; do
  reset_case
  cat > existing.rs <<'EOF'
#[test]
#[allow(unused)]
#[cfg(unix)]
#[allow(dead_code)]
#[allow(unused_variables)]
fn existing() {
    assert_eq!(1, 1);
}
EOF
  commit_case
  fixture=$(git rev-parse HEAD)
  if [[ "$change" == remove ]]; then
    sed '/fn existing()/d' existing.rs > edited.rs
  else
    sed 's/fn existing()/fn renamed()/' existing.rs > edited.rs
  fi
  mv edited.rs existing.rs
  commit_case
  expect 1 "$change test function after contiguous attributes beyond hunk" "$fixture"
  grep -Fq 'existing.rs:6: -fn existing() {' "$temp/output"
done

reset_case
cat > existing.rs <<'EOF'
#[test]
fn existing() {
    assert_eq!(1, 1);
}
fn production() {}
EOF
commit_case
fixture=$(git rev-parse HEAD)
sed 's/fn production()/fn changed()/' existing.rs > edited.rs
mv edited.rs existing.rs
commit_case
expect 0 'test attribute does not apply to a later production function' "$fixture"

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
