---
title: Diff view
description: Review what changed, and put notes on the new code.
---

Press :kbd[2]. The Diff view shows your uncommitted work: staged and unstaged changes and untracked
files, compared to `HEAD`. It's where you review what you (or the agent) just changed and write
the next round of notes.

::image[The Diff view: changed files on the left, the diff with a note on the right]{src="../img/diff.svg"}

## Reading the diff

The left pane lists the changed files with their added and removed line counts. The right pane
shows the selected file as a unified diff:

- Two line-number columns: the old line and the new line.
- `+` lines on a green background were added, `-` lines on red were removed.
- `@@ … @@` lines start each changed region.

| Key                      | In the file list                        |
| ------------------------ | --------------------------------------- |
| :kbd[j] / :kbd[k]        | Next / previous file (the diff follows) |
| :kbd[Enter] or :kbd[Tab] | Move into the diff                      |
| :kbd[r]                  | Reload the diff                         |

In the diff itself, all the usual movement, selection and search keys work, and :kbd[r] reloads
too. The Files and Diff views each keep their own place, so you can switch back and forth with
:kbd[1] and :kbd[2].

## Notes on a diff

Select added or unchanged lines and press :kbd[c], as in the Files view. The note is stored on the
file's current line numbers, so it's an ordinary note: it shows in the Files view too, and compiles
the same way.

:::note
Removed lines no longer exist in the file, so they can't have notes. Pressing :kbd[c] on a `-` line
or a `@@` header shows a message instead.
:::

## Compare against a branch

```sh
tackline --base main
tackline --base upstream/main    # a fork: everything since you branched off upstream
```

`--base` compares against the point where your branch left `<rev>` (git's merge-base), so you see
only your own changes, even if `main` moved on since. Any branch, tag or commit works.

## When the Diff view is empty or missing

- **No changes:** tackline starts on the Files view, and the list says "No changes".
- **Not a git repository:** the Diff view is off; :kbd[2] says so.
