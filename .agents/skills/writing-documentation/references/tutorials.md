# Tutorials

A tutorial is a lesson. The reader is a learner, and you are the teacher.

The learner does something, but what they do is not what they learn. The activity is a vehicle. They come away with familiarity, confidence, and a working mental picture. A tutorial that produces a correct result but leaves the reader unsure of themselves has failed.

The reader does not yet know what they want to achieve. That is the whole difference from a how-to guide. Someone who knows what they want does not need a tutorial.

## The teacher's responsibility

The learner follows. Everything that happens is your responsibility, not theirs.

**It must work, every time.** A learner who follows your directions exactly and does not get the result you promised concludes they are the problem, and stops. Reliability matters more in a tutorial than anywhere else in documentation, because the reader has no way to tell your mistake from theirs. Every time includes every way in: a clean machine, and the state the previous tutorial left behind when this one continues it.

**It must be completable.** A tutorial the reader cannot finish is worse than none, because it teaches them that they cannot do this.

**It must be safe to repeat.** Learning comes from repetition. Where you can, make steps reversible so the learner can go back and do it again.

## Rules

**Do not try to teach.** Give the learner something to do that produces understanding. Explanation is not teaching; doing is. This is the rule most often broken, and breaking it is what turns a tutorial into a bad explanation.

**Ruthlessly minimise explanation.** A learner concentrating on getting a step right cannot also absorb a discussion of why. Say the minimum needed to proceed, and link to explanation for the reader who wants it later.

**Give each step a reason, in one clause.** "Your team wants the development server on port 8080" makes a step meaningful without explaining anything. The mechanism behind the step is explanation; link to it.

**Show the destination early.** The learner needs to know where they are going. Describe it rather than promising a feeling: "we will build and deploy a small application", not "you will learn how deployment works".

**Every step produces a visible result.** However small. A learner needs to see that something happened, both to stay oriented and to know they have not gone wrong.

**Say what they will see.** Show the expected output where it confirms that the step worked or holds something to notice. A learner comparing their screen to yours is the fastest error detection you can give them. A learner shown output after every command stops comparing.

**Point out what to notice.** Learning needs reflection, and a learner concentrating on typing will not reflect unless prompted.

**Prefer the concrete.** One specific case, done fully. The general pattern forms in the learner's head from particulars; it does not transfer from a statement of the general rule.

**Ignore the options.** No alternatives, no variations, no "you could also". Every branch is a chance to choose wrong and a demand on attention the learner does not have to spare.

## The learner's hands

The learner works in their own tools, the way they will once the tutorial is over. What their hands do is what they learn.

**Files the learner writes, they write in their editor.** Name the file, say where the change goes, and show the block. A file produced by `printf`, a heredoc, or `sed -i` hides the one thing the tutorial exists to show: what the learner has to change, and where. Keep the shell for what a real user would do in the shell.

**One way of editing on the page.** The same instruction and the same kind of block, every time. Four styles for one action make the learner wonder whether they are four different actions.

**Say where the learner is.** Before every command, name the directory: their project, a temporary workspace, a second checkout. Moving between directories is where learners get lost, and the error they meet does not say why.

**Continue the series.** A tutorial that follows another starts where that one ended, and says so in its first lines. Never paste a script that rebuilds the starting point. It is the longest block on the page and the one the learner understands least, and it creates a second starting point that nobody tests. Link to the previous tutorial, or offer its end state as a download.

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

**Would it work on every path into it?** Not in principle. Run it from a clean machine, and from where the previous tutorial leaves the learner if it continues one. Tutorials rot faster than any other mode because they encode a whole environment, and a series encodes several.

**Does it read like a test script?** Headings named after cases ("both sides change the same line"), a check after every step, output after every command. A page derived from a test plan proves things; a tutorial lets the learner do them. Rewrite the headings as the learner's goals and move the proof into the script.

## A step, before and after

A step written from a test script:

````md
## 3. Both sides change the same line: a conflict

```sh
cd ~/wk/proj && printf '[server]\nport = 8080\n' > cfg.toml && git commit -qam x
```

`feat` already has `port = 9000`.

```sh
git merge --no-edit -q feat
```

```text
CONFLICT (content): Merge conflict in cfg.toml
```
````

The heading names a test case. The shell writes the file, the names are abbreviations, the commit message is a throwaway, and `-q` and `--no-edit` exist for scripts. Two blocks touch, the step has no reason, the directory hides inside a command chain, and nothing tells the learner what to notice.

The same step, written for a learner:

````md
## 3. Change the same line on two branches

Your team wants the development server on port 8080, but a teammate has already moved it to 9000 on the `faster-reload` branch. In the `my-shop` directory, on `main`, open `settings.toml` in your editor and change the port:

```toml title="settings.toml"
[server]
port = 8080
```

Commit your change:

```sh
git commit -am "Serve on port 8080"
```

Now merge your teammate's branch:

```sh
git merge faster-reload
```

The output is similar to the following:

```text
Auto-merging settings.toml
CONFLICT (content): Merge conflict in settings.toml
Automatic merge failed; fix conflicts and then commit the result.
```

Notice that git stopped before committing anything. Open `settings.toml` again: both versions of the line are there, between conflict markers.
````
