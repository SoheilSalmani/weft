---
name: architectural-decisions
description: Decides whether an architectural choice deserves a decision record, finds the accepted records that already govern a change, and handles conflicts between requested work and a recorded decision. Use when making or changing an architectural choice, when work may contradict an existing decision, or when asked whether something needs an ADR. To write or supersede the record itself, use writing-adrs.
---

# Architectural decisions

Most architectural choices need no record. This skill exists to answer that quickly, and to make sure the ones that do get written are found before they are contradicted.

The decision log lives in `docs/adr/`. `writing-adrs` writes the records; this skill decides whether one is warranted, which existing ones apply, and what to do when work and a record disagree.

## The gate

> **An ADR exists when a competent engineer, reading only the code, would reasonably conclude that a different choice was available and better.**

That is the test. Everything below refines it.

Five questions. **Two or more yes, and a record is probably warranted:**

1. Would a competent engineer reading only the code reasonably conclude a different choice was available and better?
2. Will future changes be constrained by this, rather than merely influenced?
3. Is reversal costly, risky, or coordinated across components?
4. Does it cross a boundary: module, package, service, repository, or trust?
5. Could an agent violate it while making a locally sensible change?

**Question 5 is a standalone override.** If yes, record it even when nothing else applies. That is the case this practice exists to serve in a workflow where agents make changes, and it is the one no external source addresses.

**Creating no record is a valid and usually preferable result.** Say so plainly, with the reason, and route the work where it belongs. A log of thirty meaningful decisions is worth more than four hundred nobody reads.

Before concluding either way, apply the compression test: state the decision as one sentence.

> In the context of *X*, facing *Y*, we decided for *Z* and neglected *A*, to achieve *B*, accepting that *C*.

If *Y* cannot be filled without inventing a force, there is no decision, only a preference. If *A* cannot be filled honestly, the choice was forced by a constraint, which is worth saying rather than manufacturing alternatives. If it takes two sentences with different subjects, it is two decisions.

Worked positive and negative cases are in `references/judgement-calls.md`. Read it when the call is not obvious.

## Search before proposing anything

Never propose a record without checking what already exists.

```bash
cat docs/adr/README.md                    # index: titles state decisions, so this often answers it
ls docs/adr/                              # what exists, and which numbers are taken
grep -ril "<subsystem or concept>" docs/adr/
```

Then read only the matching records, and check the status line of each before relying on it. Do not load the whole corpus; with a log this size, the index plus one grep is precise enough, and reading everything wastes the context the actual work needs.

**Grep fails on vocabulary mismatch, and that is the retrieval failure that matters.** A record titled `Preserve single-writer ownership of financial state` will not match a task about "invoice updates". Two cheap fallbacks, in order: read every title in the index, which costs almost nothing at this corpus size and catches most mismatches, and grep for the **module or package path** the work touches rather than the domain word, since records name components. Only if retrieval still fails repeatedly is scope metadata worth adding, and then only to the records that were missed. Do not tag everything pre-emptively against a problem that has not occurred.

Ask, in order:

- Is this already decided? Then link the record and stop.
- Is an existing record broader than this? Usually this is implementation under it. Not always: a narrower record earns its place when it adds a constraint the general rule does not imply, such as a general ownership rule that does not by itself say which service owns billing. Say which constraint is new, or do not write it.
- Would this amend or supersede an existing record? Then see `references/lifecycle.md`.
- Is this really an implementation detail? Then it belongs in the pull request.
- Is this still an unresolved investigation? Then it is a GitHub issue. **This is the most common false positive.** A `Proposed` record is warranted only once there is a recommendation to review; until then the question is an issue, and a record drafted early sits at `Proposed` forever.
- Is this a product decision? Then it is Linear.

Do not propose a record because someone used the word "architecture".

## Routing

The distinctions that matter here:

| Situation | Home |
| --- | --- |
| An architectural question nobody has answered | GitHub issue |
| The answer, once decided | ADR |
| How to build what was decided | GitHub issue, then pull requests |
| Why this code is written this way | Pull request or a code comment |
| Why the system is shaped this way | ADR |
| What we are building and why it matters to users | Linear |

An architectural question becomes a record at the moment it is answered, and not before.

## When work appears to conflict with a record

**Apparent inconsistency is not a violation.** At least six explanations exist, and an agent asserting a violation without checking them is the failure mode this section prevents: the code predates the record, migration is incomplete, the scope differs, the record was superseded, the implementation is wrong, or the record is wrong.

1. **Check the status.** Proposed and superseded records bind nothing.
2. **Check the scope.** Most apparent conflicts are scope mismatches.
3. **Read the code**, and its history. `git log` on the path usually settles whether the code predates the decision.
4. **State the evidence**, and label the conclusion: apparent inconsistency, or confirmed violation naming the specific clause and the specific code.
5. **Never silently change code or a record to remove the discrepancy.** Surface it.

Possible outcomes, all legitimate: an implementation bug, an incomplete migration, an intentional exception, a record that needs superseding, or a record that does not apply.

## When requested work contradicts an accepted decision

Surface it before implementing the contrary architecture. Silently building against a recorded decision is worse than either conforming or changing the decision, because it leaves the log describing a system that no longer exists.

State concisely: which record, what it decided, and why the requested work appears inconsistent. Then take one of four routes:

- **Conform.** Usually correct. Most conflicts are the work being inconvenient under the decision, not the forces having changed.
- **Exception.** Legitimate when narrow and deliberate. It belongs in the pull request, not as a silent deviation.
- **New record.** When the forces genuinely changed. Draft it as `Proposed` through `writing-adrs`.
- **Supersede.** When the old decision is simply wrong now. See `references/lifecycle.md`.

An ADR is not law, and it is not a suggestion. Changing one is a deliberate act with a record of its own.

## Implementation is not decision state

An accepted record does not mean the architecture exists. Status describes the decision, never the rollout. Implementation work goes to GitHub issues through `writing-github-issues`, and a record never becomes a progress tracker.

## Accuracy

Never invent historical rationale, alternatives that were never considered, benchmarks, scale or performance requirements, security requirements, agreement, adoption state, or a decision date.

Two specific fabrications to refuse by name. **Never write a decision nobody made**: "we need to improve resilience" is a goal, and turning it into an accepted record naming a technology invents the decision itself. Gather, research, and propose, clearly labelled. **Never write "the team decided"** when one person proposed. Attribute what actually happened, and the decision-makers field says who, if anyone, agreed. Where the reasoning behind existing architecture was never recorded, say that it is unknown. A reconstructed rationale cannot later be told apart from evidence, which is why `.agents/skills/writing-adrs/references/baseline-records.md` forbids it.

Label every claim: observed, documented, inferred, proposed, or unknown. A record must never sound more authoritative than the evidence behind it.

## Mutation safety

| Act | Default |
| --- | --- |
| Read records, search, judge whether one is warranted | Free |
| Draft a record in the conversation | Free |
| Create a file in `docs/adr/` | Ask |
| Change a status line | **Never.** Accepting or rejecting is a human act |
| Edit an accepted record | Never, beyond the corrections in `references/lifecycle.md` |
| Claim a number from the reservation table | Ask |

An agent may leave a record only as `Proposed`.

## Before you finish

- The gate was applied, and "no record needed" was considered seriously.
- Existing records were searched before anything new was proposed.
- Any conflict is labelled apparent or confirmed, with evidence.
- Nothing was accepted, superseded, or edited without a human.
- Implementation work was routed to issues, not tracked in a record.
