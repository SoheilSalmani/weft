# Lifecycle

How a decision changes, and what may be edited once it is in force. `writing-adrs` writes the prose; this file governs the states and relationships.

## Contents

- The four statuses
- Edits permitted after acceptance
- Superseding a decision
- Partial supersession
- Rejected records
- Temporary decisions
- Decisions under uncertainty
- Implementation state
- Provenance links

## The four statuses

`Proposed`, `Accepted`, `Rejected`, `Superseded by NNNN`. These four are sufficient. Do not add more, and where a decision log already uses them, do not renegotiate them.

There is deliberately no `Deprecated`, and this is **our convention rather than the source practice**. Nygard's original names both: "If a later ADR changes or reverses a decision, it may be marked as 'deprecated' or 'superseded' with a reference to its replacement." He does not distinguish them, which is the reason to collapse them. A decision that no longer applies has either been replaced by another decision, which is supersession, or was scoped to something that no longer exists, which the scope line already says. Two words for one state produce records that disagree about which to use.

**Status is decision state.** It never describes how much of the architecture has been built.

An agent may leave a record only as `Proposed`. Moving one to `Accepted` or `Rejected` is a human editing the status line.

## Edits permitted after acceptance

An accepted record is immutable in substance. This is the documented consensus, and the reason is that a reversed decision's history is usually more useful than the current answer alone.

Permitted: spelling and grammar, formatting, repairing a broken link, the status line, and adding a relationship pointer.

Not permitted: context, decision, consequences, or alternatives. If the reasoning was wrong, that is a new record. If the record is actively misleading, a new record supersedes it and says what was wrong.

Never edit an old record to make it look as though the previous decision was never taken.

## Superseding a decision

1. Draft a new record as `Proposed`, stating what changed in the forces. Not what changed in anyone's preference.
2. The new record names the one it replaces.
3. A human accepts it.
4. The old record's status becomes `Superseded by NNNN`. That is the only edit it receives.

The new record must stand on its own. A reader should not need the superseded one to understand the current architecture, though it stays available to explain how we got here.

## Partial supersession

The common case, and the one that goes wrong. A new decision usually changes part of an older one rather than invalidating all of it.

Do not mark the older record superseded when most of it still governs. Instead:

- The **new** record states exactly which clause of the older decision it replaces, and what remains in force.
- The **older** record keeps `Accepted` and gains a front-matter pointer, `amended-by: NNNN`.

Status stays a four-value field describing whether the decision is in force, which it still is. The relationship is metadata. This keeps the older record binding for everything it still covers, which is what actually happens.

**Partial supersession is usually evidence the original record bundled two decisions.** A record deciding both caching and distributed locking will one day need only one of them changed. When you find yourself amending, check whether the older record should have been two, and say so in the new one. That is a corpus-hygiene signal worth acting on, not a filing problem.

**A decision log's template will usually not describe `amended-by`.** Check before relying on it. Where the template does not carry the field, state the amendment in the new record's prose as well, so nothing depends on metadata a reader has no way to interpret.

## Rejected records

Keep one when the option was seriously considered and the reasoning is likely to recur. The purpose is to stop the same proposal returning without new information, so a rejected record must say *why*, not merely that it was declined.

Do not create one for every discarded idea. The test: would someone plausibly propose this again within a year, believing it obviously right?

A draft nobody pursued is **deleted, not rejected**, and its number returns to the reservation table.

## Temporary decisions

Some temporary architecture is significant, because everything built during the window is shaped by it.

Record it with a **review trigger, not an expiry date**:

> Revisit when the provider ships webhook delivery. Nothing else changes the forces.

A date is a guess that arrives stale and gets ignored. A condition is checkable. Never add either mechanically, and never add both.

## Decisions under uncertainty

An experiment is not a decision. "We will try X and see" is a GitHub issue.

A decision taken under uncertainty **is** a decision, and the record should say so plainly rather than projecting confidence it does not have:

> Evidence is one prototype run. This option was chosen partly because it is reversible in a day, which the alternative is not.

That tells the next engineer exactly what new evidence would justify revisiting, which a confident record does not.

Do not use a record to give an experiment false permanence.

## Implementation state

An accepted record does not mean the architecture exists. Do not track progress in the record, in its status, or in a checklist inside it. Implementation work belongs in GitHub issues.

One exception, because it is a genuine consequence the reader inherits: a **known deviation that exists at acceptance time** belongs in Consequences.

> The existing skill-engine path does not follow this and is not scheduled to change. Anything new does.

That is a fact about what you are accepting, not a progress report.

## Decisions spanning repositories

A decision governing several repositories lives in **one authoritative record**, in the repository that owns the constrained system, and the others reference it. Never copy the record, because copies diverge silently and no reader can tell which is current.

There is one repository today, so this is a rule stated before it is needed rather than a practice in use.

## Provenance links

A record must be comprehensible with no link followed. That rule is absolute, and it is why `writing-adrs` keeps tracker identifiers out of the body.

Provenance is different from explanation. It may go in **front matter**, where it costs a reader nothing and survives as an archaeological hint:

```yaml
source: ENG-14
```

What may be linked: the originating Linear item or GitHub issue, evidence such as a benchmark or incident, the superseding or superseded record, and at most one implementation pull request where it genuinely helps.

Never list every implementing pull request. Git history already connects them and the list is stale after the second one.

**This extends an accepted convention** that currently keeps tracker references out of records entirely. The body prohibition stands unchanged. Front-matter provenance needs approval before `writing-adrs` is updated to match.
