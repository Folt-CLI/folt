# XS ticket

Use XS for the smallest work item; the ticket is self-descriptive.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

XS needs no analysis and no review.

## Solution

Write Solution at creation, short and readable for the developer, in the style of Analysis. It comes from the enabling Question, with no solution selection; the Question's answer does not replace a missing Solution or scope.

Use a `> [!IMPORTANT]` decision line, **Agreed in #N** (what the parent Question decided), **You get**, **Changes**, **Not changed** and **Watch out**, then **Implementation scope** linking the complete `<N>-solution-1.md` scope attachment.

Attach with gh-attach and verify at creation by downloading and comparing.

XS may skip the scope file when the ticket is self-descriptive. Follow the [S and XS Solution layout](../issue-format.md#s-and-xs-solution-layout) and the applicable [type row](../rules.md#by-issue-type).

## Statuses

The ticket is ready when its Solution and scope exist at creation. Only self-descriptive XS may skip the scope. Do not start the implementation agent or move the ticket to S3 or S4 before these requirements are met.

See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path.

## Who does what

See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests).

## Components and parents

Set exactly one Component issue field. Use the [shared parent and repository rules](../rules.md#shared-steps).

## Closing

Record Disposition and PR verification. See the [closing rules](../rules.md#shared-steps) for parents.
