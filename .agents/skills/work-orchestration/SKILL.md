---
name: work-orchestration
description: Decides which artefacts a piece of work needs across Linear, GitHub issues, pull requests, commits, records, AGENTS.md, skills and documentation, then delegates each one to the specialist skill that owns it and reconciles the result afterwards. Use when asked where a piece of work should be tracked, which artefacts a change needs, to orchestrate or carry out a change end to end, or to audit and update every artefact a change affected. Not for single-artefact requests that already name what they want, and not for judging whether a decision deserves a record, which architectural-decisions owns.
---

# Work orchestration

This skill decides **which artefacts should exist and who writes them**. It does not write them: every artefact has a specialist that owns its craft, and restating their rules here would create a second source that drifts.

Success is not how many artefacts were created. It is that each important fact has one understandable home.

## Do not take work that is already routed

If the request names one artefact and needs no cross-artefact decision, **let the specialist own it**. "Write this commit message", "improve the README", "draft an ADR for X", "write a PR description" are not orchestration. Step aside.

Orchestrate when the question is *which* artefacts, *in what order*, or *what else did this change affect*.

## Workflow

```
classify intent → inspect what already exists → decide the minimum artefact set
→ order them safely → delegate each to its specialist → link rather than duplicate
→ reconcile state that the work made stale
```

Plan-only requests stop after the third step. "Where should this go", "how would you structure this", "plan this workflow" get the artefact plan below and reasoning, **and no mutations**.

### 1. Classify

Enough to route, then stop. Product or technical; a change or a decision; durable or transient; one change or several; user-facing or internal. Do not emit the classification as a report unless it was asked for.

### 2. Inspect before proposing

Search before creating, always. An artefact created because nobody looked is the most common failure this skill exists to prevent. Check Linear, open issues, open pull requests, `docs/adr/`, and the files themselves. Read current state before editing anything.

### 3. Minimise

Before proposing any new artefact, answer: **what durable value does this add that the existing graph does not already provide?**

Valid answers: independent status, independent delegation, durable rationale, a different audience, a separate lifecycle, discoverability, enforcement, technical persistence beyond the session.

No valid answer means do not create it. **Creating nothing is frequently the correct outcome**, and an intermediate artefact added for consistency is waste.

### 4. Route

At a glance: the tracker holds why work exists and where it stands; a technical issue holds execution state worth surviving the session; a pull request holds this change and its verification; a record holds a durable decision; the agent instruction file holds what agents need on most tasks; a skill holds a reusable procedure; tooling holds anything that must not depend on the model remembering.

`references/work-routing.md` holds the full matrices when routing is ambiguous: the four layers, which information type lives in which of them, and a second table for information belonging to none of the four, such as agent instructions, enforcement and dedicated files.

Read `references/workflow-patterns.md` for the common shapes and when each artefact is genuinely warranted.

### 5. Delegate

Hand each artefact to the skill that owns it, with **only the context that artefact needs**. Do not pass the orchestration reasoning downstream. `references/delegation.md` holds the capability map and the per-skill context contracts.

**No orchestration loops.** The orchestrator chooses responsibility; the specialist performs it; control returns here only for cross-artefact reconciliation. A specialist must not re-enter orchestration to do its own job.

### 6. Link, never duplicate

Use native relationships: issue to pull request, sub-issue, dependency, Linear's own integration, an ADR link. Free-text restatement is not a link and goes stale independently.

**Decide whether merging completes the work, then let the specialists word it.** That decision is cross-artifact and belongs here; the exact keywords do not. The pull request skill owns GitHub's closing semantics, and the Linear skill owns Linear's linking verbs.

The decision needs one input most people skip: **what happens after merge in this project?** Where a tracker has states beyond Done, or a deploy or release stands between merging and users, merging is not completion and the link must not say it is. Inspect the configured automations rather than assuming them.

### 7. Reconcile

After multi-artefact work, audit what the work made stale rather than reformatting what is fine. `references/reconciliation.md` holds the checklist and the merged-is-not-done distinction.

## The output

Two shapes, and both are conversational output or a plan file. **Neither is a section to add to an artefact.** The pull request, Linear and commit skills each ban a `Summary` heading, and this does not overrule them.

### The artefact plan, before any work

Every plan states this table before its first implementation step, and a plan-only request gets it on its own. **All six rows appear every time**, because an artefact never considered and one deliberately skipped look identical once the plan is written.

| System | Verdict |
| --- | --- |
| Linear | ENG-214, scope updated |
| GitHub issue | none — nothing outlives the session |
| PR / commits | one PR, two commits |
| ADR | none — no architectural decision |
| Docs | README install section goes stale; CONTRIBUTING and SECURITY unaffected |
| Ideas | garden row moves to `building` |

