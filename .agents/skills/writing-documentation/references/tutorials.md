# Tutorials

A tutorial is a lesson. The reader is a learner, and you are the teacher.

The learner does something, but what they do is not what they learn. The activity is a vehicle. They come away with familiarity, confidence, and a working mental picture. A tutorial that produces a correct result but leaves the reader unsure of themselves has failed.

The reader does not yet know what they want to achieve. That is the whole difference from a how-to guide. Someone who knows what they want does not need a tutorial.

## The teacher's responsibility

The learner follows. Everything that happens is your responsibility, not theirs.

**It must work, every time.** A learner who follows your directions exactly and does not get the result you promised concludes they are the problem, and stops. Reliability matters more in a tutorial than anywhere else in documentation, because the reader has no way to tell your mistake from theirs.

**It must be completable.** A tutorial the reader cannot finish is worse than none, because it teaches them that they cannot do this.

**It must be safe to repeat.** Learning comes from repetition. Where you can, make steps reversible so the learner can go back and do it again.

## Rules

**Do not try to teach.** Give the learner something to do that produces understanding. Explanation is not teaching; doing is. This is the rule most often broken, and breaking it is what turns a tutorial into a bad explanation.

**Ruthlessly minimise explanation.** A learner concentrating on getting a step right cannot also absorb a discussion of why. Say the minimum needed to proceed, and link to explanation for the reader who wants it later.

**Show the destination early.** The learner needs to know where they are going. Describe it rather than promising a feeling: "we will build and deploy a small application", not "you will learn how deployment works".

**Every step produces a visible result.** However small. A learner needs to see that something happened, both to stay oriented and to know they have not gone wrong.

**Say what they will see.** Show the expected output. A learner comparing their screen to yours is the fastest error detection you can give them.

**Point out what to notice.** Learning needs reflection, and a learner concentrating on typing will not reflect unless prompted.

**Prefer the concrete.** One specific case, done fully. The general pattern forms in the learner's head from particulars; it does not transfer from a statement of the general rule.

**Ignore the options.** No alternatives, no variations, no "you could also". Every branch is a chance to choose wrong and a demand on attention the learner does not have to spare.

## Language

- `In this tutorial, we will build a small application and deploy it.`
- `First, do x. Now, do y.`
- `The output should look something like this:`
- `Notice that the file has changed. Remember that we set this earlier.`
- `You have built a working application and deployed it.`

`We` is right here and nowhere else in documentation. It carries the relationship between teacher and learner.

## Detection tests

**Is it actually a how-to guide?** Ask whether the reader already knows what they want to achieve. If they do, it is a how-to guide, and treating them as a beginner will irritate them.

**Is it explanation wearing a tutorial's clothes?** Count the paragraphs that could be removed without changing what the reader does. If there are several, move them to explanation.

**Does it branch?** Search for "if you", "alternatively", and "you can also". A tutorial has one path.

**Would it work on a clean machine today?** Not in principle. Run it. Tutorials rot faster than any other mode because they encode a whole environment.
