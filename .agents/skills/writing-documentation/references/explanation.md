# Explanation

Explanation gives the reader understanding. It is discussion, and it permits reflection.

The reader is not at a keyboard. They are trying to grasp why something is the way it is, and they have time to think. This is the mode read away from the work, and often before or long after it.

## Why it is the hardest mode

The other three modes have natural edges. A tutorial ends when the lesson is learned, a how-to guide ends when the goal is reached, and reference is bounded by the product itself.

Explanation has no such boundary. Any subject connects to every other, and there is always more context that could be given. The writer has to draw the line themselves. That is why explanation is the mode most often left unwritten, and the mode most likely to sprawl once started.

Draw the boundary deliberately, and say what it is. A reader told what a document does not cover is better served than one who discovers it by absence.

## Rules

**Make connections.** Explanation is where separate things are related to one another. This is its distinctive value and cannot be provided by any other mode.

**Give the background.** Why the design is this way. What constraints applied. What was tried. What the alternatives were and why they were not chosen.

**Admit the alternatives.** An explanation that presents the current design as the only possibility teaches the reader nothing, because it gives them no way to reason about a change.

**Judgements are allowed here, and only here.** Explanation may say one approach is better than another and why. Reference may not. A how-to guide may not.

**Stay bounded.** Do not absorb instructions or specifications. The pull is strong, because explaining something naturally raises how to do it. Link instead.

## Titles

Prefix with `About`, or write a title that would make sense with `About` in front of it.

`About the caching strategy`. `About authentication`. `Why runs are immutable`.

The prefix is a test as much as a convention. If `About` does not fit in front of the title, the document is probably not explanation.

## Language

- `The reason for this is historical. The original implementation had to ...`
- `An x interacts with y as follows.`
- `This approach is better than the alternative because ...`
- `It is worth understanding that ...`
- `For instructions, see ...`

## Detection tests

**Are there instructions?** Search for imperative verbs at the start of sentences: `run`, `set`, `create`, `install`. Each one is a how-to guide leaking in.

**Are there specifications?** Tables of options and exhaustive lists belong in reference.

**Does it have a boundary?** Say what the document covers and what it does not. If you cannot state the boundary, the document has not been scoped and will sprawl.

**Does it explain, or only describe?** Description is reference. Explanation answers why. If no sentence answers why, this is misfiled.

**Would this help a reader disagree with the design?** Good explanation equips a reader to challenge a decision rather than accept it. If it reads as justification only, it has become advocacy.
