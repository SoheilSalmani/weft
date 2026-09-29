# ADR template

Copy everything below the line into `NNNN-short-title-with-dashes.md`. Replace the guidance in brackets and delete anything the decision does not need. Use the project's own template instead of this one when it has one:

---

```markdown
---
status: Proposed
date: YYYY-MM-DD
decision-makers: [names of the people who made or will make this decision]
---

# ADR-NNNN: [The decision, stated as a short phrase]

## Context

[What is true that forces a choice. Facts only, in full sentences.

State the constraints that bound the options.

State what is not known. An unresolved question that would change the decision belongs here, not in someone's memory.]

## Options considered

[Include this section only when more than one option was genuinely available. Keep options at the same level of abstraction.]

### A. [Option name]

[What it involves, and what it costs.]

### B. [Option name]

[What it involves, and what it costs.]

## Decision

[What was decided, in active voice: "The system will ...".

Say why this option and not the others. If the record is still Proposed, write the recommendation here and label it as a recommendation.]

## Consequences

[What becomes true as a result.

Include the costs. A record listing only benefits will not be trusted later.

Note anything that becomes harder, anything now foreclosed, and any new obligation this creates.]
```

---

## Notes

Four things the template does not show:

- **Numbering.** Allocate the next unused number when you create the file. Numbers are permanent and are never reused, including for rejected records.
- **Status.** `Proposed`, `Accepted`, `Rejected`, or `Superseded by NNNN`. Once accepted or rejected the record is immutable. To change a decision, write a new record and mark the old one superseded.
- **Decision-makers.** Always filled in. This is who to ask when a reader disagrees with the record.
- **Optional metadata.** `consulted` and `informed` are worth adding when a decision reaches beyond the people who made it. `supersedes` and `superseded-by` make the chain between records navigable.

