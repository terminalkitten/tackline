---
title: Find and search
description: Find files by name, search code across the repo, search the open file, jump to a line.
---

## Find a file

Press :kbd[Ctrl+P] and type part of a path. Matching is fuzzy (`mtch` finds `src/matcher.rs`), and
an uppercase letter makes it case-sensitive. :kbd[↑] / :kbd[↓] (or :kbd[Ctrl+P] / :kbd[Ctrl+N])
move, :kbd[Enter] opens the file.

::image[Finding a file]{src="../img/finder.svg"}

## Search code in the repo

Press :kbd[Ctrl+F], type, and press :kbd[Enter] to search. Results show as `path:line: text`;
pick one with the arrow keys and press :kbd[Enter] again to jump there, with the line in the middle
of the screen.

::image[Search results across the repo]{src="../img/search.svg"}

- The query is a regular expression. If it isn't a valid one, like `contains(`, it's searched as
  plain text.
- Lowercase queries ignore case; any uppercase letter makes the search case-sensitive.
- Files in `.gitignore` and binary files are skipped. The first 500 hits are shown.
- Change the query and press :kbd[Enter] to search again.

## Search the open file

Press :kbd[/] in the code pane, type, and press :kbd[Enter] to jump to the next match. Then :kbd[n]
goes to the next match and :kbd[N] to the previous one, wrapping around the file. Case works as in
repo search; the text is matched literally.

## Go to a line

Press :kbd[:], type a line number and press :kbd[Enter]. In the Diff view the number is a line in
the current file (the new side of the diff).
