# Reconciliation

Run after substantial multi-artefact work. The purpose is to find what the work made **wrong**, not to make everything look alike.

## The audit

**Work state**

- Is the implementation actually complete, or only merged?
- Does the technical issue still describe real remaining work, or is it now historical?
- Did the pull request close the issue it should have closed?
- Did a partial change close a parent that still has open children?
- Does the Linear item reflect where the work genuinely stands?

**Canonical content**

- Is the issue body describing an approach that was abandoned?
- Does the pull request description still match the final diff?
- Did the implementation contradict an accepted ADR? That is a supersession, not an edit.
- Did user-visible behaviour change in a way that makes documentation false?
- Did a durable constraint appear that `AGENTS.md` should carry?

**Links**

- Can someone starting at any artefact reach the others?
- Are there duplicate artefacts covering the same work?
- Do closing keywords match reality?

Update only what is genuinely stale. An artefact that is merely formatted differently from its neighbours is not a finding.

## Merged is not done

These are distinct states and conflating them produces false completion:

```
code merged  →  deployed  →  released  →  available to users  →  product work complete
```

A pull request merging closes the pull request. Whether it closes the GitHub issue depends on whether the technical work is finished. Whether it completes the Linear item depends on whether users can actually use the outcome, which merging usually does not establish.

**Often the distinction has nowhere to live.** A tracker whose states stop at Done has no way to express "merged but not released", and trackers commonly auto-advance on merge. So before linking, establish two things from the live systems, not from memory: what states exist after the one merging lands in, and what automations are configured. Both are per project or per team.

If nothing exists after Done and merging does not make the outcome usable, the link must not claim completion, and the state has to be moved deliberately later.

The keywords themselves belong to the specialists: the pull request skill owns GitHub's closing semantics, the Linear skill owns Linear's linking verbs. This skill decides only **whether merging completes the work**.

For an issue with sub-issues, closing the parent is a claim about all children. Check them.

## History is not edited for tidiness

Comments, updates, commit history and accepted or superseded ADRs are the record of what happened. Do not rewrite them so the current state reads more cleanly.

Current canonical bodies are different: an issue description, a pull request description and `AGENTS.md` are meant to describe the present, so correcting them when they become false is maintenance, not revisionism. The distinction is whether the artefact's semantics are "what we thought then" or "what is true now".

## Partial failure

If one system was unreachable, say exactly which link or update did not happen and what would complete it. A reconciliation report that omits the step that failed is worse than no report, because it implies the graph is consistent.
