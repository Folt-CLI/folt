# Orchestrator rules

## Scope and decisions

- Record technical decisions in [the stack](../stack.md) only after the developer selects them; do not infer one decision from another.
- Use `gh` with `--repo Folt-CLI/folt`.
- Use native parent and dependency relationships. Release work defaults to epic #1 unless a delivery group, Question or developer instruction specifies another parent.
- Cross-repository references use `owner/repo#N` or a full URL. Do not assume issue numbers identify issues in another repository.

## Issues and analysis

- Work items have exactly Summary, Scope, Analysis, Solution and Details. Fill Summary/Scope at creation; follow [Issue format](../issues/issue-format.md) for the remaining sections.
- See [working templates](../issues/README.md) for how a ticket is worked by its size and issue type.
- Implementation agents implement and fix only; they do not analyse code, reviews or CI. The orchestrator investigates. For M and L tickets the developer selects the solution before implementation; XS and S carry their solution. Follow [Testing](../delivery/testing.md) for roles and handoffs.
- Ask the developer questions only on Question tickets, and give ticket tracking to background sub-agents; see [shared steps](../issues/README.md#shared-steps).
- Write Analysis inline in the ticket using the layout in [Issue format](../issues/issue-format.md#analysis-and-solution-selection). Attach findings as `<N>-code-findings.md` and each solution as a complete implementation scope `<N>-solution-<k>.md` using gh-attach. Verify uploads and use real attachment links. Analyses and scopes use `GLOSSARY.md` terms exactly and no emojis.
- Parents wait for closed children; see [closing rules](../issues/README.md#shared-steps).
- Set ticket and PR metadata; see [ticket planning](../issues/ticket-planning.md#at-issue-creation) and [PRs](../delivery/workflow.md#pull-requests).
- Ticket work in its repository; see [repository rules](../issues/README.md#shared-steps).
- Use the release Epic format and closure; see [Epics](../issues/issue-format.md#epics).

## Commits and delivery

- When implementation finishes, follow the size's review rule in [working templates](../issues/README.md). The orchestrator runs the gate in [Testing](../delivery/testing.md), then commits in the issue format, pushes and opens or updates the PR without another request. Only the developer merges; an agent merges only when instructed for that PR as the developer last saw it. If the Git branch changes afterwards, including conflict resolution or new Git commits, the instruction lapses: stop when checks are green and ask again. Never chain cleanup after a merge command that may have failed. Follow [Review](../delivery/review.md) and [Stacked pull requests](../delivery/stacked-prs.md).
- All agents, including implementation and delegated agents, must do implementation work in a dedicated Git worktree for the issue branch. Do not implement in the primary checkout or share one worktree between concurrent issue work. Fetch and fast-forward the base branch before creating the worktree.
- First commit: Initial[#1]: <short explanation>.
- Later commits: <Issue Type>[#<issue-number>]: <short explanation>, copying the exact issue type.
- Dependabot uses `Dependabot[#0]: <Dependabot's description>`, the only subject type that is not an issue type; `#0` needs no ticket, epic or issue. Set it through `commit-message.prefix` in `.github/dependabot.yml`; squash nonconforming PR commits into one with that subject. Always run a read-only review of every Dependabot PR using the diff, CI result and pasted dependency release notes to check workflows and code. Post exactly `No issues.` when clean; otherwise post findings as in [Review](../delivery/review.md).
- Keep commits small and focused; group a multi-part ticket into several commits.
- Every commit body has **Summary** (why the commit exists) and, when it touches more than one thing or the reason is not obvious, **Changes**: bullets of "what changed: why it changed". See [workflow.md](../delivery/workflow.md).
- Start issue branches from `develop`, or from the lower branch of a stack, and target PRs to `develop`, or to the branch below in a stack. Promote releases from `develop` to `main` through a PR. Record requested changes on the ticket. Use the PR title and description format in [workflow.md](../delivery/workflow.md) and record delivery and verification in the PR.

## Repository conventions

- Use the issue type and fields described in [ticket planning](../issues/ticket-planning.md).
- Use the PR template to record changes, owning issues, performed verification and material risks. Do not treat template placeholders as performed checks.

## Working a Question

See [Working a Question](../issues/l-question.md#working-a-question).

## Choosing an agent tier

The orchestrator sets the tier of every agent it starts from the ticket's size and difficulty. State the tier and its reason in the report. Start from the size rules: implementation agents use small for XS and S, medium for M, and large for L. Analysis agents use medium for M and large for L; review agents use at least medium. Raise one tier when the ticket is ambiguous or costly to get wrong. Lower one tier when the scope is exact and an automated check covers it; review remains at least medium. Mechanical tracking (moving Status and fields, uploads, watching runs) uses small. The highest tier is for rare cases and needs a stated reason. Which model serves each tier is set in the developer's own configuration outside the repository; no repository document names a model.
