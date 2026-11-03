# Stack

## Confirmed

- **Language:** Rust.
- **Repository:** this repository. `folt` is a standalone tool and does not depend
  on Foltivo; Foltivo is one host that can act as a remote.
- **Role:** manage Folios locally and exchange them with remotes, replacing Git
  synchronization for Folio content.
- **Crate structure:** a virtual Cargo workspace with `folt-core` (library) and
  `folt-cli` (the `folt` binary), selected in
  [#7](https://github.com/Folt-CLI/folt/issues/7).
- **Storage:** an own per-Folio store of immutable whole-file objects named by
  SHA-256, with no dependency on Git
  ([#3](https://github.com/Folt-CLI/folt/issues/3)).
- **Sync protocol:** a minimal HTTPS request/response API with a stateless host
  that stores objects by hash, verifies them and moves refs by compare-and-swap;
  capabilities are negotiated ([#4](https://github.com/Folt-CLI/folt/issues/4)).
- **Home:** one global home, `~/.folt` (`%USERPROFILE%\.folt` on Windows),
  with a `FOLT_HOME` environment override; Folt writes nothing into user
  directories ([#14](https://github.com/Folt-CLI/folt/issues/14)).
  The home holds a one-line integer `format` file, written by temp file
  and rename. A missing format file in a non-empty directory, an unparseable
  one or a newer version is refused
  ([#18](https://github.com/Folt-CLI/folt/issues/18)). This replaces the
  per-workspace `.folt/` marker of
  [#2](https://github.com/Folt-CLI/folt/issues/2).
- **Registry:** an embedded SQLite database in the home through `rusqlite` with
  the bundled C library; a Folio exists independently of its placements.
  Building needs a C compiler ([#27](https://github.com/Folt-CLI/folt/issues/27)).
- **Lock:** an exclusive file lock on a permanent `lock` file in the home;
  temporary files live in `<home>/tmp/`; leftovers are deleted when the lock is
  acquired ([#19](https://github.com/Folt-CLI/folt/issues/19)).
- **Tests:** the test structure and tiers are described in [Testing](testing.md)
  ([#96](https://github.com/Folt-CLI/folt/issues/96)).

Terms are defined in [GLOSSARY.md](../../GLOSSARY.md).

The home and every Folio directory are trusted: whoever can change them can
change everything Folt writes. Folt does not defend against a process running as
the same user. On Unix, the home and directories Folt creates within it are
created private (`0700`); files created directly by the home and Folio directory
code use `0600`. Opening a home writable by group or others is refused; opening
one readable by them returns a warning. On other platforms the trust boundary
is documentation only and permission behavior is unchanged. This is the selected
solution for [#82](https://github.com/Folt-CLI/folt/issues/82).

## Pending decisions

- How remotes are configured and authenticated; each host defines its own
  authentication ([#5](https://github.com/Folt-CLI/folt/issues/5)).
- Command set ([#6](https://github.com/Folt-CLI/folt/issues/6)).
- Merge and rebase ([#74](https://github.com/Folt-CLI/folt/issues/74)).
- The lifecycle of registered Folios
  ([#15](https://github.com/Folt-CLI/folt/issues/15)).
- One Folio at several paths ([#16](https://github.com/Folt-CLI/folt/issues/16)).
- Portability and multi-user use of the home
  ([#17](https://github.com/Folt-CLI/folt/issues/17)).
- Local path resolution for references
  ([#28](https://github.com/Folt-CLI/folt/issues/28)).

## Delivery status

The `folt` binary still only prints its version; `folt-core` has the library
capabilities listed under "What exists today" in [README.md](../../README.md);
terms are in [GLOSSARY.md](../../GLOSSARY.md).
