# L Question

An L is always a Question. Split it into M/S/XS child tickets under it.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set type Question and the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

For a Question, follow [Working a Question](#working-a-question).

L needs analysis and review. The orchestrator writes Analysis inline in the ticket, links the findings attachment from Details and attaches each solution as a complete implementation scope. Follow [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection) for the layouts, uploads and selection. Use a read-only [review](../../delivery/review.md).

## Solution

The developer selects the answer. Mark its implementation scope attachment **[Selected Solution]** and summarize changes, Affected areas and Why this one. Link findings and two to four key points in Details. Derive the child tickets from the answer.

## Statuses

Suggested path: `S0: To Do` -> `S1: In Analysis` -> `S3: Ready For Implementation` -> `S4: In Implementation` -> `S5: Done`. Project workflows set Done on close and on merge. An answered Question may be Done with Answered while its children remain open.

## Who does what

The orchestrator analyses; the developer selects the answer. An implementation agent implements and fixes in dedicated child worktrees and does not analyse. The orchestrator runs the [gate](../../delivery/testing.md#gate-before-push), commits, pushes and opens PRs; only the developer merges. Give every PR a milestone, an assignee and the project.

## Components and parents

L may name several Components describing what can be done; represent each one by its own child ticket. Each child keeps its size's Component limit. Use native parents and the [shared repository rules](../rules.md#shared-steps).

## Closing

Close with `Disposition: Answered` only after all children are closed. It may be `S5: Done` with its Disposition before then; do not rely on "Auto-close issue" for parents.

## Working a Question

The orchestrator works every Question itself; Fugue's `fugue question` command is not used.

1. Start with a read-only interview probing intent, users, boundaries, failure cases, constraints and code touched. Offer 3 to 5 rough directions, then obtain the developer's confirmation before any analysis.
2. Write Analysis using [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection). Attach the finished interview as `<N>-conversation.md` and link it from Details. Use the [Implementation scope layout](../issue-format.md#implementation-scope-layout) for each option's scope.
3. The developer selects an option and gives a reason; record the selection and reason once in Solution. Mark the chosen scope **[Selected Solution]**.
4. Propose child tickets as a table: key, title, issue type, size, priority, components, blocked-by. Create them only after the developer answers `yes`, with native parent and blocked-by links. On a rerun, reuse existing child tickets and relationships; do not create duplicates.
