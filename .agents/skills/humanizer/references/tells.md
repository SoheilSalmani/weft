# The tells

## Contents

- How to use this
- Antithesis
- Synonym triads
- Hedging stacks
- Inflated diction
- Weightless intensifiers
- Openers and affirmations
- Decorative structure
- Closing restatement
- Typographic drift
- Emoji
- Density

## How to use this

Each class below has a cut example and a keep example. The keep column is the point. A construction on this list is a candidate, not a verdict, and the em dash test in `SKILL.md` generalises: substitute the plain version, reread, and keep whichever is better.

## Antithesis

The "not just X, it's Y" frame, and its relatives: "not merely", "far from being", "rather than simply". It manufactures contrast where none was needed, and it always inflates the second half.

Cut, because the contrast is invented:

> This is not just a performance fix, it is a correctness fix.

> This fixes a correctness bug, not a performance one.

Keep, because the contrast is the actual finding:

> The tests were not merely incomplete, they asserted the opposite of the rule.

## Synonym triads

Three items where the third is a synonym of the second, or where two would do. Real lists of three are common and fine. Padded ones have interchangeable members.

Cut:

> The message should be clear, concise and readable.

> The message should be short and clear.

Keep, because the three items are distinct and each is load-bearing:

> Locked projects keep their gate, their phase and their scored rows.

## Hedging stacks

Phrases that occupy a sentence without adding to it: "it is worth noting that", "it is important to remember", "that said", "it should be mentioned", "in many cases". Usually deletable outright, leaving the clause behind.

Cut:

> It is worth noting that the guard was already wrong before this change.

> The guard was already wrong before this change.

Keep, because the uncertainty is real and quantified:

> In the three environments checked, no row was affected; the others were not checked.

A hedge that names what is unknown is honest. A hedge that softens a claim the author is confident about is padding. When you find the second kind wrapped around a claim you cannot verify, the fix is to state the limit, not to delete the words and inherit false confidence.

## Inflated diction

Words that arrive with generated prose and rarely survive editing: delve, leverage, utilise, facilitate, robust, seamless, holistic, comprehensive, myriad, plethora, furthermore, moreover, additionally, crucially. Each has a plain equivalent that is almost always better.

| Inflated | Plain |
| --- | --- |
| leverage, utilise | use |
| facilitate | let, help, allow |
| delve into | read, examine |
| robust, comprehensive | name the actual property: tested, complete, handles nulls |
| seamless | say what does not break |
| furthermore, moreover, additionally | and, or nothing at all |
| in order to | to |
| prior to | before |
| a number of, various | the number, or the actual list |

Keep a word from this list when it is the domain's own term. "Robust" in a statistics context, or "comprehensive" in a coverage report, is vocabulary rather than inflation.

## Weightless intensifiers

Truly, genuinely, incredibly, remarkably, quite, very, really, significantly, substantially. They assert emphasis instead of earning it. Delete, or replace with the number.

Cut:

> Performance improved significantly.

> The refresh dropped from 40 seconds to 6.

## Openers and affirmations

"Great question", "Absolutely", "You are right to ask", "Certainly", "I would be happy to". In a document they are noise. In a reply they are sycophancy. Delete and start with the answer.

Related: opening a paragraph by restating the question before answering it.

## Decorative structure

Structure earns its place by making the text easier to navigate. Applied to short text it makes it harder.

- **Headings on a three-paragraph document.** Cut them; three paragraphs need no map.
- **Bullets where two sentences work.** Prose carries connective logic that bullets throw away. Reach for a list when the items are genuinely parallel and unordered.
- **Bold on every other phrase.** When everything is emphasised, nothing is. Two or three per screen.
- **A table with two rows and one real column.** Write the sentence.
- **Nested bullets three deep.** The nesting is doing the thinking the prose should.

Keep structure when the reader will scan rather than read, or will return to find one item. Reference material, option comparisons and checklists all earn it.

## Closing restatement

A final paragraph that repeats the opening in different words, often signalled by "in summary", "overall", "ultimately", or "in conclusion". A reader who reached the end does not need the beginning again.

Delete it. If the piece genuinely needs a conclusion, it says something the body did not: a recommendation, a consequence, or the next action.

## Typographic drift

Curly quotes and apostrophes, non-breaking spaces, and en dashes appearing where the surrounding text uses straight ASCII. Usually an artefact of generation rather than a choice. Match the file: code, commit messages and most plain-text formats use straight quotes.

This is visible punctuation, so it belongs here. Zero-width and other invisible characters are `ai-provenance-marks`.

## Emoji

As decoration in technical prose, cut. As content, when the text is about emoji, or in a context whose house style uses them, keep. Never in a commit message, a code comment, or an error string.

## Density

The strongest signal is not any single construction, it is how many appear together. One triad is a sentence. A triad, a hedge stack and an antithesis in the same paragraph is a register, and the paragraph usually wants rewriting from its point rather than repairing clause by clause.

When you find that, say so in the report. "Rewrote the second paragraph rather than editing it" is a more honest description than a list of six small cuts.
