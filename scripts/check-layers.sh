#!/usr/bin/env bash
# Usage: scripts/check-layers.sh [--enforce] [--root <src-dir>] [--table <layers.md>]
# Parse single-line or multi-line use crate:: statements, including visibility
# and brace lists, reporting their starting line and top-level target modules.
# Skip exactly #[cfg(test)] items through their semicolon or balanced braces,
# stripping // comments; other cfg forms remain checked. Skip tests.rs, tests/
# and test_support. A file belongs to its first path component (without .rs);
# lib.rs is skipped. Own-unit imports are allowed. Missing modules violate the
# table; otherwise listed imports and exceptions pass, and remaining imports
# are upward, sideways or not listed according to their layer numbers.
# Text heuristic only: inline crate:: paths, super:: paths and macros are not
# checked; braces or // inside strings can confuse test-item skipping.
set -euo pipefail

usage() {
  printf 'Usage: scripts/check-layers.sh [--enforce] [--root <src-dir>] [--table <layers.md>]\n' >&2
  exit 2
}

repo=$(cd "$(dirname "$0")/.." && pwd)
root=$repo/crates/folt-core/src
table=$repo/docs/dev/library/layers.md
mode=report-only
while [[ $# -gt 0 ]]; do
  case "$1" in
    --enforce) mode=enforcing; shift ;;
    --root|--table)
      [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || usage
      value=$2
      [[ "$value" == /* ]] || value=$repo/$value
      if [[ "$1" == --root ]]; then root=$value; else table=$value; fi
      shift 2
      ;;
    *) usage ;;
  esac
done
[[ -d "$root" && -f "$table" ]] || usage
root=$(cd "$root" && pwd)
export LC_ALL=C
temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT

awk '
  function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
  /^\| L[0-9]/ {
    rows++
    split($0, cols, "|")
    layer = trim(cols[2]); sub(/^L/, "", layer)
    count = split(cols[3], modules, ",")
    targets = split(cols[4], imports, ",")
    for (i = 1; i <= count; i++) {
      module = trim(modules[i])
      if (module in seen) {
        print "Layer table error: duplicate module " module > "/dev/stderr"
        bad = 1
      }
      seen[module] = 1
      print "M", module, layer, rows
      for (j = 1; j <= targets; j++) {
        target = trim(imports[j])
        if (target != "none" && target != "") print "A", module, target
      }
    }
  }
  /^## / { exceptions = ($0 == "## Allowed exceptions") }
  exceptions && /^- `[a-z_][a-z_0-9]* -> [a-z_][a-z_0-9]*`(:|[[:space:]])/ {
    split($0, parts, "`")
    split(parts[2], edge, " -> ")
    print "A", edge[1], edge[2]
  }
  END {
    if (!rows) {
      print "Layer table error: no table rows" > "/dev/stderr"
      bad = 1
    }
    if (bad) exit 2
  }
' "$table" > "$temp/table"

find "$root" -type f -name '*.rs' -print0 > "$temp/paths"
: > "$temp/violations"
while IFS= read -r -d '' file; do
  path=${file#"$root"/}
  module=${path%%/*}
  module=${module%.rs}
  case "/$path" in
    /lib.rs|*/tests.rs|*/tests/*) continue ;;
  esac
  [[ "$module" != test_support ]] || continue
  # Environment variables keep backslashes in paths literal for awk.
  export LAYER_CHECK_PATH=$path LAYER_CHECK_MODULE=$module
  awk '
    FNR == NR {
      if ($1 == "M") { layer[$2] = $3; unit[$2] = $4 }
      if ($1 == "A") allowed[$2 SUBSEP $3] = 1
      next
    }
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    function report(target, kind, from, to) {
      if (target == "") return
      if (!(importer in layer) || !(target in layer)) kind = "not in table"
      else if (unit[importer] == unit[target] || allowed[importer SUBSEP target]) return
      else if (layer[target] > layer[importer]) kind = "upward"
      else if (layer[target] == layer[importer]) kind = "sideways"
      else kind = "not listed"
      from = (importer in layer) ? layer[importer] : "?"
      to = (target in layer) ? layer[target] : "?"
      printf "%s:%d: %s (L%s) imports %s (L%s): %s\n", ENVIRON["LAYER_CHECK_PATH"], start, importer, from, target, to, kind
    }
    function item(s) {
      s = trim(s)
      if (match(s, /^[a-zA-Z_][a-zA-Z_0-9]*/)) report(substr(s, RSTART, RLENGTH))
    }
    function statement(s, i, c, depth, part) {
      s = trim(s)
      if (substr(s, 1, 1) != "{") { item(s); return }
      depth = 0; part = ""
      for (i = 2; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (depth == 0 && (c == "," || c == "}")) {
          item(part); part = ""
          if (c == "}") return
        } else {
          part = part c
          if (c == "{") depth++
          if (c == "}") depth--
        }
      }
    }
    {
      importer = ENVIRON["LAYER_CHECK_MODULE"]
      line = $0; sub(/\/\/.*/, "", line)
      clean = trim(line)
      if (!skip && clean == "#[cfg(test)]") { skip = 1; next }
      if (skip) {
        # Attributes may span lines and precede the item.
        if (!opened && (attribute || clean ~ /^#\[/)) {
          attribute = (clean !~ /\][[:space:]]*$/)
          next
        }
        for (i = 1; i <= length(line); i++) {
          c = substr(line, i, 1)
          if (c == "{") { opened = 1; depth++ }
          if (c == "}" && opened) {
            depth--
            if (depth == 0) { skip = opened = 0; break }
          }
          if (c == ";" && !opened) { skip = 0; break }
        }
        next
      }
      if (!collecting && line ~ /^[[:space:]]*(pub([[:space:]]+|\([^)]*\)[[:space:]]+))?use[[:space:]]+crate::/) {
        start = FNR; collecting = 1; buffer = ""
        sub(/^[[:space:]]*(pub([[:space:]]+|\([^)]*\)[[:space:]]+))?use[[:space:]]+crate::/, "", line)
      }
      if (collecting) {
        buffer = buffer " " line
        if (index(buffer, ";")) {
          statement(substr(buffer, 1, index(buffer, ";") - 1))
          collecting = 0
        }
      }
    }
  ' "$temp/table" "$file" >> "$temp/violations"
done < "$temp/paths"
sort -t: -k1,1 -k2,2n "$temp/violations"
count=$(wc -l < "$temp/violations")
count=${count//[[:space:]]/}
if [[ "$count" == 0 ]]; then
  printf 'Layer check: no violations (%s)\n' "$mode"
else
  printf 'Layer check: %s violations (%s)\n' "$count" "$mode"
fi
if [[ "$mode" == enforcing && "$count" -gt 0 ]]; then exit 1; fi
