# Diagrams in records

Policy only. For syntax, type selection, and readability, use the `mermaid-diagrams` skill.

## When a decision record may carry one

Rarely, and only when the diagram carries the decision rather than the design.

A diagram earns its place in a decision record when it shows **what the decision changes**: the boundary that moves, the dependency that is added or removed, the flow that is redirected. A before and after pair showing a boundary moving is the clearest case.

A diagram of how the system works does not belong in a decision record. That is architecture documentation, and putting it in a record is the Mega-ADR anti-pattern. One diagram is usually the limit; a second is a sign that the record has drifted into describing the system rather than deciding something about it.

If the record would read the same with the diagram removed, remove it.

## When a baseline record may carry one

Freely. Describing structure is the whole job of a baseline, so the restraint above does not apply and several diagrams may be justified.

This is the exception, not the general rule. It exists because a baseline is not a decision record.

## What a diagram may contain

The prose rules apply to diagrams without change.

No file paths, no line counts, no function names, and no commit references. Name components by role, exactly as the text does. A diagram built from paths dates faster than the record around it and pulls the record toward implementation.

No judgement. Do not colour a component to mark it as bad, and do not label it as a problem. A part that exists and is never invoked is drawn as present and labelled as not invoked, which is a fact. A reader can conclude what they like from a fact; they cannot check an opinion.

Nothing in colour alone. A reader may see the record in either theme, in print, or through a screen reader. Every distinction shown by fill or border must also appear in the label.

## Boundaries must be real

A boundary drawn in a diagram is a claim about the system, and readers treat it as one.

Boundaries must accurately represent deployment and process boundaries, internal components, persistence, and external systems. **A diagram must not imply a runtime separation that does not exist.**

This is the most common way an otherwise accurate diagram misleads. Four packages compiled into one process, drawn as four sibling boxes, read as four services. Nothing in the diagram says they are separate, and nothing says they are not, so the reader assumes the shape they can see.

**Detection test.** Name the runtime each box belongs to. Every box sharing a runtime must be visibly inside a boundary representing it. If two boxes sit side by side, a reader is entitled to conclude they can fail, deploy, and scale independently.

Layered dependency structure is worth showing, but it is not a runtime boundary and must not be drawn as one. Show it inside the process, or in a separate diagram that says it is about dependencies.

## Agreement with the prose

Every element in the diagram must have a counterpart in the text, described the same way.

Check this directly rather than assuming it. List the elements, find each one in the prose, and reconcile anything unmatched. An element in the diagram with no statement in the text means one of them is wrong.

A diagram that contradicts its record is worse than no diagram, because afterwards a reader cannot trust either.

## Where a diagram lives

Inside the record, in a fenced block tagged with the lowercase word `mermaid`.

Never in a separate file. A standalone diagram file does not render on GitHub, cannot be referenced from Markdown in a way that renders, and produces nothing when linked directly. A record whose diagram does not render is a record whose diagram does not exist.

Records are immutable once accepted, and a diagram inside one is part of that. It is frozen with the record and is not updated as the system changes.
