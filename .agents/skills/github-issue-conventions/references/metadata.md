# Metadata

## Contents

- Check what the repository has
- Preference order
- Issue types
- Labels
- Issue fields
- Milestones
- Projects
- Templates and forms
- Linking Linear, pull requests, and ADRs

## Check what the repository has

Establish two things before relying on any feature below.

**Whether the owner is an organisation.** Issue types and issue fields are organisation-level features, so a repository owned by a user has neither. `GET /orgs/{owner}/issue-types` returns 404 for a user.

**Whether the installed GitHub CLI has the relationship flags.** The CLI gained `--type`, `--parent`, `--add-sub-issue`, `--blocked-by`, and `--blocking` on `gh issue create` and `gh issue edit` on 2026-06-10. Older versions lack them, so set relationships and types through `gh api` instead.

Verify:

```bash
gh --version
gh issue create --help | grep -E '\-\-(type|parent|blocked-by)'
gh api repos/{owner}/{repo} --jq .owner.type
```

## Preference order

1. **Relationships**: parent, sub-issue, blocked by, blocking. They carry execution semantics nothing else expresses, and they drive `is:blocked` and the Blocked indicator.
2. **Issue type**, where available. One dimension, one value, visible everywhere.
3. **Assignee and state.** Who, and what stage.
4. **Labels**, only where a named filter or automation consumes them.
5. **Issue fields**, only for a dimension GitHub must own that Linear cannot see.
6. **Free text**, last.

Issue fields rank below labels here, which inverts the usual advice. The reason is specific to this workflow: their four defaults are Priority, Effort, Start date, and Target date, and Linear owns every one of those. Adopting them would create the duplicate state the whole split exists to avoid.

## Issue types

Where an organisation provides them, use the native type and **never put `[bug]`, `[feature]`, or `[task]` in the title**.

Without an organisation, the fallback is: **the title's grammatical form carries the kind of work**. A declarative title is a problem, an imperative one is a change, an interrogative one is an investigation. That is enough for a human reader and for search.

Add type labels only if the backlog grows past the point where it can be read, and then at most three. Do not create them in advance.

## Labels

Before creating or applying one, answer: **which filter, routing decision, or automation does this enable?** No answer means no label.

Where a repository has no issue labels in use yet, start with at most two, created only when first genuinely needed:

- `agent-ready`, gating delegation to a coding agent.
- `needs-decision`, when someone will actually filter on it.

Avoid: synonyms, status encoded as labels when state and the linked pull request already carry it, priority, anything restating the repository or the issue type, and anything created for a single issue.

## Issue fields

Available only to repositories owned by an organisation. Where available, adopt a field only when it supports a real query, routing decision, automation, or planning action **that Linear does not already own**. Priority, effort, start date, and target date are Linear's. Component is the plausible candidate, and only in a repository large enough that routing by component is a real activity.

## Milestones

**Not used by default.** A milestone carries a due date and a progress bar, which is a second progress system alongside Linear's projects and milestones.

The narrow legitimate case is a repository-scoped boundary that is not a product plan: a release tag, a version cutover, a platform drop. `v3.0` or `Node 24` are properties of the repository. `Q3 billing work` is not, and belongs in Linear.

## Projects

**Not used by default.** Every capability GitHub Projects offers, table and board and roadmap views, custom fields, charts, and automation, duplicates something Linear already owns.

The one thing Projects uniquely provides is the **hierarchy view**, which renders sub-issue trees with grouping and filtering and is not available in the repository issues list. That justifies exactly one case: a large migration whose sub-issue tree is the plan and which no single issue list can show. Create it for the migration, add no custom fields, and delete it when the migration closes.

Otherwise, saved searches cover the need: `is:open is:blocked`, `is:open label:agent-ready`, `blocked-by:owner/repo#123`.

## Templates and forms

**No issue forms** where every issue is written by the team or by an agent. Their value is validating input from people who do not know the codebase, so there the value is near zero and the cost is real: a form produces the same headings on every issue whether the content exists or not, which is the empty-heading failure this whole system avoids.

Where there is no `.github` directory, do not create one as a side effect of writing an issue. If one is created, the single template artefact worth having is `.github/ISSUE_TEMPLATE/config.yml` with a contact link routing security reports to private vulnerability reporting.

## Linking Linear, pull requests, and ADRs

**Linear.** One line at the end of the body: `Linear: ENG-23 <url>`. Never copy the Linear description, its acceptance criteria, its priority, or its dates. Summarise only the technical implication that constrains execution, and link the rest. One GitHub issue links to at most one Linear item; one Linear item may have several GitHub issues.

**ADRs.** Link the record that constrains the work, as `Decided`. When implementation surfaces a new durable architectural decision, recommend the ADR workflow rather than burying the rationale in a comment. Ordinary implementation choices are not ADRs.

**Pull requests.** Use a closing keyword only on the pull request that actually completes the issue, in its description, since keywords close only from the default branch. For work spanning several pull requests, reference the issue without a closing keyword (`Part of #123`, `Refs #123`) on the partial ones, and close either with the final pull request or manually with a comment saying what completed it.

Never put a closing keyword on the first pull request of a multi-part change. It closes an issue whose work is not done, and the reopen loses the thread.

One pull request may close several issues, each with its own keyword: `Closes #12, closes #14`. A keyword without its own issue reference closes nothing.

When a pull request reveals that an issue no longer needs doing, close the issue as **not planned** with one line saying what changed. Do not close it as completed, and do not quietly let the pull request claim work it did not do.
