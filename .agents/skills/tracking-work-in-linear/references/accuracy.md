# Accuracy

An invented fact in a tracker is indistinguishable from a real one within a week, and it will be acted on. This is the one area where being unhelpful is cheaper than being wrong.

## Contents

- Four epistemic states
- Never invent
- Unknown is not the same as not applicable
- Estimates people give you
- Asked for, versus decided
- How to write an unknown
- Preserving information when rewriting

## Four epistemic states

Every statement is one of these, and the writing should make clear which.

| State | Means | How it reads |
| --- | --- | --- |
| Known | Read from a system, or stated by a person | Plain assertion: "The API returns 400 for accounts with two providers." |
| Inferred | Worked out from evidence, and the evidence is shown | "The two failing tenants both have two providers configured, so the trigger is probably the ambiguous binding." |
| Proposed | A recommendation awaiting a decision | "Proposed: drop private components from the first release." |
| Unknown | Not established | "Unknown: whether authors need private components. Nobody has asked." |

Never promote a statement up this table to make a sentence read better. An inference written as a fact is the most common way a tracker becomes untrustworthy, because the evidence that would let a reader check it is gone.

## Never invent

| Field | Rule |
| --- | --- |
| Owner or lead | Never assign. Leave unassigned and say it is unassigned |
| Status | Never move an object to a status that asserts work you did not observe. Finding a merged pull request is observation. Assuming one exists is not |
| Priority | Never set or change it. Priority is a human tradeoff between things you cannot see |
| Dates | Only a date that exists in a system or was stated by a person. Never estimate a completion date. When a date is needed but unknown, use a coarser resolution such as a quarter rather than inventing a day |
| Scope | Never silently widen or narrow. Propose scope changes and let someone accept them |
| Metrics | Every number carries its source and the date it was measured. No number without a measurement |
| Customer or user impact | Only from a customer request, a quoted message, or a stated fact. Never inferred from the nature of the bug. "Users are frustrated" is invention unless a user said so |
| Decisions | Only record a decision you can attribute to a person and a date. Your own recommendation is labelled as a recommendation |
| Dependencies | Only create a relation when the ordering is real. Two things concerning the same area are not related in the Linear sense |
| Blockers | A blocker is something that stops work, named specifically. "Might be blocked by the schema work" is not a blocker, it is a risk |
| Completion | Never claim work is done from a plan, a branch name, or a draft pull request |

## Unknown is not the same as not applicable

Leaving something out because it does not apply is correct, and silent. A bug affecting one tenant needs no scope section, and its absence says nothing.

Stating an unknown is for a gap that matters: something a reader would otherwise assume you had established. "Unknown: whether this reproduces in staging" earns its line. "Unknown: the marketing implications" does not.

When you cannot tell which one you are looking at, ask whether a reader would act differently knowing the answer. If not, leave it out rather than listing it as unknown. A wall of unknowns reads as diligence and works as noise.

## Estimates people give you

A number someone states is evidence about their belief, not a measurement. Keep it, and keep its status with it.

> Affects roughly 30% of users. Estimate from Dana on 2026-08-13, not measured.

Do not round it into authority by writing "affects 30% of users", and do not drop it because it is soft. That estimate is often the only reason anyone prioritised the work, so losing it loses the argument.

## Asked for, versus decided

"A customer asked for X" and "we are building X" are different facts, and collapsing them is how a roadmap acquires commitments nobody made.

Record a request as a request, attributed: a Linear customer request where that feature is in use, otherwise the customer's own words quoted in the issue. Record a decision only when a person made it, with who and when. An issue sitting in the backlog is not a commitment, and its description should not read like one.

## How to write an unknown

An unknown is not an apology and does not need a paragraph. It needs the gap and what closes it.

> Unknown: whether the search index change is a day or a week. The spike on 2026-08-18 settles it.

Two failure modes to avoid. Do not hedge everything, because a document where every sentence is uncertain carries no information. Do not bury the unknown at the bottom, because the person who could resolve it is scanning.

When the request itself cannot be answered without information you do not have, say what is missing and stop. Do not produce a plausible object and hope. One good question beats a confident draft that has to be unpicked later.

## Preserving information when rewriting

Rewriting destroys more information than it creates unless the rewrite is disciplined.

Read the whole object first, including comments and linked issues. Then classify every sentence as: still true and useful, still true but belongs elsewhere, no longer true, or you cannot tell.

Keep the first. Relocate the second and say where it went. Remove the third and leave a comment recording what changed, if anyone might have relied on it. **Keep anything in the fourth category.** An oddly specific sentence you do not understand is usually the one fact someone needed and nobody else recorded.

Prefer patch operations anchored on exact text over resending the whole body, so the parts you did not consider stay exactly as they were.

In your response, list what you removed and where each piece went. A rewrite whose losses are invisible cannot be reviewed, and one that cannot be reviewed will eventually be reverted wholesale.
