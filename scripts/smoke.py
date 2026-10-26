"""Smoke test for an installed tackline: python scripts/smoke.py [path/to/tackline]

Makes a small git repo, adds a note through `tackline serve`'s JSON API,
checks `tackline sessions` and `tackline export`, then starts the terminal app
in a pseudo-terminal, waits for it to draw and quits it with `q`.
Windows needs `pip install pywinpty` for the last step.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

exe = shutil.which(sys.argv[1] if len(sys.argv) > 1 else "tackline") or sys.exit(
    "tackline not found"
)
exe = os.path.abspath(exe)
repo = tempfile.mkdtemp()


def run(*args):
    out = subprocess.run(args, cwd=repo, capture_output=True, text=True, timeout=60)
    if out.returncode != 0:
        sys.exit(f"{args} failed ({out.returncode}):\n{out.stdout}{out.stderr}")
    return out.stdout


def check(ok, what, detail=""):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok:
        sys.exit(detail)


# A repo with a committed file and an uncommitted change.
os.makedirs(os.path.join(repo, "src"))
with open(os.path.join(repo, "src", "lib.rs"), "w") as f:
    f.write("pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n")
with open(os.path.join(repo, ".gitignore"), "w") as f:
    f.write(".tackline/\n")
git = ["git", "-c", "user.name=smoke", "-c", "user.email=smoke@example.com", "-c", "commit.gpgsign=false"]
run(*git, "init", "-q")
run(*git, "add", ".")
run(*git, "commit", "-qm", "init")
with open(os.path.join(repo, "src", "lib.rs"), "a") as f:
    f.write("\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n")

check(run(exe, "--version").startswith("tackline "), "--version")

# Web version: add a note through the JSON API.
server = subprocess.Popen(
    [exe, "serve", "--no-open", "--port", "0"],
    cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
)
try:
    url = re.search(r"http://\S+", server.stdout.readline()).group(0)
    base, token = url.split("/?token=")
    request = urllib.request.Request(
        base + "/api/notes",
        data=json.dumps(
            {"file": "src/lib.rs", "start_line": 1, "end_line": 3, "text": "smoke: make it generic"}
        ).encode(),
        headers={"Content-Type": "application/json", "Cookie": f"tackline_token={token}"},
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        check(response.status == 201, "serve: POST /api/notes")
finally:
    server.terminate()
    server.wait(timeout=10)

check("Notes" in run(exe, "sessions"), "sessions lists the new session")
prompt = run(exe, "export")
check(
    "src/lib.rs:1-3" in prompt and "smoke: make it generic" in prompt,
    "export has the note",
    prompt,
)


# Terminal app: it must draw and quit on `q`.
# A real terminal answers the app's queries; this one has to as well.
QUERIES = {
    b"\x1b[c": b"\x1b[?62;22c",  # device attributes (also ends the keyboard-protocol probe)
    b"\x1b[6n": b"\x1b[1;1R",  # cursor position
    b"\x1b]11;?": b"\x1b]11;rgb:0000/0000/0000\x1b\\",  # background color
}


def replies(chunk):
    return b"".join(answer for query, answer in QUERIES.items() if query in chunk)


def tui_unix():
    import fcntl
    import pty
    import select
    import struct
    import termios

    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(repo)
        os.environ["TERM"] = "xterm-256color"
        os.execv(exe, [exe])
    # 30 rows, 100 columns: a new pty is 0x0, and the app draws nothing in that.
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
    screen, sent, deadline = b"", False, time.time() + 20
    while time.time() < deadline:
        if select.select([fd], [], [], 0.2)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError:  # the app exited and closed the terminal
                break
            if not chunk:
                break
            screen += chunk
            os.write(fd, replies(chunk))
        if not sent and b"quit" in screen:
            os.write(fd, b"q")
            sent = True
    # Output ended (or time's up): give the app 5 s to exit.
    for _ in range(50):
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return sent, os.waitstatus_to_exitcode(status), screen
        time.sleep(0.1)
    os.kill(pid, 9)
    return sent, None, screen


def tui_windows():
    from winpty import PtyProcess

    proc = PtyProcess.spawn([exe], cwd=repo, dimensions=(30, 100))
    screen, sent, deadline = "", False, time.time() + 20
    while time.time() < deadline and proc.isalive():
        try:
            chunk = proc.read(65536)
        except EOFError:
            break
        screen += chunk
        answer = replies(chunk.encode())
        if answer:
            proc.write(answer.decode())
        if not sent and "quit" in screen:
            proc.write("q")
            sent = True
        time.sleep(0.1)
    if proc.isalive():
        proc.terminate(force=True)
        return sent, None, screen
    return sent, proc.exitstatus, screen


sent, code, screen = tui_windows() if os.name == "nt" else tui_unix()
tail = screen[-2000:] if isinstance(screen, str) else screen[-2000:].decode(errors="replace")
check(sent, "tui draws its status bar", tail)
check(code == 0, f"tui quits on q (exit {code})", tail)
shutil.rmtree(repo, ignore_errors=True)
print("all good")
