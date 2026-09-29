# Delegation

## Finding the right specialist

Match the work to a skill by **what its description says it does**, not by a remembered name. Skill descriptions are loaded at startup, so semantic matching is the discovery mechanism, and it survives a rename that keeps the meaning. There is deliberately no registry file here: a hardcoded list would be a second source of truth that goes stale silently.

The capabilities to look for, with the names current at the time of writing. If a name does not resolve, search the loaded descriptions for the capability instead.

| Capability needed | Look for |
| --- | --- |
| Product work tracking, projects, milestones | `tracking-work-in-linear`, conventions in `linear-conventions` |
| Linear status updates and health | `writing-linear-updates` |
| Auditing Linear for stale or invented state | `reviewing-linear` |
| Technical work orders | `writing-github-issues`, conventions in `github-issue-conventions` |
| Auditing open issues | `triaging-github-issues` |
| Grouping changes and writing commit messages | `writing-commits` |
| Amending, splitting or reverting existing commits | `rewriting-history` |
| Pull request titles, descriptions, sizing | `writing-pull-requests` |
| Handling review feedback | `responding-to-review` |
| Whether a decision needs a record, and finding governing ones | `architectural-decisions` |
| Writing or superseding the record itself | `writing-adrs` |
| Persistent agent context | `project-instructions` |
| Building or auditing skills | `skill-engineering` |
| Cross-host skill and instruction portability | `agent-portability` |
| README as a landing page | `writing-repository-readmes` |
| CONTRIBUTING and community health files | `contributor-documentation` |
| Repository description, topics, discoverability | `repository-discovery` |
| Technical documentation structure | `writing-documentation` |
| Diagrams inside documents | `mermaid-diagrams` |

## Context contracts

Give each specialist what its artefact needs and nothing else. Passing the orchestration reasoning downstream makes the specialist re-decide what was already decided.

| Specialist | Give it | Do not give it |
| --- | --- | --- |
| GitHub issue | The technical problem, the Linear link, known constraints, investigation evidence | Product rationale, priority, dates, the routing argument |
| Pull request | The actual diff, linked work items, verification evidence and commands run | The full issue body, product background |
| ADR | The decision question, forces and constraints, existing and superseded records | Implementation detail, the ticket's status |
| Linear | Problem, outcome, scope, affected users, current state | The technical plan, filenames, stack traces |
| Commits | The staged diff and the change's intent | The artefact graph |
| Repository presentation | What user-facing behaviour actually changed, where canonical docs live, repository visibility | Internal implementation history |
| `project-instructions` | The candidate rule and the evidence it is durable and frequently needed | The whole implementation narrative |

## The no-loop rule

The orchestrator chooses responsibility. The specialist performs it. Control returns here only for cross-artefact reconciliation.

A specialist must never re-enter orchestration to carry out its own task, and orchestration must not be reloaded at each substep. If a specialist discovers that a *different* artefact is also needed, it reports that upward rather than creating it.

## When no specialist exists

In order:

1. Ask whether this is a one-off. Most are.
2. If it is, do the minimal correct thing using the repository's existing conventions, and move on.
3. Only route to `skill-engineering` when the capability is genuinely reusable and recurring.

**Creating the missing skill is never a prerequisite for finishing small work.** Blocking a two-line documentation fix on authoring a new skill is a failure of judgement, not thoroughness.

## Honest capability reporting across hosts

The specialist skills are portable, but three of them carry `disable-model-invocation`, which several hosts ignore. On those hosts an audit skill can fire unprompted. When reporting what orchestration will do on a host other than the one running it, say what that host actually supports rather than assuming parity. `agent-portability` owns that translation.
