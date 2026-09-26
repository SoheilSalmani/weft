---
name: github-issue-conventions
description: House conventions for GitHub Issues used as technical work orders. Covers when an issue should exist at all, how to title and write one, how to mark facts against hypotheses, what metadata to use, and how issues link to Linear, pull requests, and ADRs. Use when reading, writing, judging, or restructuring a GitHub issue.
---

# GitHub issue conventions

A GitHub issue exists to preserve useful technical execution state between understanding a problem and completing the code change. Nothing else.

It is not a ticket, not a status record, and not a diary. Linear owns why the work matters to anyone outside engineering. The pull request owns what the code became. The issue owns the space in between, which is where technical intent lives before code exists.

**An issue is never mandatory.** Linear straight to a pull request is a correct workflow, and it is the right one for most small work. Do not create an intermediate artefact for consistency.

## What a reader must get in one minute

1. What technical work exists, in one sentence.
2. Why it needs doing, in engineering terms.
3. What must not break.
4. How we know it is finished.
5. Which parts are fact and which are guesses.
6. Where to start looking.
7. The related Linear item, ADR, or prior pull request, in one click.

An issue that fails these is failing, however complete it looks.

## Does it deserve an issue

Create one when **at least one** holds:

- **Persistence.** Knowledge produced now will be needed after this session, and no pull request exists to hold it.
- **Multiplicity.** More than one pull request, or more than one person or agent.
- **Delegability.** Someone could pick this up cold, and the issue is what makes that possible.
- **Discovery.** Found while doing something else, and otherwise lost. The highest-value case, and the one most often skipped.
- **Sequencing.** Other work genuinely cannot start until this lands.

Skip it when one pull request finishes the work, the pull request description carries everything durable, and nobody needs to know before that pull request exists.

Saying "this does not need an issue" is a correct and useful answer. Give the reason and offer the pull request instead.

## Mark every claim

An unmarked sentence in a work order reads as a requirement. Anything less certain than a requirement carries its status:

| Marker | Means | What a reader does with it |
| --- | --- | --- |
| `Must` | Constraint, not negotiable, with its reason | Respect it, and speak up if it looks impossible |
| `Decided` | Already chosen, with who and where | Do not reopen without asking |
| `Suggested` | A starting idea | Free to discard after reading the code, and say why |
| `Hypothesis` | Unconfirmed belief | Verify before relying on it |
| `Unknown` | A gap, with what would close it | Investigate, or ask |

This is the mechanism that stops an agent's first idea from silently becoming architecture. A suspected cause written as a plain assertion becomes the root cause within a week, and everyone builds on it.

## Standing rules

**Never encode in text what GitHub stores as data.** Parent, sub-issue, blocked by, blocking, assignee, and state are structure. Text describing them goes stale independently of the truth.

**Link what you assert, name what you suggest.** A permalink for evidence, a plain path for a starting point. Never a list of every file the work might touch.

**The body is the current work order, not a log.** It holds the objective, constraints, and completion conditions. Comments hold dated findings. The pull request holds what changed.

**One planning system.** Linear owns priority, scheduling, and progress. No GitHub Projects, no milestones, no priority labels.

**Enough to begin, enough to know when done, nothing pre-solved.**

**A concise unknown beats a plausible fabrication.** This applies to root cause, impact, affected versions, reproduction rate, severity, baselines, and completion alike.

## Where the detail lives

Read the one file that answers the question in front of you.

| Question | File |
| --- | --- |
| Which archetype is this, and what does it need? | `references/archetypes.md` |
| What should the title and body look like? | `references/writing.md` |
| Which type, label, or field, if any? | `references/metadata.md` |
| What does good look like? | `references/examples.md` |
| Linear, GitHub, pull request, or ADR? | `references/boundary.md` |

Two neighbouring skills own their subjects and are not repeated here. Use `writing-adrs` when a decision is durable and architectural, and `mermaid-diagrams` when a diagram has earned its place.

## Diagrams

GitHub renders diagrams from fences tagged `mermaid`, `geojson`, `topojson`, and `stl`.

A diagram earns its place when it shows a sequence, topology, dependency, state machine, migration, or data flow materially faster than prose would. Keep it small, give it an explicit direction, and do not restate it in a paragraph underneath. Most issues need no diagram, and a five-line explanation usually wins.

## Security

Never open a public issue for an exploitable vulnerability. Use private vulnerability reporting or a draft security advisory.

In a private repository an issue is acceptable, but write it so the body states the vulnerable behaviour and its impact without being a working recipe. Keep proof-of-concept payloads and exact exploitation sequences out of the body.

## Judging an issue

- The title states the technical objective or the observed problem, and stands alone.
- Every hypothesis, suggestion, and unknown is marked, so nothing speculative reads as a requirement.
- Completion conditions are observable, and constraints are real constraints rather than preferences.
- Code references are starting points or evidence, and they still exist.

## Maintenance

`references/sources.md` records which conventions come from documented GitHub behaviour, which come from research, and which are ours, with the date each was last checked. It is for maintainers, not for normal use.
