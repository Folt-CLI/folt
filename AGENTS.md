# Folt repository instructions

This file is read by every agent; find your role below.

## Find your role

| Role | File | How to know it |
| --- | --- | --- |
| analysis agent | [analyst.md](docs/dev/agents/analyst.md) | The prompt says analyse. |
| implementation agent | [implementer.md](docs/dev/agents/implementer.md) | The prompt says implement. |
| review agent | [reviewer.md](docs/dev/agents/reviewer.md) | The prompt says review. |
| orchestrator | [orchestrator.md](docs/dev/agents/orchestrator.md) | The prompt strictly says you are the Orchestrator. |

Read the agent role files only when the prompt strictly matches your role.

Any other prompt, including an ordinary chat, does not read it.

## Rules for every role

- Read [GLOSSARY.md](GLOSSARY.md) and use its terms exactly as defined, never use a defined word in another sense.
- Add a new term in the same change that introduces it.
- Record technical decisions in [the stack](docs/dev/stack.md) only after the developer selects them; do not infer one decision from another.
- Cross-repository references use `owner/repo#N` or a full URL. Do not assume issue numbers identify issues in another repository.
- Follow .editorconfig.
- Keep build outputs, local configuration and credentials out of Git.
- Use no emojis.
- Keep each paragraph and list item on one line in Markdown; do not hard-wrap prose at a column.
- Never name an agent tool, model or provider in an issue, comment, pull request or Git commit message.
- Code must not add new line-level YAML parsing of front matter; new front matter behaviour must extend the front matter tree (`Block` and `Node` in `crates/folt-core/src/front_matter/tree.rs`) instead.
- Do only what your role file allows.

## Where things are

- Code: `crates/folt-core` (library), `crates/folt-cli` (the `folt` binary).
- Tests: `tests/`.
- Process: [docs/dev/README.md](docs/dev/README.md) and the rest of `docs/dev/`.
- Agent roles: `docs/dev/agents/`.
- Terms: [GLOSSARY.md](GLOSSARY.md).
- Issue forms: `.github/ISSUE_TEMPLATE/`. CI: `.github/workflows/`.
- Scripts: `scripts/`.
- Build settings: `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `.cargo/`.
