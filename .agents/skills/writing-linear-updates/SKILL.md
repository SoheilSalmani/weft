---
name: writing-linear-updates
description: Writes Linear project and initiative updates that report what changed since the last one, choose a health value by rule, and say what it means for the target date. Use when asked for a project update, an initiative update, a status update, or a summary of where a project stands.
---

# Writing Linear updates

An update answers one question: what changed since the last one, and what does it mean?

It is not a summary of the project. The project already describes itself, and anyone reading the update can see it. Restating it is the most common way an update ends up saying nothing.

Linear attaches a generated progress report to updates, covering overall progress, milestone movement, target date changes, and lead changes, and it only appears when progress moved by more than a couple of percent. So the numbers are already there. What the writing must add is judgement: why it moved, what it means for the date, and what someone should do about it.

## Read first

- **The previous update**, and its date. Everything in this update is a delta from that point.
- **Issues that changed status since then**, and milestone progress.
- **Target date and scope changes** since then.
- **Open blockers**, including `blocked by` relations and project dependencies.
- **Anything the user told you**, which outranks all of it, because they know things the workspace does not.

If there is no previous update, this one covers the period since the project started, and says so in three words.

## Health, by rule

Health is `onTrack`, `atRisk`, or `offTrack`. Set it as the property. Do not also write it in prose beyond the opening word.

| Value | When |
| --- | --- |
| `onTrack` | Target date and scope are unchanged, and no blocker is more than a week old |
| `atRisk` | A blocker exists with no owner or no date, or the date holds only if something uncertain goes right |
| `offTrack` | The date or the scope will change, and this update is where that is said |

Choosing by rule rather than by feel is what makes health comparable across projects and over time. If you cannot tell which value applies, say what you would need to know and propose the more pessimistic one.

## The shape

Five to eight lines. No headings at that length.

1. **Health**, as one word.
2. **What changed**, as facts with dates.
3. **What it means** for the target date or scope. This is the sentence readers actually need.
4. **What is blocked or uncertain**, and who can unblock it.
5. **The next meaningful event**, with a date if one exists.

Leave out any of 2 to 5 that has nothing in it. Do not write a heading for an empty one.

## Never include

- A restatement of the project description
- Percentages or issue counts, which Linear attaches on its own
- A list of completed issues, which the issue list already shows
- Individual activity reports
- "Continuing to make progress", "working hard", and their relatives
- Apologies
- An invented cause for a slip

## When nothing changed

Post nothing, or post two lines. Both are correct, and which one depends on whether anyone is waiting.

Post nothing when the project is simply moving slowly and nothing is blocked. Linear flags projects that are overdue for an update on its own, so silence is already reported, and it is a more honest signal than a manufactured one.

Post two lines when something is stuck: name the blocker, say how long it has been there, and say who can clear it. Health is `atRisk` or `offTrack`, never `onTrack`.

Never manufacture progress. An update saying "work continues" turns a true unknown into a false healthy signal, which is worse than the silence it replaced.

## Initiative updates

An initiative update carries cross-project judgement only: a reallocation, a change of sequence, a bet that changed. If it could be reconstructed by concatenating the project updates underneath it, do not post it.

## Writing and posting

Draft, show the draft, and post only after approval. Include with the draft the facts it rests on and the proposed health value, so both can be checked before anything is published. Posting to Linear also posts to any connected Slack channel, so it is not a private act.

If nothing in the workspace supports a claim you want to make, do not make it. Accuracy rules are in `.agents/skills/tracking-work-in-linear/references/accuracy.md`, and they apply here without exception.

## Before you finish

- It says what changed, not what the project is.
- The health value follows the rule, and the rule is defensible from the facts listed.
- The target date is either confirmed or explicitly moved.
- No percentages, no issue counts, no activity log.
- Every date is absolute.
- Nothing is claimed that the workspace or the user did not establish.
- If nothing changed, it says so in two lines or does not exist.
