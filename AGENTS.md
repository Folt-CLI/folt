# Folt repository instructions

This file is read by every agent; find your role below.

## Find your role

| Role | File | How to know it |
| --- | --- | --- |
| analysis agent | [analyst.md](docs/dev/agents/analyst.md) | The prompt says analyse. |
| implementation agent | [implementer.md](docs/dev/agents/implementer.md) | The prompt says implement. |
| review agent | [reviewer.md](docs/dev/agents/reviewer.md) | The prompt says review. |
| orchestrator | [orchestrator.md](docs/dev/orchestrator.md) | The prompt says merge; when unsure, treat yourself as the orchestrator. |

## Rules for every role

- Read [GLOSSARY.md](GLOSSARY.md) and use its terms exactly as defined;
  never use a defined word in another sense.
- Add a new term in the same change that introduces it.
- Record technical decisions in [the stack](docs/dev/stack.md) only after the
  developer selects them; do not infer one decision from another.
- Cross-repository references use `owner/repo#N` or a full URL. Do not assume
  issue numbers identify issues in another repository.
- Follow .editorconfig.
- Keep build outputs, local configuration and credentials out of Git.
- Use no emojis.
- Never name an agent tool, model or provider in an issue, comment, pull request
  or Git commit message.
- Do only what your role file allows.

Process overview: [docs/dev/process.md](docs/dev/process.md).
