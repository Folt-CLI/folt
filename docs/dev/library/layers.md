# Layers of folt-core

Top-level modules of `crates/folt-core/src` follow the import permissions below because a new upward import can silently reintroduce a cycle; see [Folt-CLI/folt#203](https://github.com/Folt-CLI/folt/issues/203). Here, “layer” means the import level of a top-level module of `folt-core`, not the glossary field.

## The rule

A module may import from another module only when its table row lists that module. A module may always import from itself and from the other modules in its own unit (the modules sharing one row). Nothing may import from a higher layer. Same-layer imports between different rows are allowed only when the table lists them. The check reads `use crate::` statements only.

## Layer table

| Layer | Modules | May import |
| --- | --- | --- |
| L0 | config | none |
| L0 | layout | none |
| L0 | id | config |
| L1 | fsx | config |
| L1 | ref_name | config, id |
| L1 | front_matter | config, id |
| L2 | object | config, id, ref_name |
| L3 | home | config, layout, fsx, object |
| L4 | folio | config, layout, id, ref_name, fsx, home |
| L5 | store | config, layout, id, object, folio, fsx, home |
| L5 | placement_path | config, id, folio |
| L6 | placement, placement_fs | config, layout, id, object, folio, store, fsx, home |
| L6 | history | config, id, ref_name, object, folio, store, fsx, home |
| L7 | registry | config, layout, id, object, folio, placement, placement_fs, placement_path, front_matter, fsx, home |
| L8 | ops | config, layout, id, fsx, ref_name, front_matter, object, home, folio, store, placement_path, placement, placement_fs, history, registry |

`none` means std and external crates only.

The `ops` internal order is `refusal`, `facts`, `working`, `status`, `step_hook`, `stage`, `placement`, `history`, `register`, `rename`, `checkout`, `prune`, then `facade`. `placement` imports `stage::refuse`, so `stage` comes first. `rollback` imports only `fsx`. The order is also stated in `crates/folt-core/src/ops/mod.rs`.

`test_support` and all `cfg(test)` code are outside the table.

`folio` now holds the Folio directory, folio-name, format, row and ref-file items. `placement_fs` is a flat file beside `placement`. The ref store lives under `history` (`history.rs` with `ref_store`, `expect` and `error` submodules); the L8 history operations live in `ops/history`. Working-file gathering and resolution live in `ops/working`, working status in `ops/status`, and stage, rename, checkout and prune operations in their own `ops` modules. Registration operations now live in `ops/register`.

The file-level corrections are `id` importing `config`, the existing `folio` in the L4 unit and its import permissions in dependent rows, and the existing `placement_path` at L5 with imports from `config`, `id` and L4 `folio`. `registry` also imports `placement_path`. The current `home -> placement_path` import is upward and remains a reported violation; listing it cannot make a higher-layer import conform to the rule.

## Allowed exceptions

None.

For a later exception, use one bullet per import with this format (shown indented so it is not an active exception):

  - `importer -> imported`: reason

## Running the check

Run these from the repository root:

```sh
grep -n '^\(pub \)\?mod \|^#\[cfg' crates/folt-core/src/lib.rs
grep -rhoE '^[[:space:]]*(pub(\([a-z]+\))? )?use crate::[a-z_]+' crates/folt-core/src | sed -E 's/.*crate:://' | sort -u
bash -n scripts/check-layers.sh scripts/check-layers-test.sh scripts/check-file-size.sh scripts/check-file-size-test.sh
# If installed:
shellcheck scripts/check-layers.sh scripts/check-layers-test.sh scripts/check-file-size.sh scripts/check-file-size-test.sh
bash scripts/check-layers-test.sh
scripts/check-layers.sh
echo $?
scripts/check-layers.sh --enforce
cargo test --locked --workspace
git status --short
git diff --stat -- docs/dev/README.md
```

Report-only is the default and exits 0 even when it reports violations. `--enforce` exits 1 on violations; both modes exit 0 when there are none. Invalid arguments, a missing source directory or table, no table rows, or a module in two rows exit 2 with a diagnostic on stderr. Violations appear on stdout sorted by relative path and numeric starting line, followed by a count and the mode. Unknown module levels appear as `L?`.

The default paths are resolved from the repository root, so the script works from any working directory. `--root <src-dir>` and `--table <layers.md>` select alternate inputs, including paths outside the repository; relative arguments are also resolved from the repository root.

To verify detection in a disposable source tree:

```sh
copy=$(mktemp -d)
cp -R crates/folt-core/src "$copy/src"
printf 'use crate::registry::Registry;\n' >> "$copy/src/id/folio.rs"
scripts/check-layers.sh --enforce --root "$copy/src"
scripts/check-layers.sh --root "$copy/src"
sed '$d' "$copy/src/id/folio.rs" > "$copy/folio.rs"
cat "$copy/folio.rs" > "$copy/src/id/folio.rs"
printf '#[cfg(test)]\nmod fixture { use crate::registry::Registry; }\n' >> "$copy/src/id/folio.rs"
scripts/check-layers.sh --enforce --root "$copy/src"
rm -rf "$copy"
```

The inserted import adds one upward violation in `id/folio.rs`; replacing it with a test-only item removes that violation. Existing violations in the source tree remain in both cases.

The gate and CI run `scripts/check-layers.sh --enforce`, so a violation fails both. Report-only mode (no flag) remains an option for local use.

## What the script checks

1. Parse a statement starting on a line with optional indentation, optional `pub` or `pub(...)` visibility, then `use crate::`, through its first `;`, including multi-line statements. Strip line and nested block comments before extracting targets, preserving the source line where each statement starts. A path such as `config::names::X` targets `config`; a brace list such as `{id::FolioId, store::ObjectStore}` targets the first identifier of each top-level list item. Report the line where the statement starts.
2. Skip an item following a line that is exactly `#[cfg(test)]` after trimming. Further attributes belong to that item. It ends at a `;` before any `{`, or at the `}` closing its first `{`. Strip `//` comments before counting braces. Other forms such as `#[cfg(any(test, feature = "test-support"))]` remain checked because they are not test-only.
3. Skip every file named `tests.rs`, every file under a directory named `tests`, and the whole `test_support` module. The importing module is the first path component below the source root, without a trailing `.rs`: `home.rs` and `home/open.rs` both belong to `home`. Skip `lib.rs`.
4. Allow imports within the importing module’s own unit.
5. Report an importer or target absent from the table as `not in table`.
6. Allow targets listed in the importer’s row or in an allowed-exception bullet. Otherwise report `upward` for a higher target layer, `sideways` for an equal target layer, and `not listed` for a lower target layer omitted from the row.

This is a text heuristic: it reads `use crate::` statements, not inline `crate::` paths, `super::` paths or macros. Comment delimiters or braces inside strings can confuse parsing and test-item skipping. It checks top-level modules, not items inside a module. Table rows start with `| L` followed by a digit; module and import cells contain comma-separated names. The check reads exception bullets only in `## Allowed exceptions`.

## Changing the table

A new top-level module or a moved item changes the table in the same change. The Phase F tickets update the rows for their moved modules.
