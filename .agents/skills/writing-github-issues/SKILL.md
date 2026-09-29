---
name: writing-github-issues
description: Decides whether a GitHub issue is worth creating, then drafts, decomposes, or rewrites it as a technical work order. Covers sub-issues, blocking dependencies, links to Linear and ADRs, and preparing an issue for a coding agent. Use when asked to file, draft, split, or improve a GitHub issue, when asked whether something is worth tracking, or when follow-up work surfaces mid-task and needs a home. For Linear issues use tracking-work-in-linear, and for auditing many issues at once use triaging-github-issues.
---

# Writing GitHub issues

Most work does not need an issue. The first job of this skill is deciding whether one should exist, and the second is making it a work order rather than a description.

Conventions live in `github-issue-conventions`. Invoke it if it is not already loaded, and read the one reference that answers the question in front of you.

## First, does it deserve an issue

Create one when **at least one** holds: knowledge produced now must outlive this session, the work needs more than one pull request, someone could pick it up cold, it was discovered while doing something else and would otherwise be lost, or other work genuinely cannot start until it lands.

Skip it when one pull request finishes the work, the pull request description carries everything durable, and nobody needs to know before that pull request exists.

**"No issue needed" is a correct answer, and a common one.** Say it plainly with the reason, offer the pull request as the record, and stop. If the user wants it tracked anyway, track it. This is advice, not a veto.

When the work has product consequences but no technical shape yet, it belongs in Linear and not here. The test is whether the line would still be true after the pull request merges: product rationale survives it, execution detail does not.

When one request contains both, such as a customer-visible failure and the refactor that would fix it, split them: the product problem is a Linear item, the technical work is a GitHub issue, and each links to the other. Do not write the full text in both places. The GitHub issue carries only the technical implication that constrains execution.

### Work discovered while doing something else

The most common decision in practice, and the one that a single rule gets wrong. Five options, in increasing cost:

| Do this | When |
| --- | --- |
| Fix it now | It is smaller than writing it down, and it is in scope |
| A TODO in the pull request description | It belongs to this change and the reviewer should see it, but not in this commit |
| A comment on the open issue | It is the same work, and an issue already owns it |
| A sub-issue | It is separable work under an issue that already exists |
| A new issue | It is separable, and nothing open owns it |

Do not default to the last row. What decides is whether the work is separable and whether anything already owns it, not whether it happened to be noticed.

The one wrong answer is dropping it silently because it is out of scope.

## Three modes

| Mode | Produces | Authorised by |
| --- | --- | --- |
| Draft | Text in the conversation | Any request to write or rephrase |
| Propose | A preview of the exact issue, its relationships, and its metadata | The default when asked to file something |
| Write | A call to `gh` or the GitHub API | An explicit yes, or an instruction that plainly authorises it |

Default to propose. Never write because the draft came out well, because an existing issue could be phrased better, or because the change looks obviously right. A better version is not authorisation.

## Before writing anything, read

1. **The request**, for what is actually being asked.
2. **The repository**, for what is true. Technical accuracy comes from the code, not the request.
3. **Existing issues**, so you do not file a near-duplicate. `gh issue list --search` before creating.
4. **The linked Linear item and any relevant ADR**, when the work touches either.

**Verify a path before naming it.** A file that does not exist, or was renamed last month, is worse than no reference at all, because it sends the reader somewhere real and irrelevant. If you have not opened the code, either check it or mark the location `Suggested`.

Do not build a list of every file the work might touch. One to three starting points is the useful amount, and more is pre-solving.

## Writing it

Titles and body shape are in `.agents/skills/github-issue-conventions/references/writing.md`. Archetype-specific content is in `.agents/skills/github-issue-conventions/references/archetypes.md`. Read the archetype that matches; do not read all of them.

Choose the least structure the work requires. For most issues that is a title and three to eight lines with no headings. Do not expand a three-line issue into a twenty-line template, and do not add a heading that will hold one sentence.

Mark everything that is not a requirement with `Must`, `Decided`, `Suggested`, `Hypothesis`, or `Unknown`. An unmarked sentence reads as a requirement, and that is how an agent's first idea becomes canonical architecture.

## Decomposition and dependencies

Read `references/decomposition.md` before creating any sub-issue or dependency. The short form:

- One issue, unless the parts are independently mergeable.
- A Markdown checklist for steps inside one pull request.
- Sub-issues when each child is separately mergeable, reviewable, delegable, or blocked.
- A blocking dependency only for a real execution constraint, never for "related".
- Two levels of nesting, about seven children at most. Beyond that it is a project, and projects live in Linear.

Show the whole shape before creating any of it, and create none of it until it is approved.

## Tool surface

Check what the installed CLI supports before emitting commands, because the flags moved recently:

```bash
gh --version
gh issue create --help | grep -E '\-\-(type|parent|blocked-by)'
```

`gh` gained `--type`, `--parent`, `--add-sub-issue`, `--blocked-by`, and `--blocking` on 2026-06-10. **Where the installed version has none of them**, relationships go through the API:

```bash
# sub_issue_id is the GLOBAL issue id, not the issue number
gh api repos/{owner}/{repo}/issues/{child} --jq .id
gh api --method POST repos/{owner}/{repo}/issues/{parent}/sub_issues -F sub_issue_id={global_id}
gh api --method POST repos/{owner}/{repo}/issues/{n}/dependencies/blocked_by -F issue_id={global_id}
```

Use `-F` rather than `-f`, since these parameters are integers. Say which surface you used, so the reader knows what to repeat.

Do not create labels, milestones, projects, issue types, or templates as a side effect of filing an issue. Configuration changes are deliberate and separate.

## Editing an existing issue

1. **Read the whole issue**, including comments, before proposing anything.
2. **Check the linked pull requests, issues, ADRs, and Linear item** where they affect current truth.
3. **Check the repository** when technical accuracy depends on it. A stated file path that no longer exists is a finding.
4. **Preserve facts.** Anything you cannot show to be obsolete stays.
5. **Promote discoveries into the body** when they change the objective, the constraints, the completion conditions, or the known-versus-unknown split. Everything else stays a comment.
6. **Remove obsolete prescriptive detail.** An implementation idea contradicted by the code is worse than nothing, because someone will follow it. A disproven hypothesis comes out of the body entirely; it survives as a comment, where the fact that it was ruled out is worth as much as a confirmation.
7. **Do not rewrite for style.**

**Leaving it alone is a result.** When an issue is accurate, current, and executable, say so and change nothing. If the only change available is one you would have phrased differently, there is no finding.

## Preparing an issue for an agent

Run the check in `references/agent-ready.md` when an issue is about to be implemented by Claude Code. If it passes, stop. Do not keep improving a usable issue.

## The preview

Before any write, show the title, the body as it will appear, the relationships you intend to create, any metadata, the links, and every unknown with what would resolve it. For an edit, show what changes, what you are removing, and where it goes instead.

## Before you finish

- The issue exists for a stated reason, or you recommended not creating one.
- The title states the objective or problem, with no type prefix, in the right grammatical form.
- The first sentence does not restate the title.
- Every hypothesis, suggestion, and unknown is marked.
- Constraints are real, and each says why.
- Completion conditions are observable.
- Code references are starting points or evidence, not an inventory.
- Nothing duplicates the Linear item beyond one link.
- Nothing was written to GitHub without approval.
