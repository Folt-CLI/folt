# Orchestrator rules

## Scope and decisions

- See [Rules for every role](../../../AGENTS.md#rules-for-every-role) for recording technical decisions and cross-repository references.
- Use `gh` with `--repo Folt-CLI/folt`.
- See [Dependencies and grouped work](../issues/ticket-planning.md#dependencies-and-grouped-work).

## Issues and analysis

- See [Issue format](../issues/issue-format.md#work-items) for the work-item body sections.
- See [working templates](../issues/rules.md) for how a ticket is worked by its size and issue type.
- Implementation agents implement and fix only; they do not analyse code, reviews or CI. The orchestrator investigates. For M and L tickets the developer selects the solution before implementation; XS and S carry their solution. Follow [Testing](../delivery/testing.md) for roles and handoffs.
- Ask the developer questions only on Question tickets, and give ticket tracking to background sub-agents; see [shared steps](../issues/rules.md#shared-steps).
- Write Analysis inline in the ticket using the layout in [Issue format](../issues/issue-format.md#analysis-and-solution-selection). Attach findings as `<N>-code-findings.md` and each solution as a complete implementation scope `<N>-solution-<k>.md` using gh-attach. Verify uploads and use real attachment links. Analyses and scopes use `GLOSSARY.md` terms exactly and no emojis.
- Parents wait for closed children; see [closing rules](../issues/rules.md#shared-steps).
- Set ticket and PR metadata; see [ticket planning](../issues/ticket-planning.md#at-issue-creation) and [PRs](../delivery/workflow.md#pull-requests).
- Ticket work in its repository; see [repository rules](../issues/rules.md#shared-steps).
- Use the release Epic format and closure; see [Epics](../issues/issue-format.md#epics).

## Commits and delivery

- See step 4 of [Stages](../delivery/workflow.md#stages). Only the developer merges; an agent merges only when instructed for that PR as the developer last saw it. If the Git branch changes afterwards, including conflict resolution or new Git commits, the instruction lapses: stop when checks are green and ask again. Never chain cleanup after a merge command that may have failed. Follow [Review](../delivery/review.md) and [Stacked pull requests](../delivery/stacked-prs.md).
- See step 3 of [Stages](../delivery/workflow.md#stages) and [Branches and commits](../delivery/workflow.md#branches-and-commits) for worktrees.
- First commit: Initial[#1]: <short explanation>.
- See [Branches and commits](../delivery/workflow.md#branches-and-commits) for later commit subjects.
- See [Branches and commits](../delivery/workflow.md#branches-and-commits) for the Dependabot subject. Always run a read-only review of every Dependabot PR using the diff, CI result and pasted dependency release notes to check workflows and code. Post exactly `No issues.` when clean; otherwise post findings as in [Review](../delivery/review.md).
- See [Commit size](../delivery/workflow.md#commit-size).
- See [Commit body](../delivery/workflow.md#commit-body).
- See [Branches and commits](../delivery/workflow.md#branches-and-commits) and [Pull requests](../delivery/workflow.md#pull-requests).

## Repository conventions

- Use the issue type and fields described in [ticket planning](../issues/ticket-planning.md).
- Use the PR template to record changes, owning issues, performed verification and material risks. Do not treat template placeholders as performed checks.

## Working a Question

See [Working a Question](../issues/sizes/l-question.md#working-a-question).

## Choosing an agent tier

The orchestrator sets the tier of every agent it starts from the ticket's size and difficulty. State the tier and its reason in the report. Start from the size rules: implementation agents use small for XS and S, medium for M, and large for L. Analysis agents use medium for M and large for L; review agents use at least medium. Raise one tier when the ticket is ambiguous or costly to get wrong. Lower one tier when the scope is exact and an automated check covers it; review remains at least medium. Mechanical tracking (moving Status and fields, uploads, watching runs) uses small. The highest tier is for rare cases and needs a stated reason. Which model serves each tier is set in the developer's own configuration outside the repository; no repository document names a model.
