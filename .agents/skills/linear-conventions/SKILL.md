---
name: linear-conventions
description: House conventions for Linear, the issue tracker. Covers which Linear object to use, how to name and write it, what belongs in a GitHub issue, a pull request, or an ADR instead, and what an agent may never invent. Use when reading, writing, naming, restructuring, or judging anything in a Linear workspace.
---

# Linear conventions

Linear holds the change we intend to make in the world, and where that change stands. The repository holds how it is made.

Trackers fail in two directions. They become a second, worse copy of the codebase, full of paths and snippets that are wrong within a week. Or they become status theatre, where every object is up to date and none of them say anything. Both failures come from writing without deciding who the reader is.

The target: someone who has been away for two weeks understands the state of work in under a minute, and an agent that retrieves the same objects with a truncated description gets the same understanding. These are the same problem. Both are solved by putting the most important sentence first and keeping the structure shallow.

## Reader altitudes

Every object has one reader and answers one question. Most "where does this go" arguments end here.

| Object | Reader | The question it answers |
| --- | --- | --- |
| Initiative | Whoever decides what gets worked on | Why does this cluster of projects matter, and are we winning? |
| Project | Anyone outside the work | What are we changing, and is it on track? |
| Milestone | The same reader as the project | What is true now that was not true before? |
| Issue | Whoever does it, and anyone auditing scope | What single outcome is owed, and how do we know it is done? |
| Sub-issue | People splitting one outcome | Which slice is mine? |
| Comment | Participants, and the reader asking "why did this change?" | What happened, was asked, or was decided at a point in time |
| Document | Someone who needs depth the project cannot hold | The detail behind one thing, kept current |
| Update | Someone not in the work | What changed since last time, and what does it mean? |
| View | Anyone with a recurring question | Which things match this condition right now? |
| Label | Whoever builds a view or automation | Which bucket is this in? |

## The rules that decide most cases

**One outcome per object.** If the title needs "and", it is two objects.

**The description is current truth. Comments are history.** A description is not a log. When an assumption dies, remove it and leave a comment saying it changed. Contradictory text left in place for historical completeness makes the object unreadable, and Linear already records what changed and when.

**Never write what a property encodes.** Team, status, priority, project, milestone, cycle, assignee, and dates are structured fields. Linear reserves label names such as `priority`, `status`, `estimate`, and `cycle` precisely because duplicating them is a known failure.

**Never write what Linear renders.** Progress percentages, issue counts, pull request state, CI results, and property history are displayed already. A project update in particular is attached to an automatically generated progress diff, so restating it wastes the only part a reader must think about.

**Unknown is a value.** An explicit "unknown, and here is what would settle it" beats a plausible number. An invented fact is indistinguishable from a real one within a week.

**Front-load.** The first sentence carries the conclusion, does not repeat the title, and stands alone. Tools that list issues return roughly the first 300 characters of a description, so for an agent that first sentence often is the description.

**Structure appears at length.** Under roughly 150 words, prose beats headings. A heading with one sentence under it is noise, and an empty heading is worse than a missing one. Never add `Context`, `Overview`, `Summary`, `Next steps`, or `Acceptance criteria` because a template had them.

**Every label names a view.** If you cannot say which saved view or automation a label feeds, it should not exist.

**Preserve on rewrite.** Read the current state first, keep anything not provably obsolete, relocate rather than delete, and say what moved where.

**Route technical depth out.** A line that would be false after the pull request merges does not belong in a Linear description.

## Where the detail lives

Read the one file that answers the question in front of you. Do not read them all.

| Question | File |
| --- | --- |
| Which object should this be? | `references/objects.md` |
| Is this title any good, and what should it be? | `references/naming.md` |
| How much structure, and in what voice? | `references/writing.md` |
| Does this belong in Linear, GitHub, a pull request, or an ADR? | `references/boundary.md` |
| Should this label exist? | `references/labels.md` |
| What does good look like? | `references/examples.md` |

Two related skills own their own subjects and are not repeated here. Use `writing-adrs` when a decision is durable and architectural, and `mermaid-diagrams` when a diagram has earned its place.

## Diagrams

Linear renders Mermaid natively, inside a fenced block tagged `mermaid` or through the `/diagram` command. So the question is never "can we", only "should we".

A diagram earns its place when it shows a relationship, sequence, state machine, dependency, or scope boundary that prose would take more than three sentences to convey and the reader would still have to assemble mentally. Escalate in this order: a sentence, bullets, a table, Mermaid, an external tool linked as a project resource.

Diagrams in issue descriptions are usually a smell, since one outcome rarely needs a graph. Diagrams in project descriptions are often the most valuable content there. Architecture diagrams belong in ADRs and repository documentation.

## Judging an object

- The first sentence says what changes and for whom, without repeating the title.
- Nothing duplicates a property, or something Linear renders.
- Every number has a source and a date, every assumption is marked as one, and every unknown is stated rather than quietly filled.
- Nothing in it would become false when the pull request merges.

## Maintenance

`references/sources.md` records which conventions come from documented Linear behaviour, which come from research, and which are ours, with the date each was last checked. It is for maintainers. Do not load it during normal work.
