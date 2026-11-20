# Work workflow

Use GitHub Issues as the durable record of a change. Keep the issue body current as the work becomes clearer; use comments for short decisions and handoffs, and link substantial design notes or evidence instead of copying long material into comments.

## Stages

Terms are defined in [GLOSSARY.md](../../../GLOSSARY.md).

Sizes decide which stages apply; see [working templates](../issues/rules.md). XS skips analysis and review. S runs scoping, then implementation, with no review. M runs full analysis, implementation and review. L is an interactive Question with no implementation of its own, and XL needs neither. An [XL Epic](../issues/sizes/xl-epic.md) only groups work and has no implementation.

1. **Propose.** Create a work item or epic from its issue form. Agree on the outcome and scope before implementation starts.
2. **Analyse and discuss.** For M and Questions, the orchestrator inspects the code and records the factual findings that bear on the decision. The options stay inline. The orchestrator writes Analysis as if the developer had already asked for context, comparison and the next step, following [Issue format](../issues/issue-format.md#analysis-and-solution-selection), recommends an option and checks the [triggers](../issues/rules.md#analysis-triggers-and-re-sizing). With no trigger fired the policy selects the recommendation; otherwise the ticket parks and the developer chooses. Leave the Solution section pending until the selection is made. Then write the selected option's implementation scope, mark its attachment **[Selected Solution]** and fill Solution with the changes, affected areas, and reason. For S, scoping runs at the start of implementation and derives the scope from the ticket. Split work that cannot be reviewed as one bounded change into linked issues.
3. **Implement.** All agents, including implementation and delegated agents, implement in a dedicated Git worktree for the issue branch, never in the primary checkout. Use one worktree per ticket and do not share a worktree between concurrent implementation work. Dependent tickets may use a [stacked pull request](stacked-prs.md), with each upper branch based on the lower branch. Keep changes within the agreed scope. Implementation agents implement and fix only, following the roles and standard prompt in [Testing](testing.md). Update the issue when a decision changes the plan or scope.
4. **Review.** Follow the size's review rule in [working templates](../issues/rules.md). The orchestrator runs the gate in [Testing](testing.md), then commits in the issue's commit format, pushes and opens or updates the pull request without a further request. Follow the [review workflow](review.md) and link the issue (for example, `Closes #2`). See [Pull requests](#pull-requests) for who merges. Record requested changes on the ticket and address them on the same branch before merging.
5. **Complete.** Record the delivered change and verification in the pull request. Link follow-up work where needed. A linked closing keyword may close the work item when its pull request merges.

## Branches and commits

Each work item gets its own branch and dedicated Git worktree. Before creating the worktree, fetch and fast-forward its base so the branch starts from the current remote state. For an independent ticket, the base is `develop`; for a dependent ticket in an open stack, use the lower branch. All implementation agents use that worktree; the primary checkout is not an implementation workspace. The bottom pull request targets `develop`; upper pull requests target the preceding stack branch. See [Stacked pull requests](stacked-prs.md). Promote releases from `develop` to `main` through a PR. Use `<type>/<issue-number>-<short-slug>` (for example, `feature/2-schema-migrations`). Keep one issue's work on its branch and use focused commits with this format:

```text
<Issue Type>[#<issue-number>]: <short explanation>
```

`<Issue Type>` must exactly match the issue type, including capitalization and wording. For example, Feature uses `Feature[#2]: ...`, Bug uses `Bug[#3]: ...`, and Documentation uses `Documentation[#4]: ...`.

Dependabot PRs use `Dependabot[#0]: <Dependabot's description>`. `Dependabot` is the only subject type that is not an issue type. `#0` means there is no ticket and none is needed: no epic or issue. Dependabot sets the subject through `commit-message.prefix` in [.github/dependabot.yml](../../../.github/dependabot.yml). If a Dependabot PR's commits do not follow this format, squash them into one commit with that subject. Always run the read-only [Dependabot review](review.md#dependabot-prs) before merging.

### Commit size

Keep commits small and focused. When a ticket covers several distinct changes, group them into separate commits (for example code, CI and documentation) instead of one large commit. Where practical, each commit should build on its own.

### Commit body

Every commit body has a `**Summary**` heading followed by one to three sentences on why the commit exists. When a commit touches more than one thing, or the reason is not obvious from the diff, add a `**Changes**` heading with one bullet per notable change in the form "what changed: why it changed". Each bullet gives the reason, not the lines changed. A one-line fix may use the Summary only. Do not repeat the issue number (the subject has it) and do not add attribution lines. The subject is imperative and capitalised, has no trailing period, and stays at about 72 characters or fewer.

The `[SKIP BUILD]` marker and the rules for skipping PR builds are in [Skipping PR builds](testing.md#skipping-pr-builds).

```text
Maintenance[#8]: Add the Rust workspace

**Summary**

Later work needs a buildable base, so the repository gets a minimal workspace
before any feature is written.

**Changes**
- Add a library crate and a CLI crate: the split keeps the core logic reusable
  by other programs.
- Mark both crates unpublishable: the API is not stable yet.
```

Stage only files belonging to the change and review the staged diff before committing. See step 4 of [Stages](#stages). After pushing a commit, do not rewrite it except for unmerged branches of a stack: rebase separately, run the local suite gate in [Testing](testing.md) on every affected branch, then push with `--force-with-lease` only when all pass. Stop and report if the local and remote stacks have diverged. See [Pull requests](#pull-requests) for who merges.

## Issue types, fields and the Folt project

Follow [Ticket planning](../issues/ticket-planning.md) for assignment, relationships, Priority, milestones and iterations. Follow [working templates](../issues/rules.md) for issue types, required fields, sizes, statuses and closing rules.

## Dependencies and grouped work

See [Dependencies and grouped work](../issues/ticket-planning.md#dependencies-and-grouped-work).

## Pull requests

Give every pull request a milestone, an assignee and the Folt project. See [ticket planning](../issues/ticket-planning.md#at-issue-creation) for ticket metadata.

See step 4 of [Stages](#stages). Follow [Review](review.md) for read-only agent review, posting findings, recording requested changes on the ticket and fixing accepted findings on the same branch. Where review applies, repeat it until a review has no findings. See [Pull requests](#pull-requests) for who merges.

For a stacked pull request, state its position in the stack and identify the pull request below it. Where the [size requires review](../issues/rules.md), review the pull request using [Review](review.md).

The title uses the commit subject format, `<Issue Type>[#<issue-number>]: <short explanation>`. The description follows [the pull request template](../../../.github/PULL_REQUEST_TEMPLATE.md), in this order:

1. **Summary:** why the pull request exists, in one to three sentences.
2. **Changes:** one bullet per notable change as "what changed: why it changed", grouped by commit when there are several.
3. **Related work:** the owning issue, with `Closes #N` only when the pull request completes it. Use a full link for another repository.
4. **Verification:** record suites and CI results as defined in [Testing](testing.md#verification-in-the-pr).
5. **Risks and follow-up:** material compatibility, configuration or migration implications, and links to remaining work.

Do not add attribution lines.

- Use a focused title and explain the change, rationale, and verification.
- Link the work issue with a closing keyword when merging should close it.
- Keep unrelated changes out of the pull request; create another issue when needed.
- Do not claim verification that was not run. List relevant checks and manual review plainly.
- Merge only when the developer merges or instructs the agent to merge, and after the required checks and review are complete. The instruction covers the pull request as the developer last saw it. If the Git branch changes afterwards, including conflict resolution or new Git commits, the instruction lapses: stop when checks are green and ask again. Never chain cleanup after a merge command that may have failed.

## Blocked work

When work is blocked, update the issue with what is blocked, why, and the specific next action or decision needed. Link the dependency or related issue. Resume by updating the status and issue plan.

## Issue-specific developer instructions

Explicit developer instructions override the default workflow for that work. Record any such override and its limits in the issue and the pull request. An instruction for one issue does not authorize merges.
