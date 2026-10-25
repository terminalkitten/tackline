---
title: Where data lives
description: The .tackline folder, the session files and how they're saved.
---

Everything is in a `.tackline` folder in the repository root, as readable JSON:

:::filetree
- **.tackline**
  - state.json the active session, last view and last open file
  - **sessions**
    - search-speedup.json one file per session
    - docs-pass.json
:::

- **Saved as you go.** Every change writes its file right away; there's no save key. Writes go to
  a temporary file that then replaces the old one, so a crash never leaves half a file.
- **File names** come from the session name. Two names that give the same file name get `-2`,
  `-3`, ….
- **Not in git.** On first run tackline offers to add `.tackline/` to `.gitignore`. Sessions are
  personal working notes; commit them only if you mean to share them.

## A session file

```json
{
  "version": 1,
  "id": "01JBX3K9Q8F7Z2M4N6P0R5T8VW",
  "name": "Search speedup",
  "created_at": "2026-10-25T13:20:00Z",
  "notes": [
    {
      "id": "01JBX3M2W4Q8E6R1T7Y3V5K9P0",
      "file": "src/matcher.rs",
      "span": { "start_line": 3, "end_line": 7, "start_col": null, "end_col": null },
      "anchor": {
        "selected": "    for item in items {\n…\n    }",
        "before": "pub fn matches(query: &str, items: &[String]) -> Vec<String> {\n    let mut out = Vec::new();",
        "after": "    out\n}"
      },
      "text": "replace the loop with an iterator chain",
      "created_at": "2026-10-25T13:21:04Z"
    }
  ]
}
```

- `notes` is in prompt order.
- `span` lines are 1-based and inclusive; `start_col` / `end_col` are set for character selections.
- `anchor` is how a note finds its code again after edits: the selected text and up to two lines
  before and after it.
- `version` is there for future changes to the format.

Whether a note is stale isn't stored: it's worked out from the files each time.
