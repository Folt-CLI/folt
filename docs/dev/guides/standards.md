# Engineering standards

Language-independent rules for agents writing or reviewing code in this repository. Each rule has an id so a finding can cite it. [Rust guidelines](rust.md) govern Rust design and win where they differ; Folt is a Rust project, so do not add traits, layers or patterns to satisfy a named principle such as SOLID. Role files and the project instructions win over this file.

## S1 Priorities

When several valid designs exist, decide in this order:

1. **S1.1** Reject any design that breaks correctness, security or an explicit requirement.
2. **S1.2** Prefer the design that is easiest to understand and maintain.
3. **S1.3** Prefer consistency with the existing architecture and conventions.
4. **S1.4** Minimise dependencies and complexity.
5. **S1.5** Add extension points only for a present requirement.
6. **S1.6** State a significant trade-off when it affects the decision.

## S2 Avoid overengineering

- **S2.1** Solve the requested problem only. No speculative features, options, flags or compatibility layers (YAGNI).
- **S2.2** Choose the simplest design that meets the requirement (KISS). Direct, readable code beats clever code.
- **S2.3** Do not extract shared code because it looks similar. Abstract when the responsibility and the variation are understood; a little duplication is cheaper than the wrong abstraction (AHA).
- **S2.4** Every new layer, helper, interface or dependency has a present benefit that can be named.
- **S2.5** Keep APIs and configuration minimal. No generic machinery for one known use.
- **S2.6** Tests, validation, error handling, security and required performance are not optional complexity.
- **S2.7** Remove dead code and unused scaffolding when it is in scope.

## S3 Design

- **S3.1** Understand the existing architecture and module boundaries before changing structure, and keep the dependency direction.
- **S3.2** Keep unrelated responsibilities apart and related code together. Avoid hidden global state and circular dependencies.
- **S3.3** State important inputs, outputs, invariants and failure conditions. Validate input where it crosses a trust boundary.
- **S3.4** Refactor to improve structure while preserving behaviour, and keep a refactor separate from an unrelated functional change.
- **S3.5** Do not optimise before measuring a real concern.

## S4 Honest verification

- **S4.1** Test behaviour that others can observe, not implementation details. Cover normal cases, edges, failures and regressions in proportion to the risk.
- **S4.2** Never report a check as passed unless it ran and passed. Report what changed, what was run and what could not be run.
- **S4.3** Preserve existing behaviour unless the ticket requires a change.

## S5 Before you finish

- Does every new component or abstraction solve a need the ticket states?
- Could a simpler change meet the same requirement?
- Did the change add functionality or a dependency nobody asked for?
- Is the result consistent with the existing architecture and easy to change?

If the added complexity cannot be explained by a current requirement, a concrete risk or a measured benefit, leave it out.
