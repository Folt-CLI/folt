# L Question

An L is always a Question. Split it into M/S/XS child tickets under it. An L carries no solution files: its children carry the work.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set type Question and the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

For a Question, follow [Working a Question](#working-a-question).

L is analysed with the developer in an interview and has no review: it changes no code, and its children are reviewed by their own size. The orchestrator writes Analysis inline in the ticket and links one findings attachment from Details; the findings are the shared context of the children. Options stay inline. Follow [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection) for the layouts, uploads and selection.

## Solution

The developer selects the answer. Record the selection and its reason once in Solution, summarize changes, Affected areas and Why this one, and link findings and two to four key points in Details. State each decision as a constraint a child can quote under **Agreed in #N**. Derive the child tickets from the answer.

## Statuses

See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path. An answered Question may be Done with Answered while its children remain open.

## Who does what

The orchestrator analyses; the developer selects the answer. See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests).

## Components and parents

L may name several Components describing what can be done; represent each one by its own child ticket. Each child keeps its size's Component limit. Use native parents and the [shared repository rules](../rules.md#shared-steps).

## Closing

Close with `Disposition: Answered` only after all children are closed. See the [closing rules](../rules.md#shared-steps) for parents.

## M Question

An M Question is a Question that needs no interview. It may sit under an XL when it is a small question or feature, or under an L when it covers something the L did not, or a small question the L left open on purpose to answer during its development.

1. The analysis runs on its own: findings, options when there is a real trade-off, a recommendation and the [trigger](../rules.md#analysis-triggers-and-re-sizing) check.
2. When no trigger fires, the recommendation is selected by policy and recorded with its reason. When one fires, the Question parks for the developer, and an architectural trigger re-sizes it to L.
3. The children are created without a further request, as in step 4 of [Working a Question](#working-a-question) but without waiting for `yes`. Record in the Question what was created.

An M Question has no review and no implementation of its own.

## Working a Question

The orchestrator works every L Question itself; Fugue's `fugue question` command is not used.

1. Start with a read-only interview probing intent, users, boundaries, failure cases, constraints and code touched. Offer 3 to 5 rough directions, then obtain the developer's confirmation before any analysis.
2. Write Analysis using [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection). Attach the finished interview as `<N>-conversation.md` and link it from Details.
3. The developer selects an option and gives a reason; record the selection and reason once in Solution.
4. Propose child tickets as a table: key, title, issue type, size, priority, components, blocked-by, with each child's outcome in one line. Create them only after the developer answers `yes`, with native parent and blocked-by links. Each child's body holds its outcome, numbered scenarios and **Agreed in #N** constraints; it carries no scope attachment. On a rerun, reuse existing child tickets and relationships; do not create duplicates.
