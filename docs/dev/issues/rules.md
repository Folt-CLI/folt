# Working templates

Use these templates to work a ticket by size and issue type. They record the rules decided in [Question #188](https://github.com/Folt-CLI/folt/issues/188).

Reading order: size file first, then the type row.

| Size | Analysis | Review | Components | Typically |
| --- | --- | --- | --- | --- |
| [XS](sizes/xs.md) | None | No | Exactly one | Smallest, self-descriptive work item |
| [S](sizes/s.md) | Scoping | No | Exactly one | Straightforward ticket whose decision is already made |
| [M](sizes/m.md) | Full | Yes | Up to two | Work that may leave several solutions open, or a small Question; sits under an L or an XL |
| [L](sizes/l-question.md) | Interactive | No | Several; each represented by its own child | Always a Question, split into M/S/XS children |
| [XL](sizes/xl-epic.md) | None | No | Not specified; see Open points | Always an Epic; a release that only groups work |

Use M when a ticket is still loose and can be read several ways: it needs full analysis, and the developer is asked only when a [trigger](#analysis-triggers-and-re-sizing) fires. If the decision is already made and only the files and scenarios remain open, use S or XS, not M.

An M Question is a Question that does not need an interview: its analysis runs on its own, its answer is recorded, and its children are created without a further request. It may sit under an XL when it is a small question or feature, or under an L when it covers something the L did not, or a small question the L left open on purpose to answer during its development. A Question that hits a trigger parks, and an architectural trigger re-sizes it to L.

## Shared steps

- Ticket work in the repository where it happens. Reference other repositories by full name. See [Dependencies and grouped work](ticket-planning.md#dependencies-and-grouped-work) for parents and prerequisites.
- Give every ticket a milestone (the milestone of the release it is for, currently `0.1.0`; otherwise `Backlog`), a Priority (`Urgent`, `High`, `Medium`, `Low`), a Size and a Status. Follow [ticket planning](ticket-planning.md) for assignment and project membership. See [Pull requests](../delivery/workflow.md#pull-requests) for pull request metadata.
- Use issue types: Task, Bug, Feature, Question, Epic, Maintenance, Documentation, Incident, Test. Issue types replace Type labels; Component and Disposition are issue fields replacing those labels. Set Components after the solution is selected, or after scoping for S and XS.
- Follow [Issue format](issue-format.md#work-items) for work-item bodies. Follow the size file for analysis and solution readiness, and the Incident and Epic rows below for their report and Objective formats.
- For M and Questions, follow [Analysis and solution selection](issue-format.md#analysis-and-solution-selection) for the inline Analysis, the findings attachment, the selected scope, the layouts, uploads and selection. XS and S carry no Solution section and no scope attachment: the ticket body holds the outcome, numbered scenarios and the constraints the parent Question decided, and S derives its scope by [scoping](#analysis-depths) when its run starts. XL has no analysis. Options stay inline in Analysis; only the selected option's implementation scope is written and attached. Attach with gh-attach and verify by downloading and comparing; see [Markdown attachments](../delivery/github-cli.md#markdown-attachments).
- Use these Status values: `S0: To Do`, `S1: In Analysis`, `S3: Ready For Implementation`, `S4: In Implementation`, `S5: Done`. XS and S are ready when the body has its outcome, numbered scenarios and the constraints from the parent Question; their suggested path is S0 -> S4 -> S5, and S scoping runs at the start of S4. M and Questions need a recorded selection before S3, whether made by the developer or by [policy](#analysis-triggers-and-re-sizing): mark the attachment **[Selected Solution]** and fill Solution. Their suggested path is S0 -> S1 -> S3 -> S4 -> S5. Do not start the implementation agent or move a ticket to S3 or S4 before these requirements are met. Project workflows set Done on close and on merge.
- See [Roles](../delivery/testing.md#roles) and [Pull requests](../delivery/workflow.md#pull-requests). Follow [Testing](../delivery/testing.md) for roles, tests and the gate, and [Review](../delivery/review.md) for read-only review of M.
- Ask the developer only on Question tickets and when a [trigger](#analysis-triggers-and-re-sizing) parks a ticket. Otherwise use the ticket and the analysis recommendation, state assumptions in the result and continue. Hard-to-reverse or outward-facing actions need confirmation unless already permitted; creating the children of an M Question is permitted.
- Background sub-agents unblock the orchestrator: move ticket Status, project fields, links, Disposition; watch implementation agents or CI to completion and report. No code/review/CI analysis or decisions. Analysis agents are read-only and produce drafts as files: findings, options, a recommendation and the selected scope. The orchestrator publishes them to the ticket and attaches them. See step 4 of [Stages](../delivery/workflow.md#stages).
- Follow [Workflow](../delivery/workflow.md#branches-and-commits) for commit subjects, commit bodies and PR delivery, and [Stacked pull requests](../delivery/stacked-prs.md) for dependent branches and PRs.
- Record Disposition before closing. Use `Merged` only for work delivered by a merged pull request; for any other closure, use the matching Disposition and record the reason before setting `S5: Done`. A superseded child ticket keeps its parent and closes with `Disposition: Superseded`. Refine the Size when analysis or scoping clarifies the scope; the new Size sets the review and Component limits. See [Re-sizing](#analysis-triggers-and-re-sizing). A parent cannot close until all its children are closed. It may reach `S5: Done` with its Disposition beforehand, such as an answered Question with open children. Do not rely on the project workflow "Auto-close issue" for parents. Record delivery and verification in the PR.

## Analysis depths

- **Scoping** (S): decides nothing. It reads the ticket, the parent Question's findings and the current code, and returns the allowed files (including the test files), the numbered scenarios and the verification commands. The result goes to the implementation agent and the allowed files are listed in the pull request; it is not attached to the ticket. If the ticket does not fit the code or leaves a design question open, scoping stops and the ticket is re-sized to M.
- **Full analysis** (M, M Question): records findings, offers one or more options with their trade-offs, recommends one and checks the triggers. When no trigger fires, the recommendation is selected by policy and the selected option's scope is written. One option is enough when one approach is clearly right; do not invent a second one.
- **Interview** (L Question): an interactive analysis with the developer, as in [Working a Question](sizes/l-question.md#working-a-question).

## Analysis triggers and re-sizing

A trigger is an observable reason to ask the developer. The analysis states for each trigger whether it fired and the evidence in one line; a confidence score is not a trigger.

1. Requirements are materially ambiguous.
2. The alternatives produce meaningfully different user-visible behaviour.
3. The decision creates a significant architectural commitment.
4. The change is destructive or hard to reverse.

When no trigger fires, the recommendation is selected by policy: the selection and its reason are recorded as made by policy, not by an agent, and the ticket moves on. When a trigger fires, the ticket parks at its current Status with the trigger shown, and the developer selects. A fired architectural trigger also re-sizes an M to an L, so the decision reaches [the stack](../stack.md) through a Question.

Re-size when new evidence warrants it, and record the old and new Size on the ticket: XS to S when implementation needs more than the ticket says; S to M when scoping finds an open design question, the run is blocked outside its allowed files or the repair limit is reached; M to L on the architectural trigger. Never lower a Size because a diff is small. A change in a risky area, such as authorization, destructive operations or the trust boundary of the home, is not XS merely because it is short.

## By issue type

| Type | What changes in the steps | Commit subject |
| --- | --- | --- |
| Task | Describe the bounded work in Summary and Scope; follow the size steps. | `Task[#N]: ...` |
| Bug | Record reproduction steps, actual and expected behaviour; follow the [Rule](../delivery/testing.md#rule) for the regression test. | `Bug[#N]: ...` |
| Feature | Describe the user need and intended result; follow the size steps. | `Feature[#N]: ...` |
| Maintenance | Describe the maintenance outcome and scope; follow the size steps. | `Maintenance[#N]: ...` |
| Documentation | Describe the documentation change; see [Skipping PR builds](../delivery/testing.md#skipping-pr-builds) for when CI skips. | `Documentation[#N]: ...` |
| Test | The whole outcome is test cases; see [Testing](../delivery/testing.md#test-tickets). Tests belonging to another change stay in that change's ticket. | `Test[#N]: ...` |
| Question | Record a decision. An L has it selected by the developer; an M Question has it selected by policy unless a trigger fires. Derive children from the answer; write each child's outcome, numbered scenarios and **Agreed in #N** constraints at creation, with no scope attachment. Close with `Disposition: Answered` only after all children are closed; Done with Answered may precede closure. | `Question[#N]: ...` |
| Epic | Use only `## Objective` in the body. Follow XL: title `<version> -- Folt`, group release work, close with `Disposition: Released` after all children close. | `Epic[#N]: ...` |
| Incident | Follow [Incidents](incidents.md) for evidence, impact, linked response tickets and resolution; the report is not itself the fix. | `Incident[#N]: ...` |

## Open points

- XL has no decided Status path; confirm which statuses an Epic visits before it is Done.
- Fugue does not yet run scoping, policy selection or automatic Questions; until it does, the orchestrator follows these rules by hand.
- XL has no Component count specified in the supplied decisions; confirm whether any limit applies. The suggested Status paths (XS/S: S0 -> S4 -> S5; M/L: S0 -> S1 -> S3 -> S4 -> S5) were inferred from the size rules and are not yet confirmed.
