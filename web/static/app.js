// tackline web: line selection, the note panel (add and edit, over the code),
// inline editing on the Session page, the status bar note excerpt, Copy
// prompt, and keeping the scroll position across page swaps (htmx boost).
(() => {
  let anchor = null;
  const table = () => document.querySelector("table.code");
  const pane = () => document.querySelector(".code-scroll");
  const form = () => document.getElementById("note-form");
  const isDiff = () => table()?.dataset.view === "diff";

  // ---- the note panel ------------------------------------------------------

  function highlight(test) {
    for (const tr of table().querySelectorAll("tr[data-index]")) tr.classList.toggle("sel", test(tr));
  }

  function mode(f, edit) {
    f.classList.toggle("editing", edit);
    const del = f.querySelector("[data-delete]");
    del.hidden = del.disabled = !edit;
    f.querySelector("[data-save]").textContent = edit ? "Save" : "Save note";
    f.querySelector(".title").textContent = edit ? "Edit note" : "Add a note";
    if (!edit) f.action = f.dataset.addAction;
  }

  // Keep what the panel is about visible above it.
  function reveal(el) {
    const f = form();
    if (!el || !f || f.hidden) return;
    const gap = el.getBoundingClientRect().bottom - (f.getBoundingClientRect().top - 12);
    if (gap > 0) pane().scrollTop += gap;
  }

  function select(a, b) {
    const f = form();
    if (!f) return;
    const [from, to] = a <= b ? [a, b] : [b, a];
    highlight((tr) => {
      const i = Number(tr.dataset.index);
      return i >= from && i <= to;
    });
    if (f.classList.contains("editing")) f.elements.text.value = "";
    mode(f, false);
    f.hidden = false;
    f.elements.start.value = from;
    f.elements.end.value = to;
    const count = to - from + 1;
    f.querySelector(".where").textContent = isDiff()
      ? `${count} row${count > 1 ? "s" : ""}`
      : from === to ? `line ${from}` : `lines ${from}–${to}`;
    f.elements.text.focus({ preventScroll: true });
    reveal(table().querySelector(`tr[data-index="${to}"]`));
    status();
  }

  // Edit a note in the same panel; false when its card isn't on this page.
  function edit(id) {
    const f = form();
    const card = document.querySelector(`[data-note-card="${CSS.escape(id)}"] .note-card`);
    if (!f || !card) return false;
    anchor = null;
    highlight((tr) => tr.dataset.note === id);
    mode(f, true);
    f.action = `/notes/${id}`;
    f.querySelector("[data-delete]").setAttribute("formaction", `/notes/${id}/delete`);
    f.querySelector(".where").textContent = card.dataset.location;
    f.elements.text.value = card.dataset.text;
    f.hidden = false;
    f.elements.text.focus({ preventScroll: true });
    reveal(card);
    return true;
  }

  function clear() {
    anchor = null;
    document.querySelectorAll("table.code tr.sel").forEach((tr) => tr.classList.remove("sel"));
    const f = form();
    if (f) {
      if (f.classList.contains("editing")) f.elements.text.value = "";
      mode(f, false);
      f.hidden = true;
    }
    document.querySelectorAll("form.inline-edit:not([hidden])").forEach(closeInline);
    status();
  }

  // ---- Session page: edit in the list ---------------------------------------

  function openInline(link) {
    const li = link.closest("li");
    const f = li.querySelector("form.inline-edit");
    li.querySelector(".text").hidden = true;
    f.hidden = false;
    const t = f.elements.text;
    t.focus();
    t.setSelectionRange(t.value.length, t.value.length);
  }

  function closeInline(f) {
    f.hidden = true;
    f.reset();
    f.closest("li").querySelector(".text").hidden = false;
  }

  // ---- status bar: the note under the mouse or on the selected line ---------

  function status(tr) {
    const bar = document.querySelector(".statusbar .status-note");
    if (!bar) return;
    tr = tr || (anchor !== null && table()?.querySelector(`tr[data-index="${anchor}"]`)) || null;
    const text = tr?.dataset.excerpt;
    bar.textContent = text ? `${tr.classList.contains("stale") ? "⚠" : "●"} ${text}` : "";
    bar.classList.toggle("stale", !!tr?.classList.contains("stale"));
    const pos = document.querySelector(".statusbar .status-pos");
    if (pos && !pos.dataset.total) pos.dataset.total = pos.textContent;
    const line = tr?.dataset.line;
    if (pos) pos.textContent = line ? `Ln ${line}` : pos.dataset.total;
  }

  document.addEventListener("mouseover", (e) => {
    const tr = e.target.closest?.("table.code tr[data-index]");
    if (tr) status(tr);
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest?.("table.code") && !e.relatedTarget?.closest?.("table.code")) status();
  });

  // ---- copy prompt -----------------------------------------------------------

  async function copyPrompt(button) {
    const label = button.textContent;
    try {
      const res = await fetch("/prompt");
      if (!res.ok) throw new Error(await res.text());
      await navigator.clipboard.writeText(await res.text());
      button.textContent = "Copied";
    } catch {
      button.textContent = "Copy failed";
    }
    setTimeout(() => (button.textContent = label), 1500);
  }

  // ---- clicks and keys -------------------------------------------------------

  document.addEventListener("click", (e) => {
    const num = e.target.closest("table.code td.num:not(.old)");
    if (num) {
      const i = Number(num.parentElement.dataset.index);
      if (e.shiftKey && anchor !== null) {
        select(anchor, i);
      } else {
        anchor = i;
        select(i, i);
      }
      e.preventDefault();
      return;
    }
    const note = e.target.closest("[data-edit-note]");
    if (note && edit(note.dataset.editNote)) {
      e.preventDefault();
      return;
    }
    const inline = e.target.closest("[data-inline-edit]");
    if (inline) {
      e.preventDefault();
      openInline(inline);
      return;
    }
    const cancel = e.target.closest("[data-cancel-edit]");
    if (cancel) closeInline(cancel.closest("form"));
    if (e.target.closest("[data-delete]") && !confirm("Delete this note?")) e.preventDefault();
    if (e.target.closest("[data-clear]")) clear();
    const copy = e.target.closest("[data-copy-prompt]");
    if (copy) copyPrompt(copy);
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") clear();
    const f = e.target.closest?.("#note-form, form.inline-edit");
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && f) {
      e.preventDefault();
      f.requestSubmit();
    }
  });

  // htmx reads a boosted form's action once, when the page loads: send an
  // edit to the note's own URL.
  document.addEventListener("htmx:configRequest", (e) => {
    const f = e.detail.elt;
    if (f?.id === "note-form" && f.classList.contains("editing") && e.detail.path === f.dataset.addAction) {
      e.detail.path = f.getAttribute("action");
    }
  });

  // ---- keep the place across page swaps ---------------------------------------
  // A note saved, edited or deleted reloads the page (a redirect that htmx
  // swaps in); the code and the file tree stay scrolled where they were.

  const KEY = "tackline-scroll";
  const where = () => {
    const t = table();
    return t ? `${t.dataset.view}:${t.dataset.file}` : null;
  };

  function remember(withCode) {
    const p = pane();
    const side = document.querySelector(".side");
    const saved = { side: side ? side.scrollTop : 0 };
    if (withCode && p) Object.assign(saved, { at: where(), top: p.scrollTop, left: p.scrollLeft });
    try {
      sessionStorage.setItem(KEY, JSON.stringify(saved));
    } catch {}
  }
  // Capture: runs before htmx takes the submit over.
  document.addEventListener("submit", () => remember(true), true);
  document.addEventListener("htmx:beforeRequest", (e) => {
    if (e.detail.elt?.tagName === "A") remember(false);
  });

  function restore() {
    let saved = null;
    try {
      saved = JSON.parse(sessionStorage.getItem(KEY));
      sessionStorage.removeItem(KEY);
    } catch {}
    const side = document.querySelector(".side");
    if (saved && side) side.scrollTop = saved.side || 0;
    const p = pane();
    if (saved && p && saved.at && saved.at === where()) {
      p.scrollTop = saved.top;
      p.scrollLeft = saved.left;
      return true;
    }
    return false;
  }

  // After loading or swapping a page: back where we were, or show the line
  // the link points at.
  function settle() {
    anchor = null;
    const kept = restore();
    const t = table();
    const n = t && Number(t.dataset.focus);
    if (!n) return;
    const tr = t.querySelector(`tr[data-index="${n}"]`);
    if (!tr) return;
    tr.classList.add("focus");
    const p = pane();
    const r = tr.getBoundingClientRect();
    const box = p.getBoundingClientRect();
    if (!kept || r.top < box.top || r.bottom > box.bottom) {
      p.scrollTop += r.top - box.top - (box.height - r.height) / 2;
    }
  }
  document.addEventListener("DOMContentLoaded", settle);
  document.addEventListener("htmx:afterSwap", settle);
})();
