# M ticket

Use M when a ticket is still loose and can be read several ways. It needs full analysis and review. The developer is asked only when a [trigger](../rules.md#analysis-triggers-and-re-sizing) fires. If the decision is already made, use S or XS, not M.

An M may be a Question; see [M Question](l-question.md#m-question). An M sits under an L or an XL. Split it into M/S items when warranted.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

The analysis agent is read-only and produces drafts as files: findings, the options with their trade-offs, a recommendation with the trigger check, and the scope of the selected option. The orchestrator publishes them to the ticket.

The orchestrator writes Analysis inline, links the findings attachment from Details and attaches the selected option's implementation scope. Options stay inline and have no files. Follow [Analysis and solution selection](../issue-format.md#analysis-and-solution-selection) for the layouts, uploads and selection. Use a read-only [review](../../delivery/review.md) of the pull request.

## Solution

When no trigger fires, the recommendation is selected by policy and recorded as such with its reason. When one fires, the ticket parks and the developer selects. Mark the scope attachment **[Selected Solution]**. Summarize changes, Affected areas and Why this one in Solution; link findings and two to four key points in Details.

## Statuses

See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path.

S3 requires a recorded selection: mark the attachment **[Selected Solution]** and fill Solution. Do not start the implementation agent or move the ticket to S3 or S4 before this is recorded.

## Who does what

See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests). Apply the [type row](../rules.md#by-issue-type).

## Components and parents

Set up to two Component issue fields, for example Source Code and Tests for unit tests. Use the [shared parent and repository rules](../rules.md#shared-steps).

## Closing

Record Disposition and PR verification. See the [closing rules](../rules.md#shared-steps) for parents.
