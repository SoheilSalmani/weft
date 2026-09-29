---
status: Accepted
date: 2026-09-29
decision-makers: [Soheil Salmani]
---

# ADR-0002: Let an extending template narrow the questions it inherits

## Context

A template can extend a base template. The extender imports the base's questions, includes and patches unchanged, the base's questions come first in answer order, and declaring a question with an inherited identifier is a load error. An extender therefore has no way to change a question it inherits. It cannot give the question a different default, require an answer, or forbid an answer that makes no sense for its stack.

Template families built on one base need exactly that. A base that carries every agent skill, including skills only one stack uses, asks every project about skills that do not concern it, and a stack cannot pre-select its own. Template families avoid this today by copying the base's patches and questions into each stack template and keeping the copies identical with scripts, which duplicates every shared question in every stack.

Presets already express a mandatory value (a lock) and, for a multichoice question, mandatory and forbidden options (fixed and blocked choices). A preset is chosen by the person scaffolding, for one command. It is not recorded in the project, it is not applied again on update, it does not apply to defaults, and its fixed choices replace the question's default instead of joining it. Those properties suit a choice the user makes. They do not suit a rule the template owns.

A scaffolded project stores the answers its user gave separately from the values the template derived. The documented contract is that given answers stay until the user changes them, while derived values follow the template on every update.

Every command and every template frame resolves its answers through the same engine function, so a rule enforced there applies to scaffolding, updating, recording sessions and included templates alike.

## Options considered

### A. A refinement table in the extender

The extender declares, per inherited question, a replacement default, a lock, an allowed or blocked subset of the choices, fixed choices for a multichoice, and replacements for the prompt, description and example. The template loader builds the effective question once, and answer resolution enforces it on every run. This adds a manifest table and a field on the question model.

### B. Presets applied by the extender

The extender names presets that always apply. This reuses the preset format and inherits preset semantics: locked values are recorded as if the user had given them, so a later change in the template cannot reach the project, fixed choices replace defaults, and nothing applies again on update unless the preset is selected again.

### C. Selecting inherited patches

The extender lists the base patches it includes or excludes. This removes content without asking anything, but it breaks patches that depend on an excluded one, and it leaves the person scaffolding no default they can change.

## Decision

The system will let an extending template refine the questions it inherits through a refinement table (option A). The table uses the vocabulary presets already use: a lock for a mandatory value, fixed choices for the mandatory options of a multichoice, and blocked choices for forbidden options, with an allow-list of choices as the alternative to a blocked list.

A refinement can only narrow. It cannot add a choice, change a question's kind, apply to a secret, or undo a narrowing made by a template higher in the extension chain. Anything an extender renders is therefore something its base could render.

A refined default or lock keeps the position of the question it refines, so it may only refer to questions declared before that question.

Answers the user gives are stored as given, including fixed choices that are part of the answer, and the template never rewrites them. A stored answer that falls outside a narrowed question stops the update with a message naming the answer and how to change it, which is what already happens when a template removes a choice.

Option B was not chosen because it records values the template owns as if the user had given them and drops defaults. Option C was not chosen because it cannot express a default the user may change.

## Consequences

Stack templates can extend one base and adapt its questions without copying them. A single base can offer every skill, and each stack can pre-select, require or hide the ones that concern it.

Once a user answers a refined multichoice, the fixed choices included in that answer belong to the user. If the template later stops fixing a choice, projects that answered keep it, and only projects that never answered follow the new default.

A template that tightens a refinement after projects exist can stop their next update until each project changes the affected answer.

A release of the tool from before this decision ignores the refinement table without an error and renders the extender with the base's questions unchanged.

Only questions inherited through extension can be refined. The questions of an included template are still seeded by binds, and the including template cannot lock them.

An extender still cannot order its own hooks before a hook it inherits. Moving a template family onto extension, where each stack's setup must finish before an inherited final hook, needs a separate decision.
