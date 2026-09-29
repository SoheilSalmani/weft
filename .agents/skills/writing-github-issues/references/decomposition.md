# Decomposition and dependencies

## Contents

- Choosing a mechanism
- Granularity
- Depth and width
- Dependencies
- Platform constraints

## Choosing a mechanism

| Mechanism | Use when | The signal |
| --- | --- | --- |
| Nothing | The work is one pull request | Most issues |
| Markdown checklist | Steps inside one pull request, or a completion checklist | No separate owner, no tracking value wanted |
| Sub-issues | Each child is independently mergeable or delegable | Each child could be a pull request by a different person |
| Sibling issues with a dependency | Sequential work that is not one outcome | The second thing would still exist if the first were cancelled |
| A plain reference in the text | Context only | No execution constraint |

## Granularity

A sub-issue is **independently meaningful executable work**. The test is mergeability: could this child become a pull request that someone reviews and merges on its own?

Never create sub-issues for coding steps. Write a test, edit a file, update a function, run the formatter: these are one pull request's checklist, and turning them into issues creates objects that all close within the hour and inform nobody.

Prefer sub-issues when the work is independently delegable, independently reviewable, independently trackable, meaningfully parallelisable, or separately blocked. If none of those is true, it is a checklist.

| Bad decomposition | Good decomposition |
| --- | --- |
| Open the webhook file | Add an idempotency key to the webhook receiver |
| Add a function | Backfill keys for in-flight retries |
| Add a test | Remove the legacy dedupe path |
| Run the test | |
| Update the comment | |

The left column is five objects that describe one pull request. The right is three that can be built, reviewed, and reverted separately.

## Depth and width

GitHub permits eight levels of nesting and 100 sub-issues per parent. Use **two levels** and **about seven children at most**.

The reason is not neatness. A parent with more than roughly seven children is a project, and projects belong in Linear where the product layer can see them. Depth past two levels means the middle layer carries nothing a reader needs.

When a parent grows past that, the fix is to promote it to a Linear project with GitHub issues underneath, not to nest further.

## Dependencies

`blocked by` means the blocked work **cannot start or cannot merge** until the blocker lands. That is the whole test.

Do not use it for work in the same area, work that would be tidier to do first, or work that merely mentions the same system. A dependency that does not change what someone works on today is noise, and it makes the real ones invisible.

Parent and child is decomposition, not dependency. Never add a `blocked by` between a parent and its own child: the hierarchy already says it.

Reasons to prefer a dependency over a sub-issue:

- The two pieces belong to different outcomes.
- One would survive the cancellation of the other.
- They have different owners and different completion conditions.

## Platform constraints

Three facts change what is possible, and all three were verified on 2026-08-13:

- **Dependencies are same-repository only.** The REST endpoint takes an issue id with no repository parameter. Cross-repository sequencing must be a sub-issue relationship, or a sentence naming the blocker with a link.
- **Sub-issues can cross repositories**, but only within the same repository owner.
- **Limits**: 100 sub-issues per parent, eight levels, and 50 issues per dependency relationship type. All are far above the conventions above, so they should never bind.

When creating either through the API rather than the CLI, remember the parameters take the **global issue id**, not the issue number:

```bash
gh api repos/{owner}/{repo}/issues/{n} --jq .id
```
