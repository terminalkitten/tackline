"""Screenshots for the web guide. Run through web-shots.sh, which starts the server.

Usage: python web_shots.py <url with token> <output dir>
"""
import sys

from playwright.sync_api import sync_playwright

url, out = sys.argv[1], sys.argv[2]
with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_context(
        viewport={"width": 1200, "height": 640}, device_scale_factor=2, color_scheme="dark"
    ).new_page()
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))

    def save(note):
        page.fill("#note-form textarea", note)
        page.keyboard.press("Control+Enter")
        page.wait_for_timeout(700)

    page.goto(url)
    page.click("summary >> text=src/")
    page.click("text=matcher.rs")
    page.wait_for_selector("table.code")

    # The first note also names the session.
    page.click('tr[data-index="4"] td.num')
    page.click('tr[data-index="8"] td.num', modifiers=["Shift"])
    page.fill('#note-form input[placeholder^="Session name"]', "Search speedup")
    page.fill(
        "#note-form textarea",
        "Use an iterator chain instead of the loop:\n"
        "items.iter().filter(|i| i.contains(query)).cloned().collect()",
    )
    page.wait_for_timeout(200)
    page.screenshot(path=f"{out}/web-add-note.png")
    page.keyboard.press("Control+Enter")
    page.wait_for_timeout(700)

    page.click('tr[data-index="2"] td.num')
    save("Take an iterator instead of a slice")
    page.hover('tr[data-index="5"]')
    page.wait_for_timeout(200)
    page.screenshot(path=f"{out}/web-files.png")

    page.click("text=Diff")
    page.wait_for_timeout(500)
    page.click('tr[data-index="5"] td.num:not(.old)')
    save("Read the version from package.json instead")
    page.screenshot(path=f"{out}/web-diff.png")

    page.click("header >> text=Session")
    page.wait_for_timeout(500)
    page.screenshot(path=f"{out}/web-session.png")

    if errors:
        sys.exit(f"page errors: {errors}")
