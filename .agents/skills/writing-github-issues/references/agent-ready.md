# Preparing an issue for a coding agent

An agent told to "implement issue #123" should know what to inspect and what outcome is required, without asking a question and without being handed a solution it has not verified against the code.

## Three thresholds

**Enough to begin.** The objective, one to three starting points, and the constraints that would otherwise be discovered by breaking something.

**Enough to finish.** Observable completion conditions, and the verification that demonstrates them.

**Unnecessary pre-solving.** Anything the agent could derive from the code in a few minutes: the functions to write, the file list, the algorithm. This is worse than silence. It is stale within days, it suppresses a better approach the code would have suggested, and it costs turns to reconcile with a codebase that disagrees.

An issue is ready when the first two hold and the third is absent.

## The check

Run this when an issue is about to be implemented. It is a check, not a rewrite.

- [ ] The objective is concrete and verifiable.
- [ ] Hard constraints are explicit and marked `Must`, each with its reason.
- [ ] Uncertainty is explicit and marked `Hypothesis` or `Unknown`.
- [ ] There is enough starting context to begin, and no more.
- [ ] The end state is understandable, and observable.
- [ ] Real dependencies are represented as relationships, not prose.
- [ ] Relevant Linear and ADR links are present.
- [ ] The implementation is not unnecessarily prescribed.
- [ ] No stated file path, symbol, or interface is stale.

**If it passes, stop.** Report that it is ready and do nothing else. Continuously improving a usable issue burns attention and changes nothing.

If it fails, fix only the failing items, and say which.

## Out of scope is the highest-value line

For an agent, the single most useful sentence in an issue is the one saying what must not change. It is the only defence against a scope that quietly widens while the work proceeds.

> Do not change search ranking, pagination, or the project search path.

Write it whenever the work sits next to something fragile.

## Verification

State how the outcome is demonstrated, using what the repository already has:

> Done when `pnpm test packages/db` passes with the new migration applied to a fresh database, and `SCHEMA_SQL` has no remaining callers.

Where no command exists, an observable condition is enough. Do not invent a test command that the repository does not have.

## Delegating

`gh issue edit {n} --add-assignee "@copilot"` assigns GitHub's coding agent. Claude responds to `@claude` in an issue comment, and in the body or title of a newly opened issue, when the GitHub Action is installed. Check whether either is set up in the repository. Where neither is, delegation means a person or agent reading the issue and starting work.

Either way the issue is the input, which is why the check above matters more than any workflow configuration.
