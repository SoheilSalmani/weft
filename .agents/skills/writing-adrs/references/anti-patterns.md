# ADR anti-patterns

Each entry gives the failure, a test for detecting it in a draft, and the fix. The catalogue follows Olaf Zimmermann's work on decision record quality.

## Fairy Tale

**Failure.** The record presents only benefits. No cost, constraint, or drawback appears anywhere.

**Test.** Read the consequences section alone. If nothing in it would make a reader hesitate, the section is incomplete.

**Fix.** Every real decision closes off something. Name what this one closes off, and what becomes harder.

## Sales Pitch

**Failure.** Promotional language stands in for evidence. Words like "robust", "seamless", "best-in-class", and "significantly" carry the argument.

**Test.** Delete every adjective and adverb. If the claim disappears, there was no claim.

**Fix.** Replace each with a measurement, a constraint, or a fact. "Significantly faster" becomes "reduces the median response from 800ms to 120ms", or it is cut.

## Free Lunch

**Failure.** Immediate consequences are recorded and long-term or operational ones are not.

**Test.** Ask what this decision costs after a year of running, not on the day it ships.

**Fix.** Record the maintenance, operational, and migration consequences alongside the immediate ones.

## Dummy Alternative

**Failure.** Options exist only to make the chosen one look inevitable. Nobody would have picked them.

**Test.** For each rejected option, ask whether a competent engineer could have argued for it. If not, it was not an option.

**Fix.** Remove the straw options. One genuine option is more honest than three fake ones. If there truly was only one viable path, say so and omit the section.

## Mega-ADR

**Failure.** The record carries design specifications, component diagrams, schemas, or code. It has become architecture documentation with a status field.

**Test.** Any code block, file path, or line number is a warning. In a decision record, more than one diagram is a warning too. A baseline record is exempt from the diagram half of this test, since describing structure is its purpose.

**Fix.** Move the detail into architecture documentation and keep the reasoning. The record explains why, not how.

## Novel

**Failure.** A single decision is inflated into a full architecture document covering the whole system.

**Test.** Check whether the record explains parts of the system the decision does not touch.

**Fix.** Cut everything that is not needed to understand this one choice. Most records fit on one page.

## Blueprint

**Failure.** The record reads as policy or instruction rather than as a record of a choice. It commands rather than explains.

**Test.** Look for imperative sentences aimed at the reader, such as "always do this" or "never do that".

**Fix.** A decision log records what was decided and why. Rules and conventions belong in contributor documentation.

## Maze

**Failure.** The record drifts into technical detail that has no bearing on the decision.

**Test.** For each paragraph, ask whether removing it would change how a reader understands the decision. If not, remove it.

**Fix.** Keep only the context that constrains the choice.

## Tunnel Vision

**Failure.** Only the developer's perspective appears. Operations, maintenance, cost, and end users are absent.

**Test.** Ask who else is affected by this decision and check whether the record mentions them.

**Fix.** Record the consequences for the people who run the system and the people who use it.

## Groundhog Day

**Failure.** Not a drafting fault but the failure the log exists to prevent. A decision gets argued again from scratch because nobody recorded why it was settled.

**Test.** Ask whether this discussion has happened before. If it has and no record exists, that is the record to write.

**Fix.** Write the record, including the options that were rejected and the reason each was rejected. Rejected records stay in the log for exactly this purpose.
