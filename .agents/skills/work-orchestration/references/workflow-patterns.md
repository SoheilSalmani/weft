# Workflow patterns

These are shapes that recur, not pipelines to execute. This file covers when each shape fits and where the judgement sits.

## Upstream of all of this

Ideation comes first and is not work. A thought about what could be built lives in the
idea garden until someone decides to build it. That decision is the boundary: after it,
the shapes below apply and the idea document stops changing, except to record that
something was dropped.

## The shapes

**Small product fix.** `Linear → implementation → commit → PR`. No GitHub issue: if nothing about the technical context needs to survive the session, the issue is overhead. This is the most common correct shape.

**Complex technical execution.** `Linear → GitHub issue → commits → PR(s)`. The issue earns its place when constraints, reproduction steps or investigation findings must outlive the session, or when several pull requests need coordinating.

**Pure technical debt.** `GitHub issue → commits → PR`. No Linear item unless there is genuine product or project coordination. Not everything needs a product representation.

**Architectural change.** `investigation → ADR → issue(s) → commits/PRs`. The record comes before the execution artefacts, because the decision constrains them.

**Tiny documentation fix.** `edit → commit → PR`. No tracking artefact at all.

**A new universal agent constraint.** Route to `project-instructions`. No issue, no ADR, unless the constraint itself came from a decision worth recording.

**A new reusable procedure.** Route to `skill-engineering`.

**Something that must not depend on the model remembering.** Route to deterministic enforcement. Prose is the wrong mechanism and a skill is not a substitute for enforcement.

Be concrete about where it goes, and **check that the destination exists before naming it**. Not every repository has CI. Where it is absent, enforcement means a script, a git hook, or a check wired into whatever validation command the project already runs. Routing something to CI in a repository with no pipeline names a destination that would have to be built first, and saying so is the honest output.

## Product and technical in one request

Keep the product story in Linear: the user pain, the outcome, the scope, where it stands. Keep implementation detail in the GitHub issue and the pull request.

Two failure modes, and the second is less obvious:

- **Copying the technical plan into Linear.** It goes stale the moment implementation diverges, and stakeholders cannot read it anyway.
- **Stripping Linear to a title.** Stakeholders then cannot tell what the work achieves. Linear is not a pointer to GitHub; it is the product record.

One test settles most cases: would this line be false after the pull request merges? Then it does not belong in Linear.

## Commit timing

Commits happen during implementation, grouped by coherence, not by schedule. **Do not create empty or filler commits to represent progress.** Status lives in the tracker, not in history. Not every intermediate step deserves a commit, and one coherent change may span several files.

## Pull request timing

A pull request exists when there is a concrete branch change worth reviewing. Draft versus ready is the pull request skill's decision; whether a pull request is warranted at all is this skill's.

Check for open work touching the same paths before restructuring anything. An open pull request turns a file move into a conflict.

## When an artefact already exists

Prefer updating over creating. A near-duplicate issue splits the conversation and the history. If an accepted ADR already governs the decision, link and follow it rather than recording the same conclusion twice.

If the existing artefact is wrong rather than stale, that is a change to make deliberately, with the reason recorded.
