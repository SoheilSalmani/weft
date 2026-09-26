---
name: writing-adrs
description: Write, review, or supersede an architecture decision record. Use when the user asks to record an architecture decision, draft or revise an ADR, or set up a decision log.
---

# Writing architecture decision records

An ADR records one decision. It states what made the decision necessary, what was decided, and what follows from it.

It is a historical record, not a description of how the system works. A reader opens an ADR to learn why the system is the way it is. Architecture documentation, API references, and onboarding guides answer different questions and an ADR does not replace any of them.

## When to write one

Write an ADR when a choice is architecturally significant and costly to reverse. In practice that means it affects one of these:

- The structure of the system, such as its components and how they are separated.
- A cross-cutting quality, such as security, availability, performance, or cost.
- Dependency coupling, including what the system now depends on and cannot easily stop depending on.
- An interface or published contract.
- A construction technique, such as a framework, protocol, or persistence approach.

Also write one when a later reader would otherwise assume the opposite decision was made. Recording that an obvious option was considered and declined prevents the same argument from being repeated a year later by someone who does not know it already happened.

## When not to write one

Do not write an ADR for a bug fix, a dependency bump, or anything the change itself already explains. Do not write one to describe how a component behaves today. Do not write one to record a plan, a task list, or a coding convention.

If the document would still be correct with the decision removed, it is not a decision record.

## Baseline records

A log adopted for a system that already exists has no starting point. When the log is new and the system is not, write one baseline record titled `ADR-0000: Architecture Baseline` before any decision record. Normal records begin at `ADR-0001`.

It is not a decision record. It describes structure and explains nothing, because the reasoning behind an existing system is usually not written down and reconstructing it would be invention. It may be cited as evidence of what the architecture was, never as evidence that any of it was decided.

It is deliberately far more descriptive than a normal record, since a future reader must be able to understand the starting architecture without reconstructing it from the repository.

Read `references/baseline-records.md` before writing one.

## Structure

Use the project's own template when one exists. Otherwise use `references/template.md`.

Every record needs these parts:

- **Title.** A short phrase naming the decision itself, not the topic. "Keep the database schema in a single source" tells a reader the outcome. "Database schema" does not.
- **Metadata.** Status, date, and the decision-makers.
- **Context.** The situation that forces a choice, stated as facts. Include the constraints that bound the options and the things that are not known.
- **Options considered.** Include this when more than one option was genuinely available. Keep options at the same level of abstraction. Comparing a protocol to a product means the comparison is not real.
- **Decision.** What was decided, in active voice.
- **Consequences.** What becomes true as a result, including the costs.

## Writing rules

The structure above decides what a record contains. These decide whether it can be read:

- **Write for a reader who has only this document.** Assume no access to the issue tracker, the chat history, the person who wrote it, or the version of the code that existed at the time. A record that depends on any of those stops working exactly when it is needed most.
- **State facts, not motives.** The context section is value-neutral. Describe what is observably true. Do not guess why an earlier choice was made, do not assign blame, and do not praise. If the reasoning behind something is not recorded anywhere, write that it is unknown. Inventing a plausible reason is worse than admitting the gap, because the invention cannot be distinguished from evidence later.
- **Name the decision-makers.** Record who made the decision in the metadata. A reader who wants to challenge a decision needs to know whom to ask, and a decision with a name attached is harder to quietly overturn by someone who was not part of it. Names belong in metadata only. Do not attribute intentions to people in the body.
- **Say nothing about staffing or authorship.** Team size, management approach, and how the code was produced are not architectural facts. They do not affect whether a decision was right, and they date the document.
- **Do not reference issue trackers.** Ticket identifiers are meaningless to a reader without access, and they tie a permanent record to a system that will be migrated or retired.
- **Name architectural elements, not files.** Refer to components by what they are, such as the payment gateway or the job scheduler. File paths, line numbers, and code snippets are the most perishable thing you can put in a permanent document, and they push the record toward describing implementation instead of reasoning. When a fact genuinely needs pinning down, pin it to a version or a date, since the record is a statement about a moment in time.
- **State the decision in active voice.** Write "The system will store uploaded files in object storage" rather than "It was decided that uploaded files should be stored in object storage".
- **Record every consequence, including the ones you dislike.** A decision with only benefits is a sales pitch and a later reader will not trust it. Consequences may be positive, negative, or neutral, and the negative ones are the reason the record has value.
- **Write full sentences.** Bullets are for visual rhythm, not an excuse for fragments. A list of noun phrases carries none of the reasoning that makes a record worth keeping.
- **Use plain words.** Cut adjectives and adverbs that carry no technical meaning. "Significantly improves performance" says less than "reduces cold start from four seconds to under one".
- **Match length to the decision.** Most records fit on one page. A record that grows past that is usually several decisions that should be separated.
- **Do not use em dashes.** Use a comma, a full stop, or brackets.

