---
name: humanizer
description: "Rewrites text so it reads as though a person wrote it, removing the punctuation and register tics that mark generated prose: em dashes used as all-purpose glue, antithesis constructions, synonym triads, hedging stacks, inflated diction and decorative structure. Judges each candidate in context instead of stripping it on sight, because every one of these is sometimes correct. Use when asked to humanize text, de-AI writing or make prose read naturally, when reviewing writing that sounds generated, and from other writing skills that produce prose a person will read. For invisible Unicode characters or container metadata, use ai-provenance-marks instead."
license: MIT
---

# Humanizer

Generated prose is rarely wrong. It is padded, evenly weighted, and punctuated by a small set of habits a reader notices without being able to name. The job is to find those habits, cut them, and leave everything that was doing real work.

**The failure mode is not missing a tic. It is cutting one that earned its place, or deleting a character without repairing the sentence around it.** Both produce worse text than the input, and both look like progress.

## Scope

In scope: visible prose style and punctuation, in text the user owns or asked you to edit.

Out of scope, and say so plainly rather than improvising:

- **Invisible Unicode artefacts and container metadata**, including C2PA and XMP. That is `ai-provenance-marks`, which works a different layer and explicitly excludes prose rewriting.
- **Making text undetectable.** No public detector exists for statistical text watermarks, so nothing here can be verified, and rewriting to defeat one only degrades the user's own writing.
- **Someone else's prose**, unless editing it is the actual request.
- **Meaning.** Register changes; claims do not.

## A tell is only a tell in context

Every habit catalogued in `references/tells.md` is sometimes the right call. An em dash can set off a genuine aside. Three items can be three real items. A hedge can be honest uncertainty. Strip by pattern and you get comma splices, false parallelism, and confidence the author never had.

The corpus has a precedent worth keeping in mind. `ai-provenance-marks` records prior art that swept every zero-width character and corrupted legitimate emoji, because invisible is not the same as meaningless. The same trap sits here: a construction is not a tic because it appears on a list.

So the unit of work is the sentence, not the character. Ask what the construction is doing. If the answer is nothing, recast the sentence without it. If it carries the meaning, leave it.

## The em dash

The most reliable tell, and the easiest to over-correct, so it gets its own test.

Replace the dash with the punctuation it displaced, then reread.

- A comma, colon, semicolon or full stop reads at least as well: the dash was glue. Cut it.
- Nothing else fits, because the clause is a real interruption a comma is too weak to hold: keep it.

Two in a paragraph is nearly always one too many. Two in a sentence is a rewrite.

Glue, and the repair:

> The guard was wrong — it protected every manual row.

> The guard was wrong: it protected every manual row.

Earned, so left alone:

> Every guard in the procedure protects a user write — except, as it turned out, the one that mattered.

A colon would promise a list, a comma cannot hold the reversal, and the aside has its own commas. The dash is the only mark that works.

Read `references/tells.md` for the other classes, each with a keep and a cut example. Open it when a construction is on the list but the call is not obvious.

## Workflow

1. **Read the whole text first.** Density is the signal: one triad in a page is a sentence, five is a habit.
2. **Classify, do not tally.** For each candidate, name what it is doing.
3. **Recast the sentence.** Deleting the character alone is how this goes wrong.
4. **Reread cold.** A sentence that now needs a second pass to parse is worse than the one it replaced.
5. **Report what changed and what you deliberately kept**, so the user can argue with the judgement and not just the result.

Where the text has a house style, follow it over anything here. A codebase using straight quotes, or a document using en dashes, has already decided.

## What never changes

- Quoted material, cited text, and anything inside quotation marks belonging to someone else.
- Code identifiers, paths, commands, log output, error text, and anything inside backticks or a fenced block.
- Numbers, names, and claims. If a hedge was load-bearing, state the uncertainty plainly rather than deleting it.
- Text you were not asked to edit.

## Composing from another skill

Other writing skills hand off here for their prose. The convention is a named cross-reference: the calling skill states that its prose goes through `humanizer` before it is final, and this skill owns the judgement.

`writing-commits` does this for subjects and bodies, where the constraints are tighter than ordinary prose. A commit subject also becomes a release-note line, so plainness beats specificity.

## Responsible use

For text the user owns or is authorised to edit. If the request is to pass generated work off as human-written, or to defeat a detector, say the skill does not cover that. One sentence, then move on.

Cutting a tic does not make text human-authored. Never claim it does.

## Before you finish

- Every change was made because the construction was doing nothing, not because it matched a pattern.
- No sentence was left with its punctuation deleted and its structure unrepaired.
- Quoted material, code, and claims are untouched.
- What was deliberately kept is stated, not silently accepted.
- The text reads as well aloud as on screen.
- No claim was made about detectability.
