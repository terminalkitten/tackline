---
title: Workflow with Claude Code
description: From notes to a prompt an agent can act on, and back again after it edits your code.
---

A session is a batch of instructions for one piece of work. You fill it while reading the code,
review it, and hand it to the agent as one prompt.

## The loop

:::steps
1. **Start a session for the task.** :kbd[Ctrl+N], and name it after the work: "Search speedup",
   "Error handling pass". One session per task keeps the prompt focused.

2. **Read and annotate.** Walk through the code (or the [diff](guides/diff.md) of what you or the
   agent just changed) and put a note on everything that should change. Small, specific notes beat
   one long instruction.

3. **Review.** :kbd[3] opens the Session view: each note with its location, code and instruction,
   in the order the agent will read them. Reorder with :kbd[J] and :kbd[K], fix text with :kbd[e],
   drop notes with :kbd[d].

4. **Send it.** Copy with :kbd[y] and paste into Claude Code, or skip the clipboard:

   ```sh
   tackline export | claude -p
   ```

5. **Check the result.** After the agent edits, the [Diff view](guides/diff.md) shows what changed.
   Add notes on that diff for the next round, in the same session or a new one.
:::

::image[The Session view: the prompt, note by note]{src="img/session.svg"}

## What the agent receives

`y` and `tackline export` produce the same Markdown. Each note becomes a numbered item with its
location, the code as it is *now*, and your instruction:

````markdown
# Search speedup

1. `src/matcher.rs:3-7`
   ```rs
       for item in items {
           if item.contains(query) {
               out.push(item.clone());
           }
       }
   ```
   replace the loop with an iterator chain

2. `scripts/score.py:2:13-2:38`
   ```py
       total = sum(v * v for v in values)
   ```
   use math.fsum here
````

- Line spans look like `3-7`; character selections like `2:13-2:38` (line:column), and the
  snippet still shows the full lines for context.
- Snippets longer than 40 lines are cut short with `…`.
- The code fence language comes from the file type, so the agent knows what it's reading.

## Writing notes that work

- **Say what to do, not what's wrong.** "Return early when `query` is empty" gets better results
  than "this is inefficient".
- **Select exactly the code it's about.** The selection is the agent's context: a whole function
  for a refactor, one expression for a rename.
- **Add constraints.** "…keep the public signature", "…don't add dependencies".
- **Use order.** Notes are read top to bottom: put setup first ("add a helper `parse_range`") and
  its uses after.

:::tip[Multi-line notes]
In the note popup, end a line with `\` and press :kbd[Enter] for a newline, or use
:kbd[Shift+Enter] in terminals that report it. :kbd[Enter] on its own saves.
:::

## When the code changes

Notes are anchored to the code they were written on, not just to line numbers. When lines are
added above a note, or the code moves within the file, the note moves with it. That includes the
agent's own edits: a note on `fn parse` still points at `fn parse` after the agent added imports
above it.

If the annotated code is gone, the note is **stale**: it shows ⚠ and is left out of the prompt,
with a warning when you copy or export. Delete it with :kbd[d] on its line (or in the Session
view), and write a new note on the new code.

## Several sessions

Keep a session per task or per round with the agent. :kbd[Ctrl+O] switches the active session; new
notes go there. A note in the wrong session can be [moved](guides/notes.md#move-a-note-to-another-session).
