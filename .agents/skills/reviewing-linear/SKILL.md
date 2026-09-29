---
name: reviewing-linear
description: Use only when explicitly asked. Audits a Linear team, project, or set of issues for stale, duplicated, misplaced, or invented information and proposes specific corrections. Use when asked to review, audit, or clean up Linear content, labels, or milestones.
---

# Reviewing Linear

A workspace decays in predictable ways. Descriptions describe a plan that changed. Implementation notes pile up in objects nobody technical reads. Labels multiply and are applied to nothing. Milestones name teams. Updates say work continues.

This skill finds those and proposes fixes. It does not apply them unasked.

It runs only when someone asks for it, because it reads widely and proposes many changes, and neither is worth doing in the background.

## Scope

Take the scope from the argument: a team, a project, an initiative, or a list of issues. If none was given, ask rather than reviewing the whole workspace, which produces more findings than anyone will act on.

Read broadly before judging anything. Projects with their summaries and descriptions, issues with their descriptions, the label set with usage counts, milestones, and the most recent updates. A finding based on a truncated description is a finding about truncation.

## The passes

Run in this order. Earlier passes are cheaper and their findings often explain the later ones.

1. **Wrong object.** Something with no end state filed as a project. A checklist filed as sub-issues. A curated grouping that should be a view. An issue that is really technical execution detail.
2. **Titles.** Vague, unbounded, prefixed, dated, or restating a property.
3. **Stale content.** Descriptions describing a plan that changed, answered open questions still listed, acceptance criteria for scope that was dropped, dead links, invalidated assumptions left in place.
4. **Misplaced detail.** Implementation detail in Linear that belongs in a pull request, a GitHub issue, or an ADR. Decisions in comment threads that should be an ADR or a decision note.
5. **Structure.** Empty headings, sections added by template, a first sentence that restates the title, a project description carrying status.
6. **Labels.** Duplicates, synonyms, labels applied to nothing, labels restating a property, prefixes that should be groups.
7. **Milestones.** Names that describe teams, disciplines, or phase numbers rather than states.
8. **Updates.** Manufactured progress, health values that contradict the facts, projects with a stale or missing update.

Detection tests for each are in `references/checks.md`. Use them rather than judging by taste, so the same object gets the same verdict twice.

## Output

A ranked list. Most consequential first, which usually means wrong object, then stale, then misplaced, then cosmetic.

For each finding:

- The object, by identifier and title
- What is wrong, in one sentence
- The proposed fix, concretely enough to apply without further thought
- Where any removed content goes
- Whether it is safe to apply mechanically, or needs a decision

Group trivial findings. Twelve titles in sentence case is one finding, not twelve.

Say plainly when a pass found nothing. A clean pass is information.

## What not to do

**Do not apply anything without approval.** Findings are proposals. Apply only what is approved, and apply it with the editing rules in `tracking-work-in-linear`: read the current state, preserve what is useful, relocate rather than delete, patch rather than replace.

**Do not rewrite for style.** A blunt description that is accurate is not a finding. Something is only wrong if it is inaccurate, stale, misplaced, missing, or genuinely unreadable.

**Do not invent the missing information.** If a description is thin, the finding is that it is thin and what it needs, not a fuller version you composed. Report the gap; do not fill it from imagination.

**Do not propose a taxonomy.** When labels are broken, the fix is subtraction first: merge duplicates, delete or archive what is unused. Proposing a new scheme to replace a scheme nobody used repeats the original mistake.

**Do not delete history.** Comments stay. Completed milestones stay. Canceled objects stay with a reason.

## Before you finish

- Every finding names the object and a concrete fix.
- Nothing was changed in Linear without explicit approval.
- No finding is a matter of preference.
- Nothing proposed for removal is lost: it is either genuinely obsolete or relocated, and the response says which.
- Findings are ranked by consequence, and trivial ones are grouped.
- Passes that found nothing are reported as clean.
