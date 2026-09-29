# Detection tests

Each test is mechanical enough that two reviewers reach the same verdict. Where judgement is needed, the test says what evidence settles it.

## Contents

- Should not exist
- Missed closure
- Stale content
- Unmarked speculation
- Completion conditions
- Structure noise
- Decomposition
- Metadata

## Should not exist

| Test | Finding |
| --- | --- |
| Does anything in this issue need to survive the session that produced it? | If not, it belongs in a pull request |
| Does the issue describe a product outcome with no technical shape? | It belongs in Linear |
| Does another open issue describe the same work? | Duplicate. Close as duplicate, natively, rather than with a comment |
| Was the work already done by a merged pull request that nobody linked? | Close, referencing the pull request |

## Missed closure

| Test | Finding |
| --- | --- |
| `gh pr list --state merged --search "#{n}"`, or the issue's timeline: did a completing pull request merge? | Open issue with merged work. Close, with the pull request named |
| Has the issue had no activity since a pull request closed a related path? | Ask whether it was completed, superseded, or abandoned |
| Is the last comment an abandonment in all but name? | Propose closing as not planned with a one-line reason |
| Did a partial pull request use a closing keyword and close the issue early? | Reopen, and note that partial pull requests use `Part of #n` |

## Stale content

| Test | Finding |
| --- | --- |
| Does a named file, directory, or symbol still exist? | Stale reference. Replace with the current location or remove it |
| Does the body contradict a later comment, a merged pull request, or the code? | Stale body |
| Does a linked pull request or issue still resolve? | Dead link |
| Is a stated constraint still true of the system? | Stale constraint, the most dangerous kind, because work is planned around it |
| Is a branch link used as evidence where a permalink was needed? | Rotting evidence |

## Unmarked speculation

| Test | Finding |
| --- | --- |
| Read every assertion about cause. Was any of it confirmed? | Unmarked hypothesis presented as root cause |
| Does the body name a class, file, or algorithm to create, without a stated constraint requiring it? | Pre-solved implementation. Demote to `Suggested` or remove |
| Are there numbers with no source: a percentage of users, a frequency, a baseline? | Invented metric. Mark unknown or attribute it |
| Does the issue state severity or impact that nobody measured? | Same |

## Completion conditions

| Test | Finding |
| --- | --- |
| Could two engineers disagree about whether this is done? | Missing completion condition |
| Are the acceptance criteria a restatement of the title? | Mechanical criteria. Remove them |
| For a removal or migration, is done expressed as an absence or a count reaching zero? | If not, it will never close |

## Structure noise

| Test | Finding |
| --- | --- |
| Does any heading hold less than a paragraph? | Empty heading |
| Does the first sentence restate the title? | Wasted first sentence |
| Are there headings on an issue under about 200 words? | Structure without content |
| Do `Overview`, `Background`, or `Context` appear with nothing the rest does not say? | Template residue |
| Is more than half the body raw log, trace, or output? | Evidence has become the issue. Collapse it or move it to a comment |
| Are there more than two alerts? | Alert fatigue. GitHub advises one or two |

## Decomposition

| Test | Finding |
| --- | --- |
| Could each sub-issue be a pull request someone reviews on its own? | If not, it is a checklist item |
| How many children does the parent have? | More than about seven means it is a project, and belongs in Linear |
| How deep is the nesting? | More than two levels means the middle layer carries nothing |
| For each `blocked by`: would removing it change what someone works on today? | If not, it is a reference, not a dependency |
| Is there a `blocked by` between a parent and its own child? | Redundant. The hierarchy already says it |
| Is a cross-repository dependency expressed as a relationship? | Not supported by the platform. It must be a sub-issue or a linked sentence |

## Metadata

| Test | Finding |
| --- | --- |
| For each label, name the filter, routing decision, or automation it feeds | No answer means delete it |
| Do two labels mean the same thing? | Synonyms. Merge |
| Does a label encode status that state or the linked pull request already carries? | Delete |
| Does a label restate the repository, the issue type, or the priority? | Delete. Priority lives in Linear |
| Does a label duplicate an issue field that already exists, such as component or effort? | Same error as duplicating a property. The field wins, because it is one value per issue |
| Do several labels cover one dimension, such as P0, P1, urgent, and priority-high? | One dimension in four places. Collapse to the field, or to nothing |
| Does a milestone track a product plan rather than a release or version boundary? | It duplicates Linear |
| Does a GitHub Project mirror a Linear project? | It duplicates Linear. The only sanctioned use is a temporary hierarchy view for a large migration |

## Templates and forms

Only when the repository has them. Configuration is changed deliberately, so these are findings to report, never edits to make in passing.

| Test | Finding |
| --- | --- |
| Does a required field produce `N/A` on real issues? | It is asking for something that is not always true. Make it optional or delete it |
| Do two fields ask for the same thing, such as context, background, and motivation? | Collapse to one, or none |
| Does the form ask for metadata that a native field or type already carries? | Delete the field and use the native one |
| Does every archetype use the same form? | One minimal form, or none, beats one form per archetype in an internal repository |
| Is there a required acknowledgment checkbox with no safety or governance purpose? | Delete it |
| Is there a contact link routing security reports to private reporting? | If not, that is the one form artefact worth adding |
