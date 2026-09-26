# Style, formatting, and accessibility

Diátaxis decides what a document is. These rules decide whether it can be read. They apply in every mode.

Drawn from the Google developer documentation style guide, its accessibility guidance, and plain language practice.

## Voice

**Second person.** Address the reader as `you`. The exception is a tutorial, where `we` carries the relationship between teacher and learner.

**Present tense.** `The command returns a list`, not `will return`. Documentation describes a system that exists now.

**Active voice.** Name what performs the action. `The server rejects the request` says who; `the request is rejected` leaves the reader guessing.

**Conversational, not frivolous.** Jokes date, translate badly, and cost the reader time when they are stuck.

**No hedging.** `The command fails` is useful. `The command may sometimes potentially fail` is not.

## Sentences

**Put the condition first.** `To enable caching, set the flag.` A reader who does not want caching stops at the comma. Reversed, they read the whole sentence before learning it does not apply to them.

**Keep sentences under about 26 words.** Longer sentences are where ambiguity hides.

**Avoid double negatives.** `Not uncommon` costs the reader a moment for no gain.

**Define an acronym on first use**, then use it consistently. Do not alternate between the acronym and the expansion.

**Prefer the simple word.** `Use` over `utilise`, `about` over `regarding`, `before` over `prior to`. Simple words are not less precise, and they survive translation.

## Structure

**Sentence case for titles and headings.**

**One level-one heading per page.** Do not skip heading levels. Headings are how a reader scans and how a screen reader navigates.

**Headings must be descriptive and distinct.** Three sections called `Overview` help nobody.

**Never open a section with a subsection.** A heading followed immediately by another heading gives the reader nothing to confirm they are in the right place, and someone navigating by heading lands on an empty stop.

The text in between must state a fact about the subject that frames what follows: what unites the subsections, or what the reader should carry into them. Do not restate the subheadings, and do not describe the document's own arrangement. `This section covers installation, configuration, and deployment` is the table of contents written twice.

If the only sentence available restates the subheadings, that is a finding rather than a formatting problem. It means the grouping is arbitrary, and the fix is to change the grouping.

The document title is not a section, so it may be followed directly by the first heading. The rule applies from the first real section down.

**Numbered lists for sequences, bulleted lists for everything else.** Numbering implies order, and using it where order does not matter misleads.

**Introduce a table before it appears.** A reader meeting a table cold has to infer what it compares.

**Do not use a table where a list would do.** Tables are hard to read on narrow screens and hard to hear.

## Links

**Describe the destination.** Never `click here`, `this page`, or a bare URL. Link text is read out of context by anyone scanning, and screen readers can list every link on a page with no surrounding sentence.

**Warn about unexpected behaviour.** Say when a link downloads a file or leaves the documentation.

**Separate adjacent links** with a character between them so they do not read as one.

## Code

**Code in code font**, including inline references to commands, flags, file names, and values.

**Never show code as an image.** It cannot be copied, searched, or read aloud, and it is unreadable when zoomed.

**Show the expected output** where a reader needs to confirm success.

**Keep examples runnable.** An example that has drifted from the product is worse than no example, because a reader will trust it.

**No placeholder that looks real.** Make substitution obvious, so a reader cannot paste a fake value and wonder why it fails.

## Accessibility

**Alt text on every image**, describing what the image conveys. Decorative images take empty alt text.

**Never put information only in an image.** Give the equivalent in text. This also serves the reader on a slow connection and the one searching the page.

**Never rely on colour alone.** Add a label, a shape, or a word. The page is read in light and dark themes, in print, and by people who do not distinguish those colours.

**No directional language.** `Above`, `below`, and `on the left` break when the layout reflows, when the content is translated, and when the page is heard rather than seen. Say `earlier`, `following`, or name the section.

**Name interface elements by their label**, not by their appearance or position.

**Avoid ableist language.** `Simply`, `just`, `obviously`, and `easy` tell a stuck reader that their difficulty is their own fault. They add nothing when a step is straightforward, and sting when it is not.

## Accuracy

**Verify, do not assume.** Every command, path, flag, and version in a document should be checked against the current system before it ships.

**Date what will age.** Where a document describes a state that will change, say when it was true.

**A wrong document is worse than a missing one.** A missing document sends the reader elsewhere. A wrong one sends them confidently in the wrong direction.
