# Folt development process

Staged issue workflow shared by the Folt repositories. Keep process docs under docs/dev and issue forms under .github/ISSUE_TEMPLATE. GitHub issues are the durable work record.

See [Issue format](issues/issue-format.md#work-items) for the work-item body sections. The orchestrator investigates and attaches findings and separate proposals; the developer selects the solution before implementation where analysis applies; see the working templates. Details contains the findings attachment and two to four key points. Do not add Acceptance criteria, Implementation plan or Closeout sections. Record delivery and performed verification in the PR.

See [Dependencies and grouped work](issues/ticket-planning.md#dependencies-and-grouped-work) for parents and prerequisites. See [Issue format](issues/issue-format.md#epics) for release Epics.

| Guide | Covers |
| --- | --- |
| [Glossary](../../GLOSSARY.md) | The terms of Folt and of the process, defined once |
| [Process](README.md) | This index, with the rules for body sections and parents |
| [Stack](stack.md) | Confirmed technical choices and pending decisions |
| [Repository settings](repository.md) | Recorded GitHub settings, drift checks and application |
| [Verification](verification.md) | Checks actually performed and their status |
| [Orchestrator rules](agents/orchestrator.md) | What the orchestrator does that agents do not |
| [Analysis agent](agents/analyst.md) | Role file of the analysis agent |
| [Implementation agent](agents/implementer.md) | Role file of the implementation agent |
| [Review agent](agents/reviewer.md) | Role file of the review agent |
| [Working templates](issues/rules.md) | How to work a ticket by size and issue type: size table, shared steps and open points |
| [XS template](issues/sizes/xs.md) | Working an XS ticket |
| [S template](issues/sizes/s.md) | Working an S ticket |
| [M template](issues/sizes/m.md) | Working an M ticket |
| [L Question template](issues/sizes/l-question.md) | Working an L ticket and Working a Question |
| [XL Epic template](issues/sizes/xl-epic.md) | Working an XL release Epic |
| [Issue format](issues/issue-format.md) | Work-item and release-epic format and analysis attachments |
| [Ticket planning](issues/ticket-planning.md) | Issue types and fields, assignment, priority, relationships, dependencies and grouped work, milestones and iterations |
| [Incidents](issues/incidents.md) | Reports, evidence, linked response work and resolution |
| [Workflow](delivery/workflow.md) | Issue stages, branches, commits and PRs |
| [Stacked pull requests](delivery/stacked-prs.md) | Creating, updating and merging native PR stacks |
| [Review](delivery/review.md) | Read-only agent reviews and posting findings |
| [Testing](delivery/testing.md) | Test rule, push gate, CI review, roles |
| [GitHub CLI](delivery/github-cli.md) | Issues, native relationships, Markdown uploads and PR commands |
| [Registration](library/register.md) | folt-core explicit-path registration behaviour |
| [Prune](library/prune.md) | folt-core prune behaviour |
| [Layers](library/layers.md) | folt-core layer table and the layer check |

[AGENTS.md](../../AGENTS.md) points each agent to its role file. [CONTRIBUTING.md](../../CONTRIBUTING.md) is the contributor entry point. The [working templates](issues/rules.md) hold the size, status and closing rules. Use the other guides for their specific procedures.
