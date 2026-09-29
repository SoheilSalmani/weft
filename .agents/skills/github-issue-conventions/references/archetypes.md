# Archetypes

Patterns, not templates. Take the lines that carry information for this specific piece of work and leave the rest. A three-line issue that answers the seven questions beats a complete one that does not.

## Contents

- Bug
- Technical task
- Refactor
- Technical debt
- Migration
- Investigation
- Performance
- Reliability and observability
- Security
- Dependency upgrade
- Removal

## Bug

Observed behaviour, expected behaviour, and how the gap was seen.

Keep the symptom separate from the diagnosis. A suspected cause is marked `Hypothesis` with what would confirm it; only a cause someone actually confirmed is stated plainly. This separation is the single most useful thing in a bug report, because a plausible diagnosis written as fact redirects everyone who reads it afterwards.

Reproduction, environment, screenshots, and logs are included **only when they exist and matter**. "Not reliably reproducible, seen twice in production" is a fact worth recording, not a gap to apologise for. Never invent a reproduction rate, an affected version, or an affected user count.

Expected behaviour can be omitted when it is obvious from the observed behaviour. `Retry submits the payment twice` does not need `Expected: one payment`.

## Technical task

The change in one sentence, why now in engineering terms, the constraints, and how we know it is done. Nothing else unless it is a constraint.

## Refactor

The only mandatory content is **what the current structure prevents**. `Refactor auth code` is not an issue. `Extract token validation so the mobile client can reuse it without importing the web session module` is.

Name the thing that becomes possible: duplication removed, a boundary established, a feature unblocked, coupling reduced, a legacy path retired, testing simplified. If none can be named, do not open the issue.

## Technical debt

State the concrete cost of leaving it: incidents it caused, time lost per change, work it blocks, or risk carried. Where a number exists, give it with its source.

A debt issue with no stated cost is a preference, and it will sit open forever because nothing can ever make it urgent.

## Migration

Source state, target state, the compatibility requirement during the transition, sequencing, rollback, data handling, and the completion criterion.

The completion criterion is the part most often missing, and for a migration it is usually a count reaching zero: no callers of the old path, no rows in the old shape, no references to the old flag. Write it as something you can query.

## Investigation

The deliverable is **knowledge or a decision, not code**.

State the question as a question, the decision it unblocks, what is unknown, and what output is expected: a comment, a document, an ADR draft, or a follow-up issue. Add a timebox only when a real deadline exists; an invented one is noise.

An investigation closes when the question is answered. If the answer implies implementation, either extend this issue when the implementation obviously belongs to the same work and the same owner, or open a separate implementation issue when that gives clearer ownership. Do not let an investigation silently become an implementation commitment, and do not automatically convert every one.

## Performance

Keep four things apart, and label them:

| Thing | Rule |
| --- | --- |
| Measurement | What was observed, how, and when. No number without a method |
| Target | Only if a person set one. Otherwise `Unknown` |
| Hypothesis | Why it might be slow, marked as a hypothesis |
| Optimisation | What to change, and only once the measurement supports it |

Never invent a baseline or a target. "No baseline measured yet" is a normal and honest state for a performance issue to start in.

## Reliability and observability

The failure mode, how it is currently detected or missed, what signal is wanted, and what someone would do when that signal fires.

An observability issue that does not say what action the signal enables produces a dashboard nobody opens.

## Security

In a private repository, an issue is acceptable. Write it so the body states the vulnerable behaviour and its impact without being a working recipe: no proof-of-concept payload, no exact exploitation sequence. Put those in a collapsed block if they are genuinely needed, or in a private advisory.

For anything exploitable against production, or any public repository, use private vulnerability reporting or a draft security advisory instead of an issue.

## Dependency upgrade

A mechanical upgrade with no behaviour change needs no issue. The pull request is the record.

An issue is warranted when the upgrade forces a code migration, changes behaviour, drops a supported platform, or carries rollback risk. Say which of those applies in the first sentence, because it is the entire reason the issue exists.

## Removal

State the evidence that the thing is genuinely unused: no call sites, no traffic in a named window, no imports, a flag switched off since a date. Weak evidence is itself worth stating, as `Hypothesis`, so the first task is confirming it.

Done is an absence: the symbol, route, table, or flag no longer exists, and nothing references it.
