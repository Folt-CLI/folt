#!/usr/bin/env bash
# Usage: scripts/check-file-size.sh [--root <dir>] [--limit <n>]
# Fail when a production Rust file has more lines than the limit. Skips
# tests.rs, files under a tests directory and files under test_support.
# There is no exception list.
set -euo pipefail

usage() {
  printf 'Usage: scripts/check-file-size.sh [--root <dir>] [--limit <n>]\n' >&2
  exit 2
}

repo=$(cd "$(dirname "$0")/.." && pwd)
root=$repo/crates
limit=200
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || usage
      root=$2
      [[ "$root" == /* ]] || root=$repo/$root
      shift 2
      ;;
    --limit)
      [[ $# -ge 2 && "$2" =~ ^[0-9]+$ ]] || usage
      limit=$2
      shift 2
      ;;
    *) usage ;;
  esac
done
[[ -d "$root" ]] || usage
root=$(cd "$root" && pwd)
export LC_ALL=C

count=0
while IFS= read -r file; do
  lines=$(wc -l < "$root/$file")
  lines=${lines//[[:space:]]/}
  if ((lines > limit)); then
    printf '%s: %s lines (limit %s)\n' "$file" "$lines" "$limit"
    count=$((count + 1))
  fi
done < <(cd "$root" && find . -type f -name '*.rs' ! -name tests.rs \
  ! -path '*/tests/*' ! -path '*/test_support/*' | sed 's|^\./||' | sort)

if ((count > 0)); then
  printf 'File size check: %s files over %s\n' "$count" "$limit"
  exit 1
fi
printf 'File size check: no files over %s\n' "$limit"
