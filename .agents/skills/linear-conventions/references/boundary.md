# Linear, GitHub, pull requests, and ADRs

## Contents

- The four layers
- Routing content
- When a GitHub issue earns its place
- Linking instead of copying
- Ambiguous cases, resolved

## The four layers

Route by the question answered and the lifetime, never by the topic.

| Artefact | Answers | Reader | Dies when |
| --- | --- | --- | --- |
| Linear | Why the work exists, what changes, for whom, and where it stands | Anyone, including people who do not read code | Never. It is product history |
| GitHub issue | What technical work exists, under what constraints, and how we know it is done | Engineers and coding agents | The work lands |
| Pull request | This exact change, why it is written this way, and how it was verified | Reviewers | Merge |
| ADR | Why the system is shaped this way, and what it cost | A future engineer with no context | Superseded, never edited |

What follows is the Linear side of that boundary.

The single test for a Linear description: **would this line be false after the pull request merges?** If yes, it belongs in the pull request or in a comment as a dated event.

Most changes need no ADR. If the reasoning is visible in the diff, and nobody would ask "why is it like this" in a year, the pull request is the record. Suggesting a decision record for every technical choice devalues the log, which only works because it is short.

## Routing content

| Content | Default home | Legitimate in Linear when |
| --- | --- | --- |
| Filenames and paths | Pull request | The file is the deliverable, or the issue is delegated to an agent and the path removes ambiguity |
| Module, class, function names | Pull request | The symbol is the user-visible contract, or as above |
| Stack traces | Pull request, GitHub issue, or an attachment | One error line as evidence of the symptom, in a collapsible section |
| SQL | Pull request | Never. The existence, blast radius, and reversibility of a migration belong in Linear. The statements do not |
| Code snippets | Pull request | A one-line API shape when that shape is the thing being agreed |
| API payloads | A document or GitHub issue while under design | The field list when acceptance depends on it |
| Migrations | Pull request, plus a runbook document | Existence, risk, reversibility, and required downtime, because these change scope |
| Test implementation | Pull request | Never. How we will know it works belongs in Linear. The test code does not |
| Implementation alternatives | ADR when architectural, otherwise the pull request | The one alternative a stakeholder specifically asked about |
| Technical investigation | GitHub issue or a Linear document | The conclusion, in one paragraph, as a comment |
| Performance measurements | Pull request or document | The number that justifies the work or defines done, with its baseline and method |
| Architecture diagrams | ADR or repository documentation | A scope or dependency diagram a non-engineer needs |

Naming a file in a Linear issue is correct when the issue is being handed to an agent. Linear's own guidance for delegated issues points at the entry point, names the existing pattern to reuse, and states what must not change, because that is what removes ambiguity before an agent starts exploring. This is a narrow exception: the detail constrains the work rather than describing it, and it is written for the implementer, not the stakeholder.

## When a GitHub issue earns its place

The default technical artefact is the pull request. Open a GitHub issue only when one of these is true:

1. The technical work will span more than one pull request and needs a stable place for cross-pull-request state.
2. An investigation must happen before any code exists, and its trail is long enough to bury a Linear comment thread.
3. An outside contributor needs to see or take the work.

When a GitHub issue exists, the Linear issue keeps the outcome and links to it. The Linear issue never becomes a stub, because the reader who needs the outcome is not the reader who clicks through.

## Linking instead of copying

A link is almost always better than a copy. Two mechanical facts make this concrete.

A pull request links to a Linear issue in three ways: the issue identifier in the branch name, the identifier in the pull request title, or a magic word in the description such as `Fixes ENG-23`. Linking drives the team's status automation, so the status moves without anyone updating it. Non-closing words, including `ref`, `part of`, and `contributes to`, link without completing the issue.

Linear posts a linkback comment on the pull request that contains the issue title and description. Whatever is written in the Linear description is therefore read by reviewers on GitHub too. That is another reason to keep it short and outcome-shaped instead of turning it into a second specification.

On GitHub's side, closing keywords only close automatically from the default branch, and cross-repository links need the `owner/repo#number` form.

Linear can also show pull request diffs, checks, and review threads against the issue when code access is enabled. Where that is on, copying pull request status into an issue duplicates something rendered a click away.

## Ambiguous cases, resolved

| Case | Resolution |
| --- | --- |
| "We need to choose between two schema sources of truth" | ADR. The Linear issue is `Record the schema source of truth decision`, its outcome is an accepted ADR, and the analysis lives in the record |
| "The orchestrator is 2,222 lines and hard to change" | Not an object yet. It is an observation. It becomes an issue when someone names what changes as a result |
| "Investigate why staging is slow" | A Linear issue holding the question and a timebox. Findings in a comment. An ADR only if the finding implies an architectural change |
| "Upgrade a dependency to patch a CVE" | Pull request only, unless it is tracked for compliance. Linear adds nothing a changelog does not |
| "Add auth" | Nothing yet. Resolve to an outcome first, then choose the object |
| "A customer says export is broken" | Linear issue, through Triage, quoting the customer rather than paraphrasing |
| "Refactor the sandbox module" | Pull request if it is incidental to other work. A Linear issue only if it has a stated consequence |
| "Document the agent inventory" | Linear issue whose deliverable is a repository document. The inventory itself never goes in the issue |
| A decision was made in an issue comment thread | Leave the thread. Add a one-line decision note to the description if it changes scope, and write an ADR if it is architectural and expensive to reverse |
