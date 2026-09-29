# Sources

For maintainers. Not needed during normal use.

Every convention here is **platform** behaviour that GitHub documents, **evidence** from outside this repository, or **ours**, meaning a defensible opinion nothing enforces. All checked **2026-08-13**.

## Platform behaviour

| Convention it supports | Source |
| --- | --- |
| Issue types are organisation-level, up to 25, defaults task, bug, feature | `docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/managing-issue-types-in-an-organization` |
| Issue fields are organisation-level, four types, 25 per org, pinnable to types, defaults Priority, Effort, Start date, Target date | `docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/managing-issue-fields-in-your-organization`, `github.blog/changelog/2026-07-02-issue-fields-are-now-generally-available/` |
| Sub-issues allow 100 children and eight levels, and can cross repositories under the same owner | `docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues`, `docs.github.com/en/rest/issues/sub-issues` |
| Sub-issue REST endpoints take `sub_issue_id`, the global issue id, not the issue number | `docs.github.com/en/rest/issues/sub-issues` |
| Dependencies are blocked by and blocking, 50 per type, with `is:blocked` and `blocked-by:` search | `github.blog/changelog/2025-08-21-dependencies-on-issues/`, `docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-issue-dependencies` |
| **Dependencies are same-repository only.** The REST endpoint takes no repository parameter | `docs.github.com/en/rest/issues/issue-dependencies` |
| Tasklist blocks retired during 2025 and replaced by sub-issues; ordinary task lists unaffected | `docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/about-tasklists` |
| Five alert types, and GitHub's own advice to use one or two at most | `docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax` |
| Diagrams render from `mermaid`, `geojson`, `topojson`, `stl` fences | `docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-diagrams` |
| Permalinks carry a commit SHA and render as a snippet only in their own repository, and only in comments | `docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/creating-a-permanent-link-to-a-code-snippet` |
| Closing keywords close only from the default branch; cross-repo needs `owner/repo#n` | `docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue` |
| Closing reasons include completed, not planned, and duplicate | `docs.github.com/en/issues/tracking-your-work-with-issues/administering-issues/marking-issues-or-pull-requests-as-a-duplicate` |
| Issue forms are YAML with presets for labels, assignees, type, projects, and are still public preview | `docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-issue-forms` |
| Projects offer table, board, and roadmap, 50 fields, and a hierarchy view that the issues list does not have | `docs.github.com/en/issues/planning-and-tracking-with-projects/learning-about-projects/about-projects`, `github.blog/changelog/2026-01-15-hierarchy-view-now-available-in-github-projects/` |
| Private vulnerability reporting exists and is separate from issues | `docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability` |
| `gh issue develop` creates a branch linked to an issue | `cli.github.com/manual/gh_issue_develop` |
| CLI gained sub-issue, type, and dependency flags on 2026-06-10 | `github.blog/changelog/2026-06-10-manage-sub-issues-types-and-dependencies-from-github-cli/` |
| GitHub MCP server toolsets and `--read-only` mode | `github.com/github/github-mcp-server` |
| Claude responds to `@claude` in a new issue's body or title, and the triggering user needs write access | `code.claude.com/docs/en/github-actions` |

## Evidence

| Convention it supports | Source |
| --- | --- |
| Separate facts from speculation; diagnosis supplements a symptom description but never replaces it | Simon Tatham, "How to Report Bugs Effectively" |
| Concise text improved measured usability 58%, scannable layout 47%, neutral language 27%, combined 124%; 79% of readers scan | Nielsen Norman Group, "How Users Read on the Web" |
| Conclusion first | Nielsen Norman Group, "Inverted Pyramid" |
| ADRs are short, immutable, and superseded rather than edited | Michael Nygard, "Documenting Architecture Decisions" |
| Vaguer issues cost more agent turns | Anthropic, Claude Code GitHub Actions |

## Ours

Nothing external requires these. They exist because they made technical work legible, and they can be changed by deciding to.

- An issue exists only when something must survive the session that produced it.
- The five epistemic markers, and the rule that unmarked text reads as a requirement.
- Titles carry the kind of work through grammatical form rather than a prefix or a label.
- Default body is three to eight lines with no headings; structure starts around 200 words.
- Decomposition is by mergeability; two levels and about seven children at most.
- No Projects, no milestones, no priority in GitHub.
- One line linking to Linear, never a copy.
- Draft, propose, and write are separate acts, and writes need explicit approval.

## Research memo

The full design, including the decision tree, the routing matrix, and the evaluation design, is the artefact "GitHub Issues as technical work orders", produced 2026-08-13. This file is the compact version, and it is what the skills follow if the two disagree.
