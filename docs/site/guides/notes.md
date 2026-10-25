---
title: Notes
description: Select code, write an instruction on it, and edit, move or delete it later.
---

A note is one instruction on one piece of code: a file, a line span (or a character span) and the
text you write. Notes live in [sessions](sessions.md).

## Select code

In the code pane of the Files or [Diff](diff.md) view:

| To select                | Do                                                    |
| ------------------------ | ----------------------------------------------------- |
| Whole lines              | :kbd[V], then move with :kbd[j] / :kbd[k] (or arrows) |
| Characters               | :kbd[v], then move                                    |
| Lines, editor style      | :kbd[Shift+↑] / :kbd[Shift+↓]                         |
| Characters, editor style | :kbd[Shift+←] / :kbd[Shift+→]                         |
| With the mouse           | Drag in the code                                      |

Press :kbd[v] or :kbd[V] again to stop selecting, :kbd[Esc] to clear the selection. Without a
selection, a note goes on the line the cursor is on.

## Write a note

Press :kbd[c]. The note popup shows where the note goes (`src/matcher.rs:L2-3`), the first lines of
the code, and the session it will be saved in.

::image[The note popup]{src="../img/note.svg"}

| Key                         | In the note popup                                                                             |
| --------------------------- | --------------------------------------------------------------------------------------------- |
| typing                      | The instruction                                                                               |
| :kbd[Enter]                 | Save                                                                                          |
| `\` then :kbd[Enter]        | New line (the `\` is removed)                                                                 |
| :kbd[Shift+Enter]           | New line, in terminals that report it (see [Terminals](../reference/terminals.md#shiftenter)) |
| :kbd[Tab] / :kbd[Shift+Tab] | Next / previous session                                                                       |
| :kbd[Esc]                   | Cancel                                                                                        |

Saving to another session than the active one doesn't change the active session. If no session
exists yet, :kbd[c] asks for a session name first and then opens the note.

## See your notes

In the code pane, the gutter marks every line with a note in the active session:

```text
 3 ●     for item in items {
 4 │         if item.contains(query) {
 5 ●             out.push(item.clone());
 8 ●     out
```

- `●` alone: a one-line note.
- `●` … `│` … `●`: a note over several lines, from first to last line.
- `⚠` in yellow: a [stale note](#stale-notes).

Files with notes in the active session get a `●` in the file tree.

## Edit a note

Put the cursor on a noted line and press :kbd[c]: the popup opens with the note's text, and
:kbd[Enter] saves your changes. Selecting exactly the lines of a note and pressing :kbd[c] does the
same. A selection that only overlaps a note makes a new note.

In the [Session view](sessions.md#the-session-view), :kbd[e] edits the selected note.

## Move a note to another session

While editing a note, press :kbd[Tab] or :kbd[Shift+Tab] to pick another session, then :kbd[Enter].
The note moves there (to the end of that session) with your edits, and both sessions are saved.

## Delete a note

Put the cursor on a noted line and press :kbd[d], or select the note in the Session view and press
:kbd[d]. tackline asks first: :kbd[y] deletes, :kbd[n] keeps it.

## Stale notes

Each note remembers the code it was written on and a little of what was around it. When the file
changes, tackline finds that code again:

- Lines inserted or removed above it: the note moves along.
- The code moved elsewhere in the file: the note follows it.
- The same code appears more than once: the copy with the most similar surroundings wins.
- The code is gone: the note becomes **stale**.

Stale notes show ⚠ in the gutter and the Session view, and are left out of the compiled prompt
with a warning. They're kept on disk, so if the code comes back (an undo, a reverted change) the
note recovers.
