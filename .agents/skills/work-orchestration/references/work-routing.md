# Where work information belongs

Four systems hold work, and each answers a different question. Route by the question
answered and the lifetime, never by the topic.

| Artefact | Answers | Reader | Dies when |
| --- | --- | --- | --- |
| Tracker item | Why the work exists, what changes, for whom, and where it stands | Anyone, including people who do not read code | Never. It is product history |
| Technical issue | What technical work exists, under what constraints, and how we know it is done | Engineers and coding agents | The work lands |
| Pull request | This exact change, why it is written this way, and how it was verified | Reviewers | Merge |
| Decision record | Why the system is shaped this way, and what it cost | A future engineer with no context | Superseded, never edited |

## The two tests

**For a tracker description**: would this line be false after the pull request merges? If
yes, it belongs in the technical issue or the pull request.

**For a technical issue**: does anything here need to survive the session that produced
it? If not, it belongs in the pull request, and no issue should exist.

## Routing by information type

`●` primary home, `○` acceptable secondary, blank means it does not belong there.

| Information | Tracker | Issue | PR | Record | Code |
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

Some rows have no home because the forge renders them natively: the file list, the commit
list, check status, linked issues, labels, reviewers, and mergeability. Prose repeating
those belongs nowhere.

Three exceptions carry most of the judgement.

**Stack traces and logs.** One frame or one line, as the evidence for a claim, belongs in
the issue. The full output belongs in a collapsed block or the pull request. Ask whether
the text is the evidence or the raw material.

**An implementation plan in an issue.** Acceptable when it sequences several pull
requests, which is scope. Not acceptable when it lists the steps inside one pull request,
which is pre-solving.

**Product acceptance criteria in an issue.** Acceptable as a quoted line when it
constrains implementation. Never as a copy of the tracker item.

## Information that belongs outside these four systems

The table above routes work information. Some information that arrives during work
belongs to none of the four systems, and putting it in one of them is the commonest way
it gets lost. `●` authoritative, `○` acceptable summary or link.

| Information | Agent instructions | Skill | README | CONTRIBUTING | Dedicated file | Repo metadata | Ideation vault | Product docs | Code | Tooling |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Canonical validation command | ● | | ○ | ○ | | | | ○ | | ● |
| Coding convention, enforceable | | | | ○ | | | | | ○ | ● |
| Coding convention, not enforceable | ● | | | ○ | | | | | ○ | |
| Generated-file rule | ● | | | | | | | | ○ | ○ |
| Deterministic formatting | | | | ○ | | | | | | ● |
| Reusable procedure | ○ | ● | | | | | | | | |
| Operational runbook | | ○ | | ● | | | | | | |
| Setup, commands, environment variables | ○ | | ○ | ● | | | | | | |
| Contributor workflow | | | ○ | ● | | | | | | |
| End-user usage | | | ○ | | | | | ● | | |
| API or CLI reference | | | ○ | | | | | ● | ● | |
| Vulnerability reporting | | | ○ | ○ | ● | | | | | |
| Support channels | | | ○ | | ● | | | | | |
| Repository category, discovery terms | | | ○ | | | ● | | | | |
| Maintainer attribution | | | ● | ○ | | | | | | |
| Local invariant, surprising algorithm | | | | | | | | | ● | |
| Exploratory thinking, not yet work | | | | | | | ● | | | |

Three rows carry the judgement. A **validation command** legitimately appears in several
places, because each audience needs it: the agent instruction file states it for agents,
CONTRIBUTING for humans, tooling enforces it. Only one of those is authoritative, and the
others are stating it for their reader rather than owning it. A **coding convention** is
authoritative in tooling whenever it can be enforced, and in the agent instruction file
only when it cannot. **Exploratory thinking** has no home in a repository at all: an idea
that is not yet work lives in the user's ideation vault, and moving it into a tracker
before someone decides to build it turns a thought into a commitment nobody made.

Two kinds of information look like they want a repository document and do not. A
**one-time investigation** produces findings, and findings belong in the issue or tracker
item that asked the question, pinned to the commit they were true at. A document reporting
them is stale the moment the code moves, and nobody deletes it. An **un-actioned
recommendation** is work: file it, or accept that it is not going to happen. Neither
becomes a file in `docs/`.

Enforcement has a practical constraint. Deterministic enforcement means whatever the
project actually runs: a script, a git hook, or an existing local validation command.
**Check that the destination exists before naming it.** Routing something to CI in a
repository with no pipeline names a destination that would have to be built first, and
saying so is the honest output.

## Valid paths

All of these are correct, and none is more official than another:

```
tracker → PR                       most small work
tracker → issue → PR               technical context worth keeping
tracker → issue → PRs              work spanning several changes
issue → PR                         work with no product dimension
issue → record → PR                work that surfaces a durable decision
```

Do not create an intermediate artefact for consistency. Each one exists because something
needed to live there.

## What is never copied

Product rationale, priority, dates, and status live in the tracker and are linked, never
duplicated. Implementation detail lives in the pull request and is linked, never
summarised upward. Architectural rationale lives in a decision record and is linked from
both.