**`none` is a verdict, not a gap**, and it carries its reason. That reason is the minimisation question from step 3, answered per system rather than in general. A tiny documentation fix still walks all six rows, and five of them saying `none` is the correct output rather than a sign the table was wasted.

**Docs means every documentation surface**, not a `docs/` directory: README, CONTRIBUTING, the community health files GitHub recognises, guides, and any external documentation site. They share one row because they share one question — did this change make something a reader is told false? — but the verdict names which surfaces, since `writing-repository-readmes`, `contributor-documentation` and `writing-documentation` own different ones. A verdict of just "yes" hides which file anyone is meant to open.

Below the table, only when they apply: `AGENTS.md`, a skill, repository metadata, tooling. These are not documentation and are rarer, so a row saying `none` for each on every plan is noise rather than rigour.

Then the order, where one artefact must exist before another can reference it, and which specialist skill writes each one.

### The recap, after delegating

What actually landed, not what was planned:

- Each artefact created or updated, with its identifier or link.
- What reconciliation changed, and what it found already correct.
- Anything proposed but not authorised, and anything a specialist declined.
- Any system that was unreachable, named precisely. Never fill a gap with a guess.

Where the plan and the recap disagree, the recap is right. Do not edit the plan to match.

## When artefacts disagree

Do not take the most recently edited text. **Authority is by information type, and evidence outranks prose.**

| Disagreement about | Authoritative |
| --- | --- |
| What the code does | The code |
| A tool's configured behaviour | The config file, not prose describing it |
| Product scope | The tracker item |
| Architecture | The accepted record, unless superseded |
| What agents should do operationally | `AGENTS.md`, with rationale linked |
| User-facing behaviour | The canonical docs source |

Two rules carry most cases. **Configuration beats prose describing it**: if an instruction file and the formatter disagree on line length, the formatter is right and the prose is stale. And **an instruction contradicting an accepted record is a question, not a fix**: establish which went stale first, because trusting whichever loads automatically is how a repository ends up with two architectures.

Surface the mismatch rather than quietly implementing the convenient reading.

## Authorisation

Four classes. **Never escalate across them automatically.**

| Class | Examples | Default |
| --- | --- | --- |
| Read | search, inspect, classify, draft, propose | Free |
| Repository mutation | edit files, stage, commit | Follow the repository's normal workflow |
| Work-management mutation | create or close Linear items and issues, change status | **Ask** |
| Publishing | open or merge a pull request, release, change repository metadata | **Ask** |

Within an authorised class, do the work. Do not stop for confirmation at every obvious internal step; that is not safety, it is friction. When something material falls outside the authorised class, do the rest and **propose** that step.

## When a system is unavailable

Continue the independent work and report the gap precisely. If Linear cannot be reached, implement and commit, then say which link was not made. **Never fabricate remote state**, and never assume an artefact exists because it should.

## Detection during implementation

Work reveals things. Route them, but only when they clear the bar:

- **A durable architectural decision** emerges: route to the ADR skill. Ordinary code choices are not decisions. If an accepted record already governs it, link and follow it.
- **A stable rule agents will need often** appears, such as a generated directory or a source-of-truth constraint: route to `project-instructions`. A temporary implementation discovery is not a durable rule.
- **A recognisable procedure recurs** and no skill captures it: route to `skill-engineering`. Once is not a pattern, and creating a skill is never a prerequisite for finishing small work.
- **Something the author will later need to recall unaided** lands, such as a new command surface, a constraint that looks arbitrary, or a term the project invented: route to `remembering-what-you-built`. Code that only moved is not it, and most changes produce nothing.
- **User-visible behaviour changed**: check whether canonical docs went stale. Not every change touches the README, and if detailed docs live externally, do not create a repository-local copy.
- **Repository identity changed**: only then consider description, topics or homepage. Never per feature or dependency. **Check visibility first**: discovery and search work does not apply to a private repository.
- **Contributor workflow changed**: only then CONTRIBUTING. Internal agent changes are not contributor-facing.

## Consistency is not a reason to edit

A good README, issue, pull request, ADR or `AGENTS.md` stays as it is. Semantic accuracy outranks stylistic uniformity, and editing correct artefacts to match a house style is churn. Historical discussion, comments, commit history and superseded records are not rewritten for presentation.

## Before you finish

- Every proposed artefact answered the minimisation question.
- No specialist's rules were restated here or in the delegation context.
- Links are native relationships, and closing semantics match actual completion.
- Nothing was mutated beyond the authorised class.
- Reconciliation changed only what was genuinely stale.
- Every plan carried all six roster rows, each with a verdict and a reason.
- Every recap named what landed, not what was intended.
- Unreachable systems were reported, not invented.
