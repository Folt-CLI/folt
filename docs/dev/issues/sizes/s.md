# S ticket

Use S for straightforward work whose decision is already made. The ticket asks no questions.

## The ticket must contain

Use the five-section [work-item body](../issue-format.md) with Summary and Scope filled at creation. Set the milestone, Priority, Size, Status and ownership from the [shared steps](../rules.md#shared-steps).

## Analysis and review

S runs [scoping](../rules.md#analysis-depths) and needs no review. The gate and CI are its checks.

## Outcome

Write the outcome in Summary and two to five numbered scenarios in Scope. Add **Agreed in #N** with the decisions the parent Question made, as constraints the work must respect, and rough file hints that do not bind. S has no Solution section and no scope attachment.

When the run starts, scoping reads the ticket, the parent Question's findings and the current code, and returns the allowed files (with their test files), the scenarios and the verification commands. The implementation agent receives that result. The pull request lists the allowed files under Changes. If scoping finds the ticket does not fit the code or leaves a design question open, re-size to M; see [Re-sizing](../rules.md#analysis-triggers-and-re-sizing).

## Statuses

The ticket is ready when its outcome, scenarios and constraints exist. Scoping runs at the start of S4. See the Status values in the [shared steps](../rules.md#shared-steps) for the suggested path.

## Who does what

See [Roles](../../delivery/testing.md#roles) and [Pull requests](../../delivery/workflow.md#pull-requests).

## Components and parents

Set exactly one Component issue field. Use the [shared parent and repository rules](../rules.md#shared-steps).

## Closing

Record Disposition and PR verification. See the [closing rules](../rules.md#shared-steps) for parents.
