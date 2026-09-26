# Judgement calls

Worked cases. The pattern across the negatives is that the choice is **visible, local, cheap to reverse, or not yet made**. Any one of those is usually disqualifying.

## Contents

- No record needed
- A record is warranted
- The hard middle
- Cases that look architectural and are not

## No record needed

| Candidate | Why not |
| --- | --- |
| Rename a class | Visible, local, free to reverse |
| Extract a helper | Nobody reading the code concludes a different structure was obviously better |
| A private method signature | Invisible outside its file |
| A loop rather than a map | Style, and the diff shows it |
| Move files | Unless the move establishes a boundary that must hold, in which case the boundary is the decision, not the move |
| An index for an obvious query | Unless it encodes a durable access-pattern commitment nothing else records |
| Implementing an already-decided architecture | The decision exists. This is execution, and it belongs in an issue |
| A library for one local utility | Unless others must adopt it, which makes it a dependency decision |
| A one-off bug fix | |
| A feature flag name | |
| Test organisation | Unless it constrains how every future test must be written |
| An ordinary pull request choice | The pull request body is the record |

## A record is warranted

| Candidate | Which questions it triggers |
| --- | --- |
| Which database is authoritative for a domain | Structure, reversal cost, boundary. A later engineer would reasonably ask why not the other one |
| A service or package boundary | Boundary, constraint on future changes |
| Publishing domain events through a transactional outbox | Constraint, non-obviousness. The direct publish looks correct until you know why it is not |
| At-least-once versus exactly-once delivery | Every consumer is constrained by the answer |
| An API compatibility policy | Cross-cutting constraint on all future changes |
| Where authentication is validated | Boundary and trust model, and an agent could easily move it while doing something sensible |
| Retaining an existing architecture deliberately | The decision most likely to be silently reversed by a plausible refactor |

## The hard middle

**The same technology, two different answers.** This pair is the test of whether the gate is being applied or a keyword is being matched:

> "Use PostgreSQL advisory locks inside this maintenance script." **No record.** One script, visible, deletable tomorrow. Nothing else is constrained.
>
> "All distributed job coordination uses PostgreSQL leases instead of process-local locks." **Record.** It binds every future worker, crosses component boundaries, and an agent adding a job with a process-local lock would be making a locally sensible change that violates it.

Nothing about the word "PostgreSQL" decided either one. Scope and blast radius did.

**A dependency that starts local and spreads.** A parsing library used in one helper is not a decision. The same library, once three packages import it and a fourth is expected to, is a dependency decision. Record it when adoption becomes an expectation, not when it becomes common.

**A temporary architecture.** Polling because a provider has no webhooks is temporary and still architectural, because everything built during that window is shaped by it. Record it with a review trigger, never an expiry date. See `lifecycle.md`.

**A performance constraint.** A benchmark result is evidence and belongs in the issue or pull request. A commitment such as "this path must stay under 50ms because the caller times out at 100" is a durable constraint and belongs in a record, with the benchmark linked rather than pasted.

**A decision forced by a constraint.** If a provider's API leaves exactly one workable approach, that is still worth recording, because the next engineer will otherwise propose the approach that does not work. Say the constraint forced it. Do not manufacture alternatives to look rigorous.

**A migration.** Three artefacts, not one. The decision to migrate is a record. The plan is an issue. Each phase is a pull request. Writing the plan into the record turns it into a tracker.

## Cases that look architectural and are not

**An unresolved investigation.** "Should we move to event sourcing?" is a question. It becomes a record when it is answered, and a GitHub issue until then. Proposing a record for an open question is the most common false positive, and it produces documents that sit at `Proposed` forever.

**A product decision.** "We will not support self-serve downgrades" is Linear. It may later force an architectural decision, and that one gets a record.

**A project priority.** "Billing work comes before search" is Linear.

**A coding convention.** "We use named exports" belongs in `CLAUDE.md` or a linter, both of which enforce it. A record cannot.

**Something the code already makes obvious.** If a reader sees the reason immediately on reading the code, the record adds nothing. The test is whether they would conclude a *different* choice was better, not whether they would find the current one interesting.

## When two people would disagree

Prefer no record, and say why. A missing record for a genuine decision costs one conversation later. A log full of marginal records costs every future reader the effort of deciding which ones matter, and that is how these collections stop being read at all.
