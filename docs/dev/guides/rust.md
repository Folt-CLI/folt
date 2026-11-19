# Rust guidelines

Guidance for agents writing or reviewing Rust in this repository. Each rule has an id so a finding can cite it. Project instructions, role files, the lints in `Cargo.toml` and `clippy.toml`, and existing conventions win over this file where they differ.

The workspace lints already forbid `unsafe_code` and deny `unwrap_used`, `expect_used`, `panic` and `unreachable`; `clippy.toml` bans direct environment access. The commands that run before a push are in [Testing](../delivery/testing.md#gate-before-push). This file does not repeat them. See [Standards](standards.md) for the language-independent rules.

## R1 Working method

- **R1.1** Read the crate's `Cargo.toml`, the nearby code and tests, and the ticket's implementation scope before editing. Find the module that owns the behaviour.
- **R1.2** Decide ownership, invariants, error boundaries and public API impact before writing.
- **R1.3** Make the smallest coherent change. Reuse existing helpers and test conventions. Add no crate, trait, layer, macro or dependency without a reason in the ticket.
- **R1.4** Fix the cause of a compiler or Clippy finding. Do not silence it. A lint exception follows [Testing](../delivery/testing.md#rule).
- **R1.5** Read the final diff for accidental API changes, needless clones, unrelated edits and untested behaviour.

## R2 Idiomatic design

- **R2.1** Design around structs, enums, traits and pattern matching. Do not recreate class hierarchies, dependency-injection containers or interface layers.
- **R2.2** Use an inherent `impl` for a type's normal behaviour. Add a trait only when several types share a contract, generics need a bound, or an external type must be extended.
- **R2.3** For a closed set of variants use `enum` and `match` before a trait object. Use `dyn Trait` only for runtime polymorphism that an enum cannot express.
- **R2.4** Keep abstractions proportional to real variation. Typestate, builders and macros earn their place by removing invalid states or real repetition.

## R3 Ownership and allocation

- **R3.1** Decide who owns each value before choosing a signature. Take `&str`, `&[T]` and `&Path` when ownership is not needed; take owned values when the callee keeps them.
- **R3.2** Clone on purpose. A frequent or expensive clone means the ownership boundary is wrong.
- **R3.3** Avoid needless allocation in hot paths, but prefer clear code where performance is not material.
- **R3.4** Add lifetime annotations only where needed. Prefer simpler ownership to lifetime plumbing.
- **R3.5** Use shared ownership and interior mutability only when the design needs them, not to satisfy the borrow checker.

## R4 Types and API

- **R4.1** Use `Option` for absence, `Result` for recoverable failure and enums for distinct states. Avoid magic strings and ambiguous booleans.
- **R4.2** Use a newtype for a distinct domain concept or an important invariant, not for every primitive.
- **R4.3** Make invalid states hard to construct. Prefer a plain runtime check to elaborate type machinery when it is clearer.
- **R4.4** Keep the public surface small: private fields and `pub(crate)` by default. A new export is a decision.
- **R4.5** Follow the Rust API Guidelines for naming and conversions (`as_`, `to_`, `into_`). Implement standard traits only where their meaning applies.
- **R4.6** There is no release yet: keep one public path per item and add no compatibility layers, aliases or deprecated shims.

## R5 Errors

- **R5.1** Use `Result` and `?` for expected failure. Keep the context and the source error.
- **R5.2** Library boundaries return typed errors; the CLI may use context-rich report errors.
- **R5.3** Do not discard an error with `let _ =`, `.ok()` or an empty handler unless ignoring it is the intent, and say so.
- **R5.4** Do not turn a failure into a default or success value. Document error behaviour on public items.

## R6 Concurrency and resources

- **R6.1** Add `async` only where asynchronous I/O is needed. Do not make a call chain async by default.
- **R6.2** Keep locks short. Never hold a synchronous guard across `.await`. Prefer ownership or message passing to shared `Arc<Mutex<_>>` state.
- **R6.3** Release resources through RAII. Where dropping cannot guarantee success, make cleanup explicit.

## R7 Tests and documentation

- **R7.1** Test observable contracts, invariants, edge cases, error paths and regressions. A test that repeats an implementation constant or branch proves nothing. The test rule and layout are in [Testing](../delivery/testing.md).
- **R7.2** Tests are deterministic and need no external service.
- **R7.3** Document public items with rustdoc, including `# Errors` and `# Panics` where they apply. Comments explain reasons and constraints, not syntax.

## R8 Security and dependencies

- **R8.1** Validate untrusted input at file, process and serialization boundaries. Keep secrets out of errors, logs and debug output.
- **R8.2** Add a dependency only when the ticket's selected solution names it. Do not run unrelated upgrades.

## R9 Cargo and toolchain

- **R9.1** Respect the edition, toolchain pin, targets and feature policy. Do not change them incidentally.
- **R9.2** Keep Cargo features additive. Add no proc macro, build script or lint configuration without a reason in the ticket.

## R10 Before you finish

- Is it idiomatic Rust, or a pattern imported from another language (R2)?
- Is every clone, pointer wrapper and lifetime deliberate (R3)?
- Could an enum, function or method replace a new trait or layer (R2.3, R2.4)?
- Are failures explicit and free of discarded errors (R5)?
- Are API, dependency and compatibility changes justified (R4, R8)?
- Do the tests verify behaviour, including errors and regressions (R7.1)?

Use the compiler to make the code reliable, and never make the design harder to read to show off Rust.
