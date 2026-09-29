---
name: triaging-github-issues
description: Use only when explicitly asked. Audits open GitHub issues for staleness, unmarked speculation, over-decomposition, fake dependencies, label sprawl, and missed closures, and proposes specific corrections. Use when asked to review, triage, audit, or clean up the GitHub issue tracker.
---

# Triaging GitHub issues

An issue tracker decays in predictable ways. Bodies describe a plan the code has since contradicted. A hypothesis written as fact three months ago is now everyone's mental model. A parent has fourteen children that each took ten minutes. Issues sit open whose pull request merged in March.

This skill finds those and proposes fixes. It changes nothing on its own.

It runs only when asked, because it reads widely and proposes many changes, and neither is worth doing in the background.

## Scope

Take the scope from the argument: a repository, a label, a milestone, or a list of issue numbers. With no argument, ask rather than auditing everything, since an unbounded audit produces more findings than anyone will act on.

Read before judging. `gh issue list` for the set, then the full body and comments of each issue you intend to report on, plus linked pull requests where closure or staleness is in question. A finding based on a truncated body is a finding about truncation.

## The passes

Run in this order. Earlier passes are cheaper, and their findings often explain the later ones.

1. **Should not exist.** Work with no technical dimension that belongs in Linear, trivial work already covered by a merged pull request, or a duplicate of another issue.
2. **Missed closure.** The completing pull request merged and nobody closed the issue, or the work was abandoned and nobody said so.
3. **Stale content.** The body contradicts the code, a merged pull request, or a later comment. A named file, symbol, or interface no longer exists.
4. **Unmarked speculation.** A hypothesis, a suggested approach, or a guess written as a plain assertion, which every later reader now treats as a requirement.
5. **Missing completion conditions.** No observable way to tell whether the work is done.
6. **Structure noise.** Empty headings, template residue, a first sentence restating the title, or evidence that has grown larger than the issue.
7. **Decomposition.** Sub-issues that are coding steps, parents with too many children, dependencies that express no real execution constraint, and dependencies used where a parent relationship belongs.
8. **Metadata and configuration.** Label sprawl, labels nothing filters on, one dimension spread across several labels, status encoded as a label, milestones or projects duplicating Linear, and issue templates asking for what a native field already carries.

Detection tests for each are in `references/checks.md`. Use them rather than judging by taste, so the same issue gets the same verdict twice.

## Output

A ranked list, most consequential first, which usually means should-not-exist and missed closure above stale, and stale above cosmetic.

For each finding:

- The issue, by number and title.
- What is wrong, in one sentence.
- The proposed fix, concrete enough to apply without further thought.
- Where anything removed should go instead.
- Whether it is safe to apply mechanically, or needs a decision.

Group trivial findings. Nine titles needing sentence case is one finding, not nine.

Report a pass that found nothing as clean. That is information.

## What not to do

**Do not change anything without approval.** Findings are proposals. Apply only what is approved, and apply it with the editing rules in `writing-github-issues`: read the whole issue, preserve facts, relocate rather than delete, and say what moved.

**Do not rewrite for style.** A blunt issue that is accurate and executable is not a finding. Something is wrong only if it is inaccurate, stale, misleading, unexecutable, or genuinely unreadable.

**Do not invent the missing content.** When a body is thin, the finding is that it is thin and what it needs, not a fuller version you composed. Report the gap.

**Do not propose a taxonomy.** When labels are broken, the fix is subtraction: merge synonyms, delete what nothing filters on. Replacing an unused scheme with a better unused scheme repeats the mistake.

**Do not close on someone's behalf.** Propose the closure and the reason. Closing an issue is a statement about the world, and the person who owns the work makes it.

**Do not push technical detail back into Linear.** An issue carrying too much implementation detail is fixed by moving it to the pull request or a comment, never by summarising it upward into the product layer.

## Before you finish

- Every finding names the issue and a concrete fix.
- Nothing was changed in GitHub without explicit approval.
- No finding is a matter of preference.
- Nothing proposed for removal is lost: it is obsolete or relocated, and the response says which.
- Findings are ranked, and trivial ones grouped.
- Passes that found nothing are reported as clean.
