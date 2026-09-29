---
name: responding-to-review
description: Handles the conversation around a pull request review. Triages incoming feedback into changes, disagreements, questions, and out-of-scope follow-ups, keeps the description current when review changes the implementation, and shapes review comments so they say why. Use when addressing or replying to review feedback, or when deciding how to phrase and place comments. To find bugs in a diff, use the built-in /code-review instead.
---

# Responding to review

Two failure modes bracket this work. An agent that agrees with every comment implements bad suggestions and lets a two-file change grow into a twelve-file one. An agent that replies `Fixed.` to everything leaves a thread nobody can learn from.

Finding problems in code is not this skill's job. `/code-review` reviews a diff, a branch, or a pull request as a background subagent and can post findings inline. Use it rather than re-implementing it. This skill covers the conversation around the review.

## Triage every comment before acting

Four responses. Never default to the first.

| Response | When | What it looks like |
| --- | --- | --- |
| Agree and change | The concern is valid and in scope | Make the change, then reply only if the diff does not speak for itself |
| Disagree with a reason | The suggestion is wrong, or costs more than it saves | State the reason once, concretely. Do not argue twice |
| Defer as out of scope | Valid, but separate work | Propose a linked issue, then resolve the thread |
| Clarify | The comment rests on a misreading | Explain, and consider whether the code caused the misreading |

**Verify before you agree.** A reviewer can be wrong about what the code does. Read the surrounding code, confirm the claimed behaviour, and check whether automation already covers it. Implementing an incorrect suggestion because it arrived as feedback is worse than pushing back.

**Check scope before you agree.** "While you are here" requests are how pull requests double in size. Valid separate work becomes a GitHub issue through `writing-github-issues`, linked from the thread. Trivial fixes that would take less time than filing anything just get done.

## When the code was unclear

If a reviewer misread the code and your reply explains it, the explanation belongs in the code or a comment, not only in the thread. A future reader will hit the same confusion without the thread in front of them.

## After changing code

1. Rerun the verification that the change affects, and update the verification line to what you actually ran.
2. Update the description if the change altered the canonical explanation. If review replaced approach A with C, the body describes C. The thread holds the chronology.
3. Update the title if the change altered what the pull request means.
4. Resolve threads that are genuinely settled, not to tidy the page.

Use `writing-pull-requests` for the description rules rather than reinventing them.

## Replying well

Reply when it adds information: what you did instead and why, why you disagree, what you deferred and where it went.

Do not reply when the pushed diff answers the comment completely. `Fixed.` on a thread whose fix is visible in the next commit is noise, and so is a paragraph restating the change.

## Writing review comments

When leaving comments on a change, use the prefix convention rather than inventing severity labels:

| Prefix | Meaning |
| --- | --- |
| (none) | Should be addressed before merge |
| `Nit:` | Minor, non-blocking |
| `Optional:` or `Consider:` | A real idea, the author's call |
| `FYI:` | Not for this change |

Comment on the code, never on the author. Explain why when the reason is not obvious, since a comment that only says what to change teaches nothing. Point at the problem rather than writing the fix; it is the author's change to fix, and their decision how.

Use a line comment for a local issue, a review summary for a cross-cutting concern, and a suggested change only when the exact replacement is small and certain.

**The default is not to comment.** A comment costs the author a read and often a reply, so it has to carry something they did not already know. `if (user == null) return;` needs no comment saying it returns when the user is null. Say nothing.

When something is worth raising, name the consequence rather than the observation: not "this check happens twice", but "the second check reads a value the first may have invalidated, so a concurrent delete slips through". Place it on the line when it is local, and in the review summary when it spans files.

Do not paraphrase the code back, generate praise at scale, raise style that a formatter already enforces, or claim certainty you do not have. If you are unsure whether something is a bug, ask about it as a question rather than asserting it.

## Mutation safety

| Act | Default |
| --- | --- |
| Read the pull request, its diff, and its threads | Free |
| Draft replies or comments | Free |
| Post comments, or resolve threads | Ask |
| Push changes to the branch | Follow the authorisation for the code change itself |
| Approve, request changes, merge, or close | Never without an explicit instruction |

Approving a pull request is a statement that a human is accountable for. An agent does not make it.

## Before you finish

- Every comment you acted on was verified against the code first.
- Nothing out of scope was implemented in this pull request.
- Deferred work is captured somewhere, not lost in a resolved thread.
- Verification was rerun and the line updated.
- The description matches the current head.
- No thread was resolved without being settled.
- Nothing was posted, pushed, approved, or merged without approval.
