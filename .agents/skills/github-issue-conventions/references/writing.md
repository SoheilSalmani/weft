# Writing a technical issue

## Contents

- Titles
- Body shape
- Formatting
- Referring to code
- Comments
- After closure

## Titles

A title is read where nothing else is: issue lists, notifications, search results, pull request bodies, branch names, and agent tool results. In all of those the metadata is invisible, so the title carries the work alone.

- **No type prefixes.** `[BUG]`, `[TECH]`, and `[REFACTOR]` consume the characters that survive truncation, and they duplicate either the issue type or the title's own grammar.
- **Name the system, not the file.** `webhook retry handling` beats `webhook.ts`.
- **Sentence case, no trailing full stop, no em dashes.**
- **Roughly 50 to 70 characters**, with the distinguishing word early.

Three grammatical forms, chosen by what is actually known:

| Form | Use when | Example |
| --- | --- | --- |
| Imperative | The change is known | `Replace process-local locks with a distributed lease abstraction` |
| Declarative | The fix is not yet known | `Duplicate webhook processing during concurrent retries` |
| Interrogative | The deliverable is an answer | `Can the legacy session-token path be removed without breaking mobile clients?` |

Do not force the imperative onto a problem whose cause is unknown. `Fix the webhook race` presumes a diagnosis, and the title outlives the presumption.

**A title that already states the objective is finished.** Do not lengthen it to match the pattern, and do not add the component, the system, or a qualifier that the body already carries. Making a good title longer makes it worse, because the extra words push the distinguishing ones past where lists and notifications truncate.

| Weak | Why | Better |
| --- | --- | --- |
| `Auth` | A topic asserts nothing and never finishes | `Reject expired refresh tokens at the gateway` |
| `Cleanup` | No object, no cost, no completion | `Remove the unused Docker sandbox and its configuration` |
| `Refactor` | The category, not the change | `Extract token validation so the mobile client can reuse it` |
| `Fix bug` | Zero information anywhere it appears | `Retry after a failed charge submits the payment twice` |
| `Improve tests` | Unfalsifiable | `Cover failed OAuth refreshes in integration tests` |
| `Backend changes` | True of most issues in the repository | Name the change |

## Body shape

Default: **no headings, three to eight lines.**

1. What is wrong, or what must change.
2. Why now, in engineering terms, if it is not obvious.
3. Constraints, marked `Must`, and only real ones.
4. How we know it is done. One to three observable conditions.
5. Links: the Linear item, an ADR, a prior pull request, evidence.

Headings appear above roughly 200 words, or when there are genuinely three or more sections worth separating. Choose headings for cognitive value, never from a list. `Problem`, `Evidence`, `Constraints`, `Desired state`, `Open questions`, `Verification`, and `References` are all available and none is required.

Never write a heading that will hold one line. Never add `Overview`, `Background`, `Context`, or `Summary` because an issue looked bare without them.

A three-line issue is finished at three lines. Do not expand it.

## Formatting

| Element | Convention |
| --- | --- |
| First sentence | The objective or the observed failure. Never a restatement of the title |
| Alerts | At most one, for a real hazard such as data loss or an irreversible step. GitHub's own guidance is one or two maximum |
| Collapsed sections | `<details>` for any evidence over about fifteen lines: traces, logs, query plans, long output |
| Code blocks | Always tagged with a language. Excerpts only |
| Task lists | `- [ ]` for a completion checklist or steps inside one pull request. Never for work that deserves its own issue |
| Tables | For two dimensions: options against criteria, environments against behaviour |
| Dates | Absolute and ISO, `2026-08-13` |
| Numbers | With their source and how they were measured |

Tasklist blocks, the special code fence from the old private preview, were retired during 2025 and now render as raw Markdown. Sub-issues replaced them. Ordinary task lists were never deprecated and are fine.

## Referring to code

GitHub is the technical layer, so code references are welcome. They still rot, so choose the form by what the reference is doing.

| Purpose | Form | Why |
| --- | --- | --- |
| Evidence for a claim | Permalink with the commit SHA | It cannot drift, and it renders as a real code snippet in a comment |
| A starting point | Plain path, `src/billing/webhook.ts` | Cheap, and still useful as a hint when it is slightly wrong |
| A symbol worth naming | Backticked name, `verifySignature` | Survives file moves |
| Another issue | `#123`, or `owner/repo#123` across repositories | Native, bidirectional |
| A specific change | Commit SHA | Immutable |

A permalink renders as a code snippet only in the repository it came from, and only in comments, not in Markdown files. A branch link points at whatever the branch later becomes, so it is the wrong tool for evidence.

**Never enumerate every file the work might touch.** A list produced by a shallow search is stale on day two, and it pre-solves the problem by fixing the implementation's shape before anyone has read the code. One to three starting points is the useful amount.

If an issue states exact files as requirements, check whether they are genuinely constraints, such as a public entry point that must keep its path, or merely the likely place the work lands. Demote the second kind to `Suggested`.

## Comments

Comments carry dated findings. Write one when it contains information someone else can act on:

- A reproduction discovered, or a reliable one found for an intermittent failure.
- A hypothesis disproven, which is worth as much as one confirmed.
- A benchmark result, with its method.
- A newly discovered constraint.
- A blocker identified, with what would clear it.
- An investigation conclusion.
- A handoff, saying where the work stands and what the next person needs.

Do not write `Starting now`, `Still working on this`, `Made progress`, or a summary of the issue that adds nothing. An agent narrating its own activity is noise for every future reader.

A comment should become something else when it changes the work:

| The comment | Becomes |
| --- | --- |
| Changes the objective, constraints, or completion conditions | An edit to the body |
| Describes separable work | A new issue |
| Settles something durable and architectural | An ADR, using `writing-adrs` |
| Changes product-visible scope or timing | A Linear update |

Leave the comment in place when it is promoted. It is the dated record of when the thing became true.

## After closure

A closed issue should still say what the work was and why. Do not rewrite it into a description of what the code became: the pull request already holds that, and the ADR holds the durable rationale.

Two edits are worth making at closure:

- **The premise was invalidated.** Add a closing comment saying what was learned, and close as not planned.
- **Scope shrank.** Edit the body to the delivered scope and open a follow-up, rather than leaving a body that half describes work nobody did.

A future engineer should learn the problem and its constraints from the issue, what changed from the pull request, and why the system is shaped this way from the ADR. Keeping those three distinct is most of the value of the arrangement.
