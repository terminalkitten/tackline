---
title: JSON API
description: The small JSON API that tackline serve offers, for scripts and other tools.
---

`tackline serve` also answers JSON under `/api`. It's the same logic as the terminal app: notes are
located in the files as they are now, and prompts compile exactly like `tackline export`.

Requests need the cookie the browser gets from the printed link (see
[Safety](../guides/web.md#safety)). From a script, pass the token once and keep the cookie:

```sh
URL="http://127.0.0.1:7878"
TOKEN="…"   # from the link tackline serve printed
curl -s -c jar "$URL/?token=$TOKEN" > /dev/null
curl -s -b jar "$URL/api/sessions"
```

In paths, `active` stands for the active session's id.

## Sessions

`GET /api/sessions`

```json
[{ "id": "01JBX3K9…", "name": "Search speedup", "notes": 3, "active": true }]
```

`GET /api/sessions/{id}`: the session with each note where it is now.

```json
{
  "id": "01JBX3K9…",
  "name": "Search speedup",
  "notes": [
    {
      "id": "01JBX3M2…",
      "file": "src/matcher.rs",
      "span": { "start_line": 3, "end_line": 7, "start_col": null, "end_col": null },
      "stale": false,
      "text": "replace the loop with an iterator chain"
    }
  ]
}
```

`GET /api/sessions/{id}/prompt`: the compiled prompt, as `tackline export` prints it.

```json
{ "prompt": "# Search speedup\n\n1. `src/matcher.rs:3-7`\n…", "warnings": [] }
```

## Notes

`POST /api/notes` adds a note on whole lines (1-based, inclusive). `session` is optional and
defaults to the active one; without any sessions, one called "Notes" is made.

```json
{ "file": "src/matcher.rs", "start_line": 3, "end_line": 7, "text": "use an iterator", "session": "01JBX3K9…" }
```

Answers `201 Created` with `{ "id", "session", "span" }`, or `400` with `{ "error" }` for a file
outside the repository, empty text, or lines that aren't in the file.

`DELETE /api/notes/{id}` answers `204 No Content`, or `404` when there's no such note.
