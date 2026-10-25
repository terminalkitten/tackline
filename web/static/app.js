// tackline web: line selection and the add-note panel, copy prompt, and
// scrolling back to the line you were on after a page swap (htmx boost).
(() => {
  let anchor = null;
  const table = () => document.querySelector("table.code");
  const form = () => document.getElementById("add-note");

  function select(a, b) {
    const [from, to] = a <= b ? [a, b] : [b, a];
    for (const tr of table().querySelectorAll("tr")) {
      const i = Number(tr.dataset.index);
      tr.classList.toggle("sel", i >= from && i <= to);
    }
    const f = form();
    f.hidden = false;
    f.elements.start.value = from;
    f.elements.end.value = to;
    const diff = table().dataset.view === "diff";
    const count = to - from + 1;
    f.querySelector(".where").textContent = diff
      ? `${count} row${count > 1 ? "s" : ""}`
      : from === to ? `line ${from}` : `lines ${from}–${to}`;
    f.elements.text.focus();
  }

  function clear() {
    anchor = null;
    document.querySelectorAll("table.code tr.sel").forEach((tr) => tr.classList.remove("sel"));
    const f = form();
    if (f) f.hidden = true;
  }

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
    if (e.target.closest("[data-clear]")) clear();
    const copy = e.target.closest("[data-copy-prompt]");
    if (copy) copyPrompt(copy);
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") clear();
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && e.target.closest("#add-note")) {
      e.preventDefault();
      form().requestSubmit();
    }
  });

  // After loading or swapping a page: show the line we came back to.
  function focus() {
    anchor = null;
    const t = table();
    const n = t && Number(t.dataset.focus);
    if (!n) return;
    const tr = t.querySelector(`tr[data-index="${n}"]`);
    if (tr) {
      tr.scrollIntoView({ block: "center" });
      tr.classList.add("focus");
    }
  }
  document.addEventListener("DOMContentLoaded", focus);
  document.addEventListener("htmx:afterSettle", focus);
})();
