# Issue format

The issue body is the shared record of why work exists, what it includes, and how completion will be judged. Keep it short enough to scan. Put larger research or design material in a linked document; comments should remain brief.

## Work items

Terms are defined in [GLOSSARY.md](../../../GLOSSARY.md).

Use the forms under `.github/ISSUE_TEMPLATE`, one per issue type: Task, Bug, Feature, Question, Epic, Maintenance, Documentation, Incident and Test. Follow the [working templates](rules.md) for what each size contains; the table below describes sections when their stage applies. Work items contain exactly Summary, Scope, Analysis, Solution and Details. XS and S fill Summary and Scope only; see [XS and S body layout](#xs-and-s-body-layout).

| Section | When to fill it | Purpose |
| --- | --- | --- |
| Summary | At creation | State the current situation or user need, expected outcome, and who benefits. |
| Scope | At creation | Name affected area and what is in and out. |
| Analysis | During refinement | Record current behavior, cause when relevant, constraints, compatibility, and viable options. |
| Solution | After the selection, for M and Questions | Summarize the selected solution, affected areas, the reason for choosing it and whether the developer or the policy selected it. XS and S have none: write `_Not applicable._` |
| Details | After investigation | Link the code findings attachment and summarize two to four key facts. |

For a defect, describe what happens and what should happen without presuming a cause. For new work, describe the user need and the result. Keep `In scope` and `Out of scope` explicit. Avoid unresolved questions in an issue marked ready to implement; resolve them during refinement or create a discovery issue.

## Analysis and solution selection

Details holds a link to the full code findings and two to four key points the reader must not miss. State the findings and their implications; do not use this section as a list of file paths or routes. Exact code references belong in the attachment. Leave Details pending until investigation has produced findings.

```markdown
## Details

**Code findings:** [<N>-code-findings.md](<attachment-url>)

**Key points**
- <important finding>
- <important finding>
```

The orchestrator, not the implementation agent, investigates and writes Analysis inline in the ticket, as if the developer had already asked for context, comparison and the next step. A recommendation is not a selection. The developer selects, or the policy selects when no [trigger](rules.md#analysis-triggers-and-re-sizing) fires; an agent never does.

XS and S tickets have no Analysis and no Solution. S derives its scope by [scoping](rules.md#analysis-depths) when its run starts. XL has no analysis.

1. Record the facts that bear on the decision, with exact `file:line` references and each marked verified or unverified, in `<N>-code-findings.md`, linked from Details. Do not tour the repository.
2. Write the options inline in Analysis using the layout below. One option is enough when one approach is clearly right; offer alternatives only when there is a real trade-off. An option is a short paragraph and its trade-offs, not a scope.
3. Recommend one option and check the four triggers, each with its evidence.
4. Select. With no trigger fired, the policy selects the recommendation and the selection is recorded as made by policy with the reason. With a trigger fired, park the ticket and wait for the developer's choice. If no option fits, revise the options.
5. Write the implementation scope of the selected option only, as `<N>-solution.md`, using the layout below, and mark the attachment **[Selected Solution]**. Fill Solution from the selection. The implementation agent must be able to run the scope as written, without another prompt.

### Analysis layout

Use this order in the ticket's `## Analysis` section:

1. A `> [!IMPORTANT]` callout stating the decision in one sentence.
2. `### Where we are`: bullets of facts and their consequences, followed by **Out of scope** and **Terms**. Use terms exactly as defined in [GLOSSARY.md](../../../GLOSSARY.md); list a missing term as unresolved, never invent it.
3. A comparison table when there are two or more options. Use rows such as rule duplication, diff size, main risk and reader.
4. One `### Option N: <name>` block per option, with bold labels **You get**, **Changes** and **Watch out**. Options have no scope attachment.
5. `### Recommendation`: one option and one sentence of reason.
6. `### Trigger check`: the four triggers, each marked fired or not, with one line of evidence.
7. After the selection, one line naming who selected and the reason, and the **Implementation scope** link to the selected scope attachment. When a trigger fired, add a `> [!TIP]` callout telling the developer to reply with the option number.

Use short bullets, bold labels, tables and callouts. No paragraph may exceed three lines. Use no emojis. A short skeleton for two options:

```markdown
## Analysis

> [!IMPORTANT]
> <Decision in one sentence.>

### Where we are

- <Fact and its consequence.>

**Out of scope**

- <Excluded work.>

**Terms**

- <Glossary term and its exact meaning, or missing term listed as unresolved.>

| Comparison | Option 1 | Option 2 |
| --- | --- | --- |
| Rule duplication | <Extent> | <Extent> |
| Diff size | <Extent> | <Extent> |
| Main risk | <Risk> | <Risk> |
| Reader | <Who benefits> | <Who benefits> |

### Option 1: <name>

**You get**

- <Result.>

**Changes**

- <Change.>

**Watch out**

- <Risk or limitation.>

### Option 2: <name>

**You get**

- <Result.>

**Changes**

- <Change.>

**Watch out**

- <Risk or limitation.>

### Recommendation

Option <N>: <one sentence of reason>.

### Trigger check

| Trigger | Fired | Evidence |
| --- | --- | --- |
| Requirements materially ambiguous | <yes or no> | <One line.> |
| Meaningfully different user-visible behaviour | <yes or no> | <One line.> |
| Significant architectural commitment | <yes or no> | <One line.> |
| Destructive or hard to reverse | <yes or no> | <One line.> |

**Selected:** Option <N> by <the developer or policy>: <reason>.

**Implementation scope:** [<N>-solution.md](<attachment-url>)

> [!TIP]
> Only when a trigger fired: reply with the option number.
```

### XS and S body layout

XS and S carry their outcome in the ticket body and nothing else; there is no Solution section and no scope attachment. Fill Analysis, Solution and Details with `_Not applicable._`.

- **Summary:** the outcome and who benefits, in one or two sentences.
- **Scope:** `In scope`, `Out of scope`, and two to five numbered scenarios, each stating something that can be observed when the work is done. XS needs two or three.
- **Agreed in #N:** under Scope, when a parent Question decided something the work must respect. State each decision as a constraint, in the Question's own words. A vague decision makes scoping guess.
- **File hints:** optional, under Scope. They name areas, not the files the agent may touch; scoping decides those.

### Implementation scope layout

The `<N>-solution.md` attachment is the complete scope of the selected option. S derives the same content by scoping at run time and does not attach it. Use these sections in order:

- **Allowed files:** the exact files the implementation agent may touch, including their test files, introduced by the sentence "You may touch only these files" and one bullet per path in backticks. Tooling reads this list as the boundary. The worktree path and the Git branch come from the prompt, not from the scope.
- **Source of truth** and **Vocabulary changes:** include both when words change. State the authoritative definitions and exact vocabulary changes; leave missing terms unresolved rather than inventing them.
- **Changes:** one line per file with the behaviour change it must make. Give exact text only where exactness is the decision: public signatures, error variants, glossary wording and output formats. Name symbols, not line numbers.
- **Scenarios:** numbered, each observable. These define done and are what the review checks.
- **Do not change:** the boundaries the implementation agent must preserve.
- **Verification:** commands and their expected output.
- **Final message:** what the implementation agent returns.

The standing rules (implement only, no Git commit, push or `gh`, no emojis, read `GLOSSARY.md` first and use its terms exactly) are in [the implementation role file](../agents/implementer.md); the scope does not repeat them. Cite the numbered rules of [Rust guidelines](../guides/rust.md) and [Standards](../guides/standards.md) that matter for the ticket. Commit plans, PR fields and review focus are orchestrator work and are not part of the scope.

For M and Questions, Solution contains the selected result, not another plan document:

```markdown
## Solution

> More details are in the attachment tagged **[Selected Solution]** above.

- <what changes, one short bullet each>

**Affected areas:** <pages, routes, or components covered by the chosen solution>

**Selected by:** <the developer or policy>

**Why this one:** <one sentence>
```

Match the affected-area list to Scope: one page for a page change, a list for a feature group, or a table for a change across the application. Copy the affected areas from the selected solution attachment.

Findings contain facts; the solution attachment contains the executable implementation scope of the selected option. Long material stays in attachments and issue comments stay short. Name files with the issue number so they remain identifiable.

Upload Markdown files using the `gh-attach` extension, then link their returned URLs from the issue. See [the upload procedure](../delivery/github-cli.md#markdown-attachments). Verify the uploaded contents and issue links. Do not substitute public gists, repository commits, or local filesystem paths for GitHub attachments. If no signed-in browser session is available, keep upload pending and report that specific blocker.

## Epics

An epic contains only `## Objective` followed by a quick explanation of what we want from the release. Its title is `<version> -- Folt`. Close it with the Disposition issue field set to Released only after all its children are closed; see the [XL template](sizes/xl-epic.md). Place release work under its existing delivery group or Question where applicable; otherwise default to #1 unless the developer specifies another parent. See [workflow](ticket-planning.md#dependencies-and-grouped-work). Use native parent relationships; the description does not repeat them.

## Notes, attachments, and links

- Use comments for concise decisions, questions, and handoffs; update the body when the issue's agreed scope or plan changes.
- Link long design notes and investigation findings instead of pasting them in full. Keep sensitive data, credentials, and private user information out of issues and attachments.
- Link dependencies and related work with `#<issue-number>` references.
- Pull requests should state what changed, how it was checked, and any follow-up work, with links to the relevant issues.
