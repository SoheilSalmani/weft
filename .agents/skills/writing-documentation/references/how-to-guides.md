# How-to guides

A how-to guide takes a competent reader who already knows what they want and gets them there.

It is a recipe, not a lesson. The reader can cook. They do not need to be taught what a pan is, and telling them will insult them and waste the time they came to save.

This is wholly distinct from a tutorial, and conflating the two causes more damage than any other confusion in documentation. A tutorial serves a reader who does not yet know what they want. A how-to guide serves one who does.

## Rules

**Write from the reader's problem, not the machinery.** Real problems are the organising principle. "How to recover a corrupted database" is a how-to guide. "Using the repair command" is reference material with an instruction stapled to it.

**Action and only action.** No teaching, no digression, no history. Everything else dilutes the guide and costs the reader time.

**Sequence carries meaning.** The order is part of the answer. Present steps in the order the reader must take them. Match how the work actually flows, not how the system is structured.

**Be complete about the goal, not about the machinery.** A how-to guide need not cover every option. It must get the reader to the outcome. Link to reference for the full set.

**Handle the real world.** Readers arrive with variations you did not imagine. Use conditional imperatives so a reader can find their case: `If you are using a managed instance, do x instead.`

**Leave out what does not serve the goal.** A step that is merely interesting is a step that costs time and attention.

## Titles

Name the goal, and start with `How to`.

| Title | Verdict |
| --- | --- |
| `How to integrate performance monitoring` | Good. States the goal and promises directions. |
| `Integrating performance monitoring` | Ambiguous. Directions, or an essay about integration? |
| `Performance monitoring` | Useless. Could be any of the four modes. |

A reader scanning a list of guides is matching titles against the problem in their head. The closer the title is to how they would phrase their problem, the faster they find it.

## Language

- `This guide shows you how to integrate performance monitoring.`
- `To achieve w, do z.`
- `If you want x, do y.`
- `Refer to the configuration reference for the full list of options.`

## Detection tests

**Does it teach?** Look for sentences explaining what something is or why it works. Move them to explanation and link.

**Is it organised by the machinery?** Check whether the headings name features or name goals. Features mean it has drifted into reference.

**Would a beginner be lost?** Good. That is the correct audience boundary. Send beginners to a tutorial rather than widening the guide to hold both.

**Is it trying to be exhaustive?** Count the options presented. A guide listing every flag has become reference material and has stopped answering the question it was named after.

**Can the reader tell when they are done?** A how-to guide that does not state the finished condition leaves the reader unsure whether it worked.
