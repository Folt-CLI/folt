# Modules of folt-core

This guide maps the modules of `crates/folt-core/src` as they are now and names the one owner of each rule. The import permissions between top-level modules are in [Layers](layers.md); terms are in [GLOSSARY.md](../../../GLOSSARY.md). A change that moves a module or an owner updates the matching row here.

## Module map

The modules are listed in reading order, from the modules with no imports to the modules that use the rest.

| Module | Purpose |
| --- | --- |
| `config` | Fixed names, limits and format facts, `Settings`, `FileSettings` and the `Environment` reader. |
| `layout` | The home format version and layout support. |
| `id` | Folio ids and object ids. |
| `fsx` | File-system writes, the `FileSystem` trait, `TempFile` and the file signature. |
| `merge` | Pure three-way merge of bytes, with binary conflicts and recorded conflict marker lines. |
| `ref_name` | Validated names of history refs and retention pin names. |
| `front_matter` | Locating, reading and editing front matter through the syntax tree. |
| `object` | Immutable history objects: blobs, commits and tags. |
| `home` | The home: locating, opening, recognising and the home lock. |
| `folio` | The Folio directory, folio-name, Folio format, registry row and refs directory. |
| `store` | The object store of one Folio. |
| `placement_path` | The resolved placement path. |
| `placement` | Placement kinds, observations, drift and the in-memory placement records. Still a flat file. |
| `placement_fs` | File-system access for placements. Still a flat file. |
| `history` | The ref store, commit ancestry and merge bases of one Folio. Still a flat file with submodules; the history operations live in `ops/history`. |
| `registry` | The SQLite registry: schema, rows, lookups and transactions. |
| `ops` | The operations, which gather facts, judge them and apply the plan. |
| `test_support` | Shared test fixtures, enabled by the `test-support` feature and by `cfg(test)`. |

## Order inside ops

The order is `refusal`, `facts`, `working`, `status`, `step_hook`, `stage`, `placement`, `history`, `register`, `rename`, `checkout`, `prune`, then `facade`, as stated in `crates/folt-core/src/ops/mod.rs`. Each module imports only earlier modules or `rollback`, and `rollback` imports only `fsx`. `placement` imports `stage::refuse`, which is why `stage` comes first. `facade` only adds `Home` methods that forward to the owning functions.

| Module | Purpose |
| --- | --- |
| `ops::refusal` | Placement refusals. |
| `ops::facts` | Facts of a placement and the comparison with the branch tip. |
| `ops::working` | Gathering and resolving working files. |
| `ops::status` | Judging working status. |
| `ops::step_hook` | The `StepHook` trait that observes operation steps. |
| `ops::rollback` | The `Rollback` guard. |
| `ops::rules_error` | `RulesError`, the error shared by the operations. |
| `ops::open` | Opening or creating a home, as a `Home` method. |
| `ops::stage` | Stage and unstage, with the stage refusal checks in `ops::stage::refuse`. |
| `ops::placement` | Placement convert, move, rehome, relocate, remove and repair. |
| `ops::history` | History operations: commit, merge, branch, tag, reset and rebase. |
| `ops::register` | Registration. |
| `ops::rename` | Rename. |
| `ops::checkout` | Checkout. |
| `ops::prune` | Prune. |

## Where a rule lives

Each rule has one owner. Extend the owner; do not add a second one.

| Rule | Owner |
| --- | --- |
| Fixed names, file names and the marker name | `config::names` |
| Scan limit and the window that reads past it | `config::limits` (`SCAN_LIMIT`, `SCAN_WINDOW`) |
| Registry application id | `config::formats` (`APPLICATION_ID`) |
| Reading the process environment, the OS home directory and the current directory | `config::env` (`Environment`) |
| Typed runtime settings and the values from `config.toml` | `config::settings` (`Settings`, `FileSettings`) |
| Front matter structure and every edit of it | `front_matter::tree` (`Block`, `Node`) |
| Where front matter starts and ends | `front_matter::locate` |
| File-system writes and temporary files | `fsx` (`FileSystem`, `TempFile`) |
| Observing an operation step | `ops::step_hook` (`StepHook`) |
| Restoring files and removing created files when an operation fails | `ops::rollback` (`Rollback`) |
| Placement refusals | `ops::refusal` |
| Gathered facts of a placement | `ops::facts` |
| Whether a placement has a stage | `ops::stage::refuse` |
| Binary test, whole-file equality rules, line merge and conflict marker rendering | `merge` |
| History operations and their refusals | `ops::history` |
| Merge entry, two-parent merge commit and merge refusals | `ops::history` |
| Validation of merged content: reference keys, reference cycles and folio-name collisions | `ops::stage::validate` |
| Registration plan, apply and refusals | `ops::register` |
| Rename plan and apply | `ops::rename` |
| Prune judge and apply | `ops::prune` |
| Exclusive access to one home | `home::lock` (`HomeLock`) |
| Resolving a path to a placement path | `placement_path` (`PlacementPath`) |
| Registry schema, rows and transactions | `registry` |
| Branch, tag and pin refs on disk | `history::RefStore` |
| Commit ancestry and merge base computation | `history::ancestry` |

## Forbidden patterns

These are the same rules as in [AGENTS.md](../../../AGENTS.md).

- A mutating `std::fs` call outside `fsx`: `fsx` owns file-system writes, so atomicity, syncing and fault injection stay in one place.
- `unwrap`, `expect` and `unreachable` in non-test code: the clippy lints `unwrap_used`, `expect_used` and `unreachable` are set to deny in `Cargo.toml`, and a failure must reach the caller as an error.
- Reading the process environment, the OS home directory or the current directory outside `config::env::Environment`: tests and callers must be able to supply those values.
- Repeating a name, limit or format literal that `config` owns: a repeated literal drifts from its owner.
- A second owner for a rule listed in the table above: two owners disagree over time.
- An import that violates the layer table: an upward import can silently reintroduce a cycle.
- A `pub use` shim or alias kept for an old path: there is no release to stay compatible with, and a shim hides the true owner.
