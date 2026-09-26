# The research canon

Load this when a judgement call is genuinely unclear. The condensed rules in `SKILL.md` are the
operational form of what follows.

Sources:

- Piotr Wozniak, [Twenty rules of formulating knowledge](https://www.supermemo.com/en/blog/twenty-rules-of-formulating-knowledge), SuperMemo.
- Andy Matuschak, [How to write good prompts: using spaced repetition to create understanding](https://andymatuschak.org/prompts/).
- Andy Matuschak, [Important attributes of good spaced repetition memory prompts](https://notes.andymatuschak.org/z42J1vxsMjhkdbrqVfoqjiEesSzfaEqurBtoJ).

## The five attributes

Matuschak's test for a prompt. Run a doubtful card against all five.

1. Focused. One detail. Too much detail in either the question or the answer produces a partial
   retrieval, which is a partial memory.
2. Precise. The question specifies exactly what is being asked for. Vague questions produce vague
   answers that do not reinforce anything in particular.
3. Consistent. The same answer every time. A prompt with several valid answers causes
   retrieval-induced forgetting: recalling one answer actively inhibits the others.
4. Tractable. You should be right about ninety percent of the time. A card you keep failing is not
   teaching you, it is churning.
5. Effortful. Retrieval, not inference. If the answer follows from the question by reasoning, you are
   practising reasoning and not building the memory.

Consistent and effortful pull against each other, and that tension is where most bad cards live. Add
enough context to force one answer, but not so much that the question's shape becomes the cue.

## Minimum information principle (SuperMemo 4)

Formulate the smallest piece of knowledge that stands on its own. Simple items get scheduled well by
the algorithm because performance on them is stable. Complex items produce partial recall, which the
scheduler cannot interpret, so they need redundant repetition at awkward intervals.

The canonical example is a "characteristics of the Dead Sea" card carrying location, depth, length,
salinity, and density at once. It becomes five cards, each of which the scheduler can then handle
independently, dropping the easy ones fast and drilling the hard one.

Practical consequence: cards are cheap. Matuschak puts the lifetime cost of a prompt at ten to thirty
seconds of review across its first year. Splitting one bad card into three good ones is close to free.
Write more cards than feels necessary, but only about material you actually care about.

## Sets, enumerations, and closed lists (SuperMemo 9 and 10)

Three distinct cases, three different treatments.

A set is an unordered group: "which countries are in the EU". Never card it directly. Listing members
in a different order each review has, in Wozniak's words, a disastrous effect on memory. Convert to
individual membership facts, or to the historical sequence that produced the set, or drop it.

An enumeration is ordered: steps in a process, a pipeline, a gate sequence. Better than a set because
the order forces a consistent traversal, but still hard. Use overlapping cloze deletions so each card
asks for one step in context rather than the whole chain.

A closed list has a fixed, small, known membership: the five R&D pillars, the four criteria for
selecting a country. Write one cloze card per member on a single note, so each member is retrieved
separately with the others visible as context. That is the one case where a multi-index cloze note is
the right answer.

An open list has no fixed membership: "examples of a design pattern", "functions this library
exposes". Do not write a fill-in-the-blank card. Instead write cards linking each instance to the
category, cards about the pattern the instances share, and a generative card asking for any example.

## Interference (SuperMemo 11)

Wozniak calls interference probably the single greatest cause of forgetting in a mature collection.
It is the failure mode of carding a source that presents many parallel items, which is exactly what
an org chart, an acronym glossary, or an API surface looks like.

Symptoms: you recall that there are six committees but cannot attach the right mandate to the right
acronym, and each review reinforces the confusion.

Countermeasures, in order of preference:

1. Card fewer of them. Six near-identical facts you half-remember are worth less than two you know.
2. Differentiate by content. Ask for the distinguishing property, not the label. "Which committee
   makes major investment decisions" separates cleanly from "which committee evaluates benefit and
   risk", where "what does IDCC stand for" and "what does BRAC stand for" do not.
3. Add a personal or concrete hook (rules 14 and 15), such as the one decision you actually saw a
   committee make.
4. Space the introduction. Card two today and two next week rather than six at once.

## Optimize wording (SuperMemo 12) and context cues (SuperMemo 16)

Every word in a card is read on every review, so redundant words have a recurring cost and no
benefit. The SuperMemo example compresses "What does GRE stand for in biochemistry?" to
`bioch: GRE`, moving the domain out of the question and into a context label.

This collection's note types implement that label as the `Topic` and `Tool` fields. Using them is not
optional politeness, it is the mechanism that lets Fronts stay short without becoming ambiguous.

## Legitimate redundancy (SuperMemo 17)

Redundancy does not contradict the minimum information principle when it encodes the same fact from a
genuinely different angle:

- Both directions of a pairing. Term to definition and definition to term. Use `c1` and `c2` on one
  cloze note.
- An explanation card alongside a fact card. What it is, and why it is that way.
- Intermediate steps of a derivation as their own cards.
- The same concept in a different representation, for example a formula and a graph.

Two cards asking the same question in slightly different words is not redundancy, it is waste, and it
creates the inconsistency problem from attribute 3.

## Prompt types beyond bare recall

Recall cards are the floor, not the ceiling. When the material is conceptual rather than factual,
reach for:

- Explanation. Why does this hold, how does the mechanism work.
- Application. Given this situation, what does this concept tell you to do.
- Connection. How does this relate to, differ from, or generalise something already known.
- Creative or generative. Produce a new example. The answer varies, so accept anything valid. Judge
  these against attribute 4 rather than 3.
- Salience. Keep an idea available at the moment it is relevant, by tying it to a decision you make.

## How much to card

Start at five to ten cards per article or chapter. Completionism is the failure mode: it drains the
motivation that the whole system depends on, and produces cards about material you do not care about,
which you will then fail and resent.

It is hard to write good cards on first exposure to unfamiliar material. For a difficult source, card
the basic factual scaffolding first, and write the conceptual cards on a second pass once you
understand it.

Delete cards you no longer care about. A card that survives only out of sunk cost is a tax.

## Sources and date stamps (SuperMemo 18 and 19)

Record where a fact came from, so you can verify it, judge its reliability, and update it when it
changes. Stamp anything volatile with a date or version: statistics with the year collected, software
behaviour with the version, org structures and prices with the date read.

In this collection, the source goes in a tag and the date goes in the visible card text, because tags
are not shown during review. See `house-style.md`.
