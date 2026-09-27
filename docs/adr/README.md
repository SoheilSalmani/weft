# Architecture decision records

Why the system is shaped the way it is. Each record states one decision, what forced it, and what follows from it. A record is a historical document: once accepted or rejected it is never edited, and a decision is changed by writing a new record that supersedes the old one.

Records are numbered permanently. Numbers are never reused, including for rejected records, so gaps in the sequence are expected.

| Record | Status | Decides |
| --- | --- | --- |
| [ADR-0000](0000-architecture-baseline.md): Architecture Baseline | Not a decision | The state of the system on 2026-08-28, when this log was adopted |
| [ADR-0001](0001-project-side-agent-guide.md): Write the project-side agent guide into the state directory | Rejected | Rejected: every option assumed per-project content, which cannot reach an agent uniformly |

ADR-0000 is a baseline rather than a decision. It describes the starting architecture so later records have a historical reference point, and it may be cited as evidence of what existed on that date, never as evidence that any of it was chosen deliberately. Decision records begin at ADR-0001.

## Adding a record

Allocate the next unused number and copy the template from the `writing-adrs` skill. Fill in the status, the date, and the decision-makers. An agent may draft a record and recommend an option, but only a person moves one to `Accepted` or `Rejected`.

Nothing should be built on a record that is still `Proposed`.