`references/example.md` shows one decision written badly and then correctly, with the rule behind each correction. Read it when a draft is hard to phrase, or when a review turns up more than one of the failures below.

When a record carries a diagram, read `references/diagrams-in-records.md` first. It covers when a diagram is justified and what it may contain, and the same rules above apply to it.

## Anti-patterns

Check a draft against these before finishing, each described with a detection test in `references/anti-patterns.md`:

| Name | What it looks like |
| --- | --- |
| Fairy Tale | Only benefits, no costs. |
| Sales Pitch | Promotional language and claims with nothing behind them. |
| Free Lunch | Long-term or operational consequences left out. |
| Dummy Alternative | Options included only to make the chosen one look better. |
| Mega-ADR | Design detail, diagrams, and code that belong in architecture documentation. |
| Novel | One decision inflated into a full architecture document. |
| Blueprint | A commanding, policy-like tone instead of a record of a choice. |
| Maze | Discussion that drifts into detail irrelevant to the decision. |
| Tunnel Vision | Only the developer view, ignoring operations, maintenance, and users. |

## Status and immutability

A record moves from `Proposed` to either `Accepted` or `Rejected`. An accepted record can later become `Superseded by NNNN`.

Once accepted or rejected, a record is immutable. To change a decision, write a new record and mark the old one superseded. Never edit or delete an accepted record, because the history of a reversed decision is often more useful than the current answer alone.

A rejected record stays in the log. It is what stops the same option being proposed again without new information.

Numbers are permanent and are never reused, including for rejected records. Gaps in the sequence are expected.

Nothing should be built on a record that is still `Proposed`.

## Approval

Drafting a record and deciding it are separate acts. An agent may gather facts, set out the options, state the consequences of each, and give a recommendation that is clearly labelled as one. Moving a record to `Accepted` or `Rejected` requires a human, and the decision-makers recorded in the metadata are the people who did it.

## Before you finish

Check each of these against the draft:

- The title states the decision, not the topic.
- One decision only. If the title needs "and", split it.
- Status, date, and decision-makers are filled in.
- Context contains facts, and anything unknown is marked unknown.
- Options, if listed, are real and at the same level of abstraction.
- Consequences include costs, not only benefits.
- No tracker identifiers, file paths, line numbers, or code.
- No staffing details, and no claims about who wrote the code.
- No em dashes.
- Sentences are complete.
- It fits on roughly one page.
- If it is a baseline record, it explains nothing. Any sentence answering "why" means it has become a decision record with no author.

## Sources

The rules above come from these references rather than from convention:

- Michael Nygard, "Documenting Architecture Decisions" (2011), the origin of the format.
- MADR, the Markdown Any Decision Records project at `adr.github.io/madr`.
- AWS Prescriptive Guidance on architectural decision records, for the lifecycle and immutability model.
- Olaf Zimmermann, "How to create Architectural Decision Records, and how not to", for the anti-pattern catalogue.
