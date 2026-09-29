# Reference material

Reference describes the machinery. It is what the reader consults while working, not what they read to learn.

It is a map. A map tells you what you need to know about the territory without walking it, and it does not tell you where to go. Reference does the same for a product: it states what is there, accurately, and leaves the reader to decide what to do with it.

Reference is the only mode organised around the product rather than around the reader. That is correct, because a reader consulting reference already knows what they are looking for and needs to find it where they expect.

## Rules

**Describe, and only describe.** Neutral description is the whole job. State what the thing is, what it does, what values it takes, what it returns, what it refuses.

**Mirror the structure of the product.** If the product has modules, the reference has sections matching them, named the same way. A reader who knows the product can then navigate the reference without learning a second structure.

**Be consistent before anything else.** Every entry of the same kind takes the same shape, in the same order, with the same headings. Consistency is worth more than elegance here. Reference is scanned rather than read, and a reader scanning relies on shape.

**Be authoritative.** There must be no doubt in reference. A reader who has to verify a claim elsewhere has lost the reason to use it.

**Include examples, but keep them short.** An example illustrates usage. It does not walk the reader through a task, which would make it a how-to guide.

**Warn where warning is due.** Destructive behaviour, irreversible operations, and surprising defaults belong in reference, where a reader meets them at the moment they matter.

## What must not appear

Reference excludes recipes, advice, opinion, speculation, marketing, instruction, and explanation.

Each one has a home. Instruction belongs in a how-to guide, teaching in a tutorial, reasoning in explanation. Link to them. The pull toward stuffing this material into reference is strong, because reference is where the reader already is, and it should be resisted every time.

## Titles

Name the thing. Reference titles are nouns, matching the product's own vocabulary exactly.

A reference page for a configuration file is called by the name of that file. A reference page for a command is called by the name of that command. Inventing friendlier names breaks the mapping between product and documentation, which is the one property reference cannot lose.

## Language

- `The command accepts three options.`
- `The value must be an integer between 1 and 65535. The default is 8080.`
- `Returns an empty list when no records match.`
- `Deleting a project also deletes its runs. This cannot be undone.`
- `For instructions, see the deployment guide.`

The register is austere and factual. Reference is the one mode where dry writing is a virtue.

## Detection tests

**Is there advice in it?** Search for `should`, `recommended`, `best`, and `prefer`. Each one is an opinion, and belongs elsewhere.

**Is there a reason in it?** Search for `because` and `so that`. Reasoning is explanation.

**Does an entry differ in shape from its neighbours?** Compare two entries of the same kind side by side. Different headings, different order, or a field present in one and missing in the other all cost the reader more than the words saved.

**Could a reader answer their question without reading prose?** Reference is scanned. If the fact is buried in a paragraph, put it in a table or a list.

**Does the structure match the product?** Read the section names against the product's own names. Any divergence is a place a reader will fail to look.

**Is it complete?** Missing entries are the characteristic failure of reference, and unlike the other modes there is no judgement involved: either every option is documented or the reference cannot be trusted.
