---
title: Sessions
description: Group notes per task, switch between sessions, and review and copy the prompt.
---

A session is a named, ordered list of notes: one prompt in the making. You can have as many as you
like; one is **active**, and new notes go into it.

The status bar always shows the active session and its note count:

```text
 FILES  Search speedup (3 notes)  src/matcher.rs 3:1  │ ? help  q quit
```

## Create a session

Press :kbd[Ctrl+N], type a name and press :kbd[Enter]. The new session becomes the active one.
Pressing :kbd[c] when no session exists asks for a name the same way.

## Switch sessions

Press :kbd[Shift+Tab] to make the next session active, in any view; it wraps around after the last
one, and a message shows which session you're on. The gutter markers follow straight away.

For a longer list, press :kbd[Ctrl+O] for the list of sessions with their note counts; `●` marks the active one.
:kbd[j] / :kbd[k] move, :kbd[Enter] makes the session active and opens the Session view, :kbd[Esc]
closes the list.

::image[The session picker]{src="../img/picker.svg"}

## The Session view

Press :kbd[3]. Every note of the active session, in prompt order, with its location, up to six
lines of its code and its instruction.

::image[The Session view]{src="../img/session.svg"}

| Key               | Action                                                                                     |
| ----------------- | ------------------------------------------------------------------------------------------ |
| :kbd[j] / :kbd[k] | Select the next / previous note                                                            |
| :kbd[Enter]       | Jump to the note's code in the Files view                                                  |
| :kbd[e]           | Edit the note's text (and :kbd[Tab] to [move it](notes.md#move-a-note-to-another-session)) |
| :kbd[d]           | Delete the note (asks first)                                                               |
| :kbd[J] / :kbd[K] | Move the note down / up: this is the order in the prompt                                   |
| :kbd[y]           | Copy the compiled prompt                                                                   |

The locations show where each note is *now*: if its code moved, `L88-95` follows it. Stale notes
show `⚠ stale`.

## Copy or export the prompt

:kbd[y] copies the compiled prompt (see [what the agent receives](../workflow.md#what-the-agent-receives))
and shows how many notes went in. Stale notes are skipped and mentioned in the message; an empty
session copies nothing.

From the shell, without opening the app:

```sh
tackline export                   # the active session
tackline export "Search speedup"  # a session by name
tackline export | claude -p       # straight into Claude Code
tackline sessions                 # list them; * marks the active one
```

Warnings (stale notes, an empty session) go to stderr, so piping stays clean.
