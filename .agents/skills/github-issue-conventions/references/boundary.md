# Linear, GitHub, pull requests, and ADRs

## Contents

- The four layers
- The two tests
- Routing content
- Three exceptions that carry the judgement
- What is never copied

## The four layers

Route by the question answered and the lifetime, never by the topic.

| Artefact | Answers | Reader | Dies when |
| --- | --- | --- | --- |
| Linear | Why the work exists, what changes, for whom, and where it stands | Anyone, including people who do not read code | Never. It is product history |
| GitHub issue | What technical work exists, under what constraints, and how we know it is done | Engineers and coding agents | The work lands |
| Pull request | This exact change, why it is written this way, and how it was verified | Reviewers | Merge |
| ADR | Why the system is shaped this way, and what it cost | A future engineer with no context | Superseded, never edited |

## The two tests

**For a Linear description**: would this line be false after the pull request merges? If
yes, it belongs in the GitHub issue or the pull request.

**For a GitHub issue**: does anything here need to survive the session that produced it?
If not, it belongs in the pull request, and no issue should exist.

## Routing content

`●` primary home, `○` acceptable secondary, blank means it does not belong there.

| Information | Linear | GitHub issue | PR | ADR | Code |
| --- | --- | --- | --- | --- | --- |
| User pain | ● | ○ | | | |
| Business motivation | ● | | | ○ | |
| Desired product outcome | ● | | | | |
| Product acceptance criteria | ● | ○ | | | |
| Product scope and non-goals | ● | ○ | | | |
| Technical objective | | ● | ○ | | |
| Filenames and paths | | ● | ● | | ● |
| Modules and packages | | ● | ● | ○ | ● |
| Functions and classes | | ○ | ● | | ● |
| Stack traces | | ○ | ● | | |
| Log excerpts | | ○ | ● | | |
| Reproduction steps | | ● | ○ | | |
| Environment | | ● | ○ | | |
| SQL | | ○ | ● | | ● |
| API payloads | | ● | ● | | ● |
| Schemas | | ○ | ● | ○ | ● |
| Migration requirements | | ● | ● | ○ | |
| Technical constraints | | ● | ○ | ● | ○ |
| Investigation findings | | ● | ○ | ○ | |
| Implementation options | | ○ | ● | ● | |
| Implementation plan | | ○ | ● | | |
| Rejected approaches | | ○ | ○ | ● | |
| Test plan | | ● | ○ | | |
| Exact test output | | | ● | | |
| Benchmarks | | ● | ● | ○ | |
| Architecture diagrams | | ○ | | ● | ● |
| Rollout and sequencing | | ● | ○ | ○ | |
| Observability requirements | | ● | ○ | | ● |
| Compatibility requirements | | ● | ○ | ● | ● |
| Security implications | | ● | ● | ● | |
| Follow-up work | | ● | ○ | | |
| Architectural rationale | | | ○ | ● | |
| Discovered technical debt | | ● | ○ | | |
| Implementation actually chosen | | | ● | | ● |
| Code-level rationale | | | ● | | ● |
| Exact commands run | | | ● | | |
| Manual verification | | | ● | | |
| Screenshots of a visible change | | ○ | ● | | |
| Known limitation | | ○ | ● | | ● |
| Reviewer concern | | | ● | | |
| Feature flag and rollout gating | | ● | ● | | |

Some rows have no home because GitHub renders them natively: the file list, the commit
list, check status, linked issues, labels, reviewers, and mergeability. Prose repeating
those belongs nowhere.

## Three exceptions that carry the judgement

**Stack traces and logs.** One frame or one line, as the evidence for a claim, belongs in
the issue. The full output belongs in a collapsed block or the pull request. Ask whether
the text is the evidence or the raw material.

**An implementation plan in an issue.** Acceptable when it sequences several pull
requests, which is scope. Not acceptable when it lists the steps inside one pull request,
which is pre-solving.

**Product acceptance criteria in an issue.** Acceptable as a quoted line when it
constrains implementation. Never as a copy of the Linear item.

## What is never copied

Product rationale, priority, dates, and status live in Linear and are linked, never
duplicated. Implementation detail lives in the pull request and is linked, never
summarised upward. Architectural rationale lives in an ADR and is linked from both.
