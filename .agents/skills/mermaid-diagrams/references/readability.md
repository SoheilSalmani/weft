# Readability failures

Each entry gives the failure, a test for detecting it, and the fix. Run these against a diagram before it ships.

## Silent fence

**Failure.** The block is tagged `Mermaid`, `mmd`, `diagram`, or nothing at all, so it renders as plain text.

**Test.** Read the opening fence literally. It must be exactly `mermaid`, lowercase.

**Fix.** Correct the tag. There is no error message for this, so it is caught by looking or not at all.

## Separate file

**Failure.** The diagram lives in its own file and is linked from the document.

**Test.** Does any diagram exist outside a Markdown file?

**Fix.** Move it inside the document. A standalone diagram file does not render on GitHub, cannot be referenced from Markdown, and renders nothing when linked raw.

## Wrong type

**Failure.** A sequence of exchanges drawn as a flowchart, or branching logic drawn as a sequence.

**Test.** Ask what question the reader brought. If it is "in what order", the answer is a sequence diagram. If it is "under what condition", it is a flowchart.

**Fix.** Redraw in the type that answers the question. Do not add a second diagram to compensate.

## Too many nodes

**Failure.** The diagram has grown past the point where a reader can see its shape, and they trace lines instead.

**Test.** Count the nodes. Past roughly fifteen, it has stopped being a diagram and become a map.

**Fix.** Raise the level of abstraction until it fits, or split it into two diagrams that each answer one question. Adding `subgraph` grouping helps only if the group count stays small.

## Mixed direction

**Failure.** Hierarchy and flow are drawn in the same diagram, so the layout has to be worked out before the content can be read.

**Test.** Ask what the direction means. If it means two things, that is the failure.

**Fix.** Pick one. Top to bottom for containment and hierarchy, left to right for a pipeline.

## Unlabelled edges

**Failure.** Arrows show that things connect without saying how, which is the least useful thing a diagram can say.

**Test.** Cover the nodes and read the edges alone. If they carry no meaning, they are decoration.

**Fix.** Label each edge with the relationship. If every edge means the same thing, say it once in the surrounding text and leave them bare deliberately.

## Meaning in colour alone

**Failure.** A node is coloured to mark it as different, and nothing in its label says so.

**Test.** Read the diagram as though every node were the same colour. Is anything lost?

**Fix.** Put the distinction in the label. Colour reinforces, never carries. The same page is read in light and dark themes, in print, and by people who do not see those colours apart.

## Sentences as labels

**Failure.** Node labels have become prose, so the diagram is a paragraph with boxes drawn round it.

**Test.** Look for a label longer than about five words.

**Fix.** Shorten the label to a name and move the detail into the text. If the detail cannot be moved, the diagram is doing the wrong job.

## Contradicting the prose

**Failure.** The diagram shows something the surrounding text denies, usually because one was updated and the other was not.

**Test.** List every element in the diagram and find its statement in the text. Anything unmatched is either missing from the text or wrong in the diagram.

**Fix.** Reconcile them. A diagram that contradicts its document is worse than none, because afterwards neither can be trusted.

## Fragile styling

**Failure.** The diagram depends on `linkStyle` with positional edge indexes, so inserting an edge silently restyles the wrong one.

**Test.** Search for styling that refers to an element by number rather than by name.

**Fix.** Use `classDef` and `class` with named nodes. Where an edge genuinely needs to look different, prefer an edge syntax that carries it, such as a dashed `-.->`.

## Unquoted labels

**Failure.** A label contains a space, punctuation, or a reserved word, and the diagram fails to parse.

**Test.** Look for labels that are not quoted, especially any containing `end`, a colon, or brackets.

**Fix.** Quote every label that is not a single bare word. Quoting one that did not need it costs nothing.
