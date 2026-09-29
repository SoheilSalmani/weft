# Detection tests

Each test is mechanical enough that two reviewers reach the same verdict. Where a test needs judgement, it says what evidence settles it.

## Contents

- Wrong object
- Titles
- Stale content
- Misplaced detail
- Structure
- Labels
- Milestones
- Updates

## Wrong object

| Test | Finding |
| --- | --- |
| Name the day the project finishes. If no answer exists in principle, not merely today | It is not a project. It is an area, a view, or ongoing work |
| Would you open one pull request for this issue? If it needs several by different people | It is a parent, or it is a project |
| Would each sub-issue make sense in a list without its parent's title? If not | It is a checklist wearing sub-issues |
| Can a filter produce the membership of this initiative? If yes, and no tradeoffs are made across its projects | It is a view |
| Does the issue describe a code change with no stated consequence outside the codebase? | It belongs in a pull request |

## Titles

| Test | Finding |
| --- | --- |
| Read the title alone, with no properties visible. Can you say what will be different when it is done? | Vague |
| Does the title contain a type, priority, status, team, component tag, or date? | Restates a property |
| Does it contain "and", or two verbs with different objects? | Two objects |
| Is it a bare noun or noun phrase for an issue or project? | A topic, not a change |
| Is it an outcome-shaped object named after a mechanism that might not achieve the outcome? | Mechanism title. Correct only when delivering the mechanism is the point |
| Does a bug title assert a cause or a fix? | Presumes the diagnosis |

## Stale content

| Test | Finding |
| --- | --- |
| Does the description contradict a later comment, a status, or the current code? | Stale |
| Is an open question listed that has since been answered anywhere? | Answered, still listed |
| Do acceptance criteria refer to scope that was dropped? | Stale criteria |
| Do links resolve? | Dead link |
| Does the description contain "we assume", "for now", or "to be confirmed" written more than a month ago? | Unverified assumption, ageing |
| Is the project description describing where the work stands? | Status in the wrong place |

## Misplaced detail

| Test | Finding |
| --- | --- |
| Would this line be false after the pull request merges? | Belongs in the pull request |
| Does the object contain SQL, a full stack trace, test code, or a migration script? | Route out. Keep only the existence, risk, and reversibility |
| Does it name a file or symbol, and is the issue not delegated to an agent and not about that file as a deliverable? | Route out |
| Is a decision that is expensive to reverse recorded only in a comment? | Needs an ADR, using `writing-adrs` |
| Does a comment thread contain the reasoning for the current shape of the system? | Promote a decision note, and write an ADR if architectural |
| Is a paragraph of investigation notes in a description rather than a comment? | Wrong place, even though the content is fine |

## Structure

| Test | Finding |
| --- | --- |
| Does any heading hold less than a paragraph? | Empty heading |
| Does the first sentence restate the title? | Wasted first sentence |
| Are there headings on an object under about 150 words? | Structure without content |
| Do `Context`, `Overview`, `Summary`, or `Next steps` appear with nothing under them that the rest does not already say? | Template residue |
| Is the same fact stated in the summary, the description, and the latest update? | Duplication. Keep it in the one place its reader looks |
| Is the project summary empty, a fragment, or describing a different project? | Broken summary, the most-read field in the workspace |

## Labels

| Test | Finding |
| --- | --- |
| Name the saved view or automation that consumes this label | If you cannot, delete or archive it |
| How many open issues carry it, and when was it last applied? | Unused |
| Does another label mean the same thing, in different words or casing? | Duplicate, merge |
| Does it restate a property? | Delete |
| Do several labels share a prefix without being a group? | Mutual exclusivity is unenforced. Consider a group, but only after the set is small |
| Does it have a description? | Undescribed labels are applied by guesswork |

## Milestones

| Test | Finding |
| --- | --- |
| Can the name be true? Read it as a sentence: "X is now true" | If not, it is a phase label |
| Does it name a team, a discipline, or a layer? | Not an outcome |
| Is it numbered rather than named? | Carries no information |
| Does its progress bar mean anything to someone outside the team? | If not, it is internal bookkeeping |

## Updates

| Test | Finding |
| --- | --- |
| Remove every sentence derivable from the issue list and the project description. Is anything left? | Manufactured update |
| Does it state percentages or issue counts? | Duplicates the generated progress report |
| Does the health value match the rule in `writing-linear-updates`? | Health by feel |
| Is a blocker described, with health `onTrack`? | Contradiction |
| Is the target date unaddressed while the update reports a slip? | Missing the sentence readers need |
| When was the last update, and does the project have a schedule? | Overdue, though Linear flags this itself |
