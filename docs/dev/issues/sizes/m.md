# M ticket

Use M when a ticket is still loose and can be read several ways. It needs analysis and review, and the developer selects a solution. If there is one straightforward solution, use S or XS, not M.

Split it into M/S items when warranted.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

For a Question, follow [Working a Question](l-question.md#working-a-question).

M needs analysis AND review. Analysis agents are read-only and produce drafts as files: findings, solution scopes and Analysis text. The orchestrator publishes them to the ticket and attaches them.

The orchestrator writes Analysis inline, links the findings attachment from Details and attaches each solution as a complete implementation scope.

Follow [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection) for the layouts, uploads and selection. Use a read-only [review](../../delivery/review.md).

## Solution

The developer selects the solution before implementation. Mark its implementation scope attachment **[Selected Solution]**. Summarize changes, Affected areas and Why this one in Solution; link findings and two to four key points in Details.

## Statuses

See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path.

S3 requires a recorded selected solution: mark the attachment **[Selected Solution]** and fill Solution. Do not start the implementation agent or move the ticket to S3 or S4 before this is recorded.

## Who does what

See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests). Apply the [type row](../rules.md#by-issue-type).

## Components and parents

Set up to two Component issue fields, for example Source Code and Tests for unit tests. Use the [shared parent and repository rules](../rules.md#shared-steps).

## Closing

Record Disposition and PR verification. See the [closing rules](../rules.md#shared-steps) for parents.
