# Labels

## Contents

- The governing rule
- What Linear gives you
- Dimensions worth having
- Dimensions not worth having
- Keeping the set clean

## The governing rule

**Every label names a view or an automation.** Before creating one, say which saved view, filter, or triage rule consumes it. If you cannot, the label is decoration, and decoration in a taxonomy is worse than nothing because it invites more of itself.

A label that is applied to nothing is not a taxonomy. It is a plan for one.

## What Linear gives you

- **Label groups.** Only one label from a group can be applied at once, so a group models a real dimension. Creating a label named `Type/Bug` or `Type:Bug` creates the group and the value together. A prefix convention without a group looks like mutual exclusivity but does not enforce it.
- **Descriptions.** A label can carry a one-line description, shown on hover, that says when to apply it. Triage Intelligence also reads descriptions when suggesting labels. An undescribed label is applied by guesswork.
- **Scope.** Labels live at workspace or team level. Shared vocabulary belongs at workspace level. Team-specific labels with the same name still filter together across teams in the interface, though not through the API.
- **Merge and archive.** Duplicates can be merged. A label that should no longer be applied can be archived, which keeps it on existing issues and out of new ones. Deleting removes it from issues and cannot be undone.
- **Reserved names.** Linear refuses label names that duplicate features, including `priority`, `status`, `state`, `estimate`, `cycle`, `project`, and `assignee`. Treat that list as the principle, not the boundary: any label that restates a property is the same mistake, whether Linear blocks it or not.

## Dimensions worth having

Only when each is backed by a view someone opens.

| Dimension | Justified by | Shape |
| --- | --- | --- |
| Type | Reporting on the mix of work, when anyone actually asks | A group with three to five values, not nine |
| Area | Routing, and filtering a backlog too large to read | A group whose values match how work is actually assigned |
| Source | Distinguishing customer-reported from internal work when it changes prioritisation | A group, usually two or three values |
| Workflow flag | An automation or a delegation gate, such as marking an issue ready for an agent to pick up | A standalone label, not a group |

## Dimensions not worth having

- **Risk or severity as labels.** Priority already carries urgency, and a separate risk axis drifts from it immediately.
- **Anything a property holds.** Status, priority, team, project, milestone, cycle, assignee, dates.
- **Blocked as a label.** A `blocked by` relation says which thing blocks it, which is the part anyone needs.
- **Decision needed as a label**, when the same fact is carried by the status or the assignee.
- **Synonyms.** `Bug` beside `type:bug`, or `docs` beside `documentation`. Merge them the moment they appear.
- **Labels created for a single issue.**

## Keeping the set clean

One person owns the taxonomy. Adding a label requires naming its view.

Review quarterly. For each label, check how many issues carry it and when it was last applied, both shown in label settings. A label with no recent use is either archived or deleted, depending on whether its history is worth keeping.

Prefer archiving over deleting whenever the label has been applied to anything, because deletion silently changes the meaning of closed work.

When the set is already broken, fix it by subtraction first. Merging duplicates and deleting unused labels is cheap and reversible in effect. Migrating prefixes into real label groups is a one-time cost that only pays off once something filters on them, so do it after the set is small, not before.
