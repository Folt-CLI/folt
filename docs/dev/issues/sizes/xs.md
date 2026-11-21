# XS ticket

Use XS for the smallest work item; the ticket is self-descriptive.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

XS needs no analysis and no review.

## Outcome

Write the outcome in Summary and two or three numbered scenarios in Scope, with **Agreed in #N** when a parent Question decided something the work must respect. XS has no Solution section and no scope attachment; the implementation agent reads the code itself, within the ticket's Scope.

If implementation needs more than the ticket says, stop and re-size to S. See [Re-sizing](../rules.md#analysis-triggers-and-re-sizing).

## Statuses

The ticket is ready when its outcome and scenarios exist. See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path.

## Who does what

See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests).

## Components and parents

Set exactly one Component issue field. Use the [shared parent and repository rules](../rules.md#shared-steps).

## Closing

Record Disposition and PR verification. See the [closing rules](../rules.md#shared-steps) for parents.
