# Sources

For maintainers. Not needed during normal use.

Every convention in this skill set is one of three things: **platform** behaviour that Linear or Claude Code documents, **evidence** from research outside this repository, or **ours**, meaning a defensible opinion that no tool requires. When a convention is challenged, this file says which it is and what to re-check.

All sources last checked **2026-08-13**.

## Platform behaviour (Linear)

| Convention it supports | Source |
| --- | --- |
| Object hierarchy and what each container is for | `linear.app/docs/conceptual-model` |
| Projects have a short summary and a separate long description; projects expect a start and an end | `linear.app/docs/projects`, `linear.app/docs/project-overview` |
| Project summary is capped at 255 characters; issue and project descriptions accept patch operations; update health is `onTrack`, `atRisk`, or `offTrack` | Linear MCP tool schemas (`save_project`, `save_issue`, `save_status_update`) |
| Project status is customisable within five categories, and is not the same thing as health | `linear.app/docs/project-status` |
| Updates carry an automatically generated progress diff; projects overdue for an update are flagged and then greyed out | `linear.app/docs/initiative-and-project-updates` |
| Milestones mark stages inside a project, show completion progress, and can be converted to projects | `linear.app/docs/project-milestones` |
| Label groups are mutually exclusive; `Group/Value` creates both; labels carry descriptions; labels can be merged or archived; some names are reserved | `linear.app/docs/labels`, `linear.app/docs/project-labels` |
| Sub-issues inherit team, priority, and project but not labels; issues convert to projects | `linear.app/docs/parent-and-sub-issues` |
| Relations are `blocks`, `blocked by`, `related`, `duplicate`; mentioning an issue creates a `related` relation | `linear.app/docs/issue-relations` |
| Project dependencies are end-to-start and show violations on the timeline | `linear.app/docs/project-dependencies`, `linear.app/docs/timeline` |
| The editor renders Mermaid, tables, collapsible sections, and mentions | `linear.app/docs/editor` |
| Triage is the intake queue, with accept, decline, duplicate, snooze, and merge | `linear.app/docs/triage` |
| Initiatives are for curated sets tied to an objective; views are for filter-based collection | `linear.app/docs/initiatives`, `linear.app/docs/custom-views` |
| Naming an entry point and stating non-goals is correct for a delegated issue | `linear.app/docs/coding-sessions` |
| Pull request diffs, checks, and reviews can be read against the issue in Linear | `linear.app/docs/diffs` |
| Linear's own example agent prompts show the proposed objects before creating anything, and tell the agent not to invent structure the source material does not support. This is Linear recommending a prompt pattern, not the platform enforcing one. Our preview gate is stricter, and is ours | `linear.app/docs/mcp` |
| Workspace, team, and personal guidance shape the in-Linear agent | `linear.app/docs/linear-agent` |
| Customer requests attach to issues and projects and carry customer attributes | `linear.app/docs/customer-requests` |

## Platform behaviour (GitHub and Claude Code)

| Convention it supports | Source |
| --- | --- |
| Closing keywords, cross-repository syntax, default-branch constraint | `docs.github.com`, "Linking a pull request to an issue" |
| Skill layout, frontmatter fields, invocation control, content lifecycle | `code.claude.com/docs/en/skills` |
| SKILL.md under 500 lines, references one level deep, table of contents above 100 lines, third-person descriptions, evaluation-first development | `platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices` |
| `evals/evals.json` format and the with-skill against without-skill comparison | `agentskills.io/skill-creation/evaluating-skills` |

## Evidence

| Convention it supports | Source |
| --- | --- |
| Concise text improves measured comprehension by 58%, scannable layout by 47%, neutral language by 27%, and the three together by 124%; 79% of readers scan | Nielsen Norman Group, "How Users Read on the Web" |
| First sentence carries the conclusion | Nielsen Norman Group, "Inverted Pyramid: Writing for Comprehension" |
| The first level of disclosure must hold what readers frequently need | Nielsen Norman Group, "Progressive Disclosure" |
| Percent-done and task counts are poor status signals; movement and unknowns are the signal | Basecamp, *Shape Up*, chapter 13 |
| ADRs are short, immutable, and superseded rather than edited | Michael Nygard, "Documenting Architecture Decisions", and `adr.github.io` |
| Short titles, optional descriptions, quote users rather than paraphrase | Linear, "Write issues, not user stories" |
| Smallest set of high-signal tokens; retrieve just in time; prefer canonical examples to exhaustive rules | Anthropic, "Effective context engineering for AI agents" |

## Ours

These have no external authority. They exist because they made the workspace legible, and they can be changed by deciding to.

- One outcome per object, and a title that needs "and" is two objects.
- Descriptions are current truth, comments are history, and invalidated assumptions are removed rather than struck through.
- Structure starts at roughly 150 words, and never because a template offered it.
- Bug titles are declarative, issue titles imperative.
- No dates, prefixes, or property restatements in any title.
- Sentence case, British spelling, no em dashes, matching the rest of this repository's skills.
- Every label names a view.
- Health values are chosen by the rule in `writing-linear-updates`, not by feel.
- An agent never writes to Linear without a preview and explicit approval.

## Research memo

The full research behind these conventions, including the object decision matrix, the source register with access dates, and the evaluation design, is in the artefact `A Linear operating model, and the skills that enforce it`, produced 2026-08-13. This file is the compact version. If the two disagree, this file is what the skills follow.
