# Shapes for common work

Patterns, not templates. Take the parts that carry information for this specific object and leave the rest. Worked before-and-after pairs are in `.agents/skills/linear-conventions/references/examples.md`.

## Contents

- Issues
- Parent and children
- Projects
- Milestones
- Initiatives
- Documents

## Issues

| Kind | Shape | Usually omit |
| --- | --- | --- |
| Small change | Title, then one or two sentences saying what is wrong and what will be true instead | Everything else |
| Bug | Observed, expected, where and when it was seen, one line of evidence in a collapsible | Cause, fix, and priority language |
| Feature or engineering change | Outcome sentence, why now with its evidence, then only what would surprise someone: non-goals, or how we will know it works | Restated context, acceptance criteria that repeat the title |
| Delegated to an agent | Outcome sentence, where to start, the existing pattern to reuse, and what must not change | Product background the agent cannot act on |
| Spike | The question as a question, the decision it unblocks, a timebox, and what it produces | Speculation about the answer |
| Chore or operational task | One sentence, and the trigger if it is recurring | A justification nobody asked for |

Take only the parts that carry information. When the expected behaviour is obvious from the observed one, stating it is ceremony: "clicking Retry sometimes submits the payment twice" does not need "expected: one payment". When you do not know where it was seen, say that in the same sentence rather than leaving a heading with nothing under it.

A bug title is declarative. Every other issue title is imperative.

## Parent and children

Use a parent when one outcome has to split across people, teams, or pull requests. Not for a checklist you will work through yourself.

The parent carries the outcome and the context. Each child carries its own slice and reads sensibly in a list without its parent's title. Children inherit team, priority, and project from the parent, but not labels, so set labels explicitly if they matter.

Propose the split before creating it. A tree created without approval is tedious to undo.

## Projects

What belongs in the summary and the description is in `.agents/skills/linear-conventions/references/writing.md`. This is the order that avoids rework:

1. **The description**, so you find out what the project actually is.
2. **The summary**, written last, once you know.
3. **Dates at honest resolution.** Linear accepts a year, half, quarter, month, or day, so use the coarsest one that is true rather than a precise date nobody believes.
4. **A lead**, only if someone has actually taken it.

## Milestones

The decision here is whether any should exist. Create them only when the project passes through states a stakeholder would notice, which a project of three issues does not.

A target date on a milestone is optional, and a wrong one is worse than none.

## Initiatives

Only when two or more projects share an outcome and someone makes tradeoffs between them. One project is a project. A filterable grouping is a view.

Two to four short paragraphs: what changes at the company level, why now, what it will not do, how we will know it worked. The projects underneath are visible, so do not list them.

## Documents

A document is justified when depth outlives an issue and more than one issue would link to it: a specification, a research write-up, a contract under design, a runbook.

It is not the place for a decision, which is an ADR, or for code detail, which is the repository. If you cannot name who opens it and why, do not create it.
