#!/usr/bin/env -S uv run --quiet
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyobjc-framework-Quartz"]
# ///
"""
Drive the iOS Simulator's SubsetJuliaVM REPL: paste a block of Julia code into
the REPL input, run it, and (optionally) capture a screenshot.

This automates the manual flow used to reproduce/verify REPL bugs (e.g. Issue
#8214): switch to the REPL tab, focus the input field, paste the code via the
device pasteboard, press Enter to evaluate, then screenshot the result.

Why it is built this way
------------------------
* UI elements are located by **accessibility role/label** (the Simulator exposes
  the running app's AX tree to macOS), not hard-coded pixels, so it survives
  layout changes and different window sizes/displays.
* Clicks and keystrokes are posted with **Quartz CGEvents** at the AX element's
  on-screen frame.
* Paste uses Cmd+V with an **explicit Cmd key-down/up around 'v'** — a lone
  `kCGEventFlagMaskCommand` flag is dropped by the Simulator and types a literal
  "v" instead of pasting.

Requirements
------------
* `uv` (the shebang runs this under `uv run`, which installs pyobjc on demand).
* macOS **Accessibility** permission for the terminal/app that runs this script
  (System Settings → Privacy & Security → Accessibility). Under tmux the
  "responsible" process may differ from the visible terminal; if AX queries fail
  with -1719 this script prints a hint.
* A booted Simulator with the app installed (see `scripts/ios_repl_e2e.sh` for
  the full build/install/launch wrapper).

Examples
--------
    uv run scripts/ios_repl_paste.py --code-file path/to/snippet.jl --screenshot out.png
    uv run scripts/ios_repl_paste.py --code 'using Plots; plot(sin)' --wait 4 --screenshot p.png
    uv run scripts/ios_repl_paste.py --dump-ax        # debug: print the REPL AX tree
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import time

import Quartz

CMD = Quartz.kCGEventFlagMaskCommand
KEY_V = 9
KEY_RETURN = 36
KEY_CMD = 55
BUNDLE_ID_DEFAULT = "jp.atelier-arith.subsetjuliavm"

# AX descriptions for the controls we drive. Update here if the SwiftUI labels
# change (find them with `--dump-ax`).
REPL_TAB_ROLE, REPL_TAB_DESC = "AXRadioButton", "REPL"
INPUT_ROLE = "AXTextField"
RESET_ROLE, RESET_DESC = "AXButton", "Reset"


class InfraFailure(RuntimeError):
    """Harness failure that must not be counted as a sample failure."""

    def __init__(self, kind: str, message: str):
        self.kind = kind
        super().__init__(f"{kind}: {message}")


def log(msg: str) -> None:
    print(f"[ios-repl] {msg}", flush=True)


def die(msg: str, code: int = 1):
    print(f"[ios-repl] ERROR: {msg}", file=sys.stderr, flush=True)
    sys.exit(code)


# ---------------------------------------------------------------------------
# simctl helpers
# ---------------------------------------------------------------------------
def simctl(*args: str, check: bool = True, capture: bool = False) -> str:
    cp = subprocess.run(
        ["xcrun", "simctl", *args],
        check=check,
        text=True,
        capture_output=True,
    )
    if capture:
        return cp.stdout
    return ""


def set_pasteboard(device: str, text: str) -> None:
    subprocess.run(
        ["xcrun", "simctl", "pbcopy", device],
        input=text,
        text=True,
        check=True,
    )


def screenshot(device: str, path: str) -> None:
    subprocess.run(
        ["xcrun", "simctl", "io", device, "screenshot", path],
        check=True,
        capture_output=True,
    )


def launch_app(device: str, bundle_id: str, relaunch: bool) -> None:
    if relaunch:
        subprocess.run(
            ["xcrun", "simctl", "terminate", device, bundle_id],
            capture_output=True,
            check=False,
        )
    subprocess.run(
        ["xcrun", "simctl", "launch", device, bundle_id],
        check=True,
        capture_output=True,
    )
    activate_simulator()


# ---------------------------------------------------------------------------
# AppleScript / accessibility helpers
# ---------------------------------------------------------------------------
def osascript(script: str) -> str:
    cp = subprocess.run(
        ["osascript", "-"],
        input=script,
        text=True,
        capture_output=True,
    )
    out = (cp.stdout or "").strip()
    err = (cp.stderr or "").strip()
    if "not allowed assistive access" in err:
        raise InfraFailure(
            "accessibility_denied",
            "Accessibility permission denied for the controlling app.\n"
            "  Grant it in System Settings → Privacy & Security → Accessibility\n"
            "  to the terminal/app running this script (note: under tmux the\n"
            "  'responsible' process can differ from the visible terminal).",
        )
    if "-1719" in err:
        return f"ERR:{err}"
    if err and not out:
        return f"ERR:{err}"
    return out


def restart_simulator_app() -> None:
    log("AX preflight failed; restarting Simulator once")
    subprocess.run(["osascript", "-e", 'tell application "Simulator" to quit'], check=False)
    time.sleep(1.0)
    subprocess.run(["open", "-a", "Simulator"], check=False)
    time.sleep(1.0)


def activate_simulator() -> None:
    # `open -a` is what actually surfaces the Simulator's device window; a bare
    # AppleScript `activate` sometimes leaves the process with zero windows, so
    # `entire contents of window 1` fails with "Invalid index".
    subprocess.run(["open", "-a", "Simulator"], check=False)
    osascript('tell application "Simulator" to activate\ndelay 0.3')
    if wait_for_window():
        return
    restart_simulator_app()
    osascript('tell application "Simulator" to activate\ndelay 0.3')
    if not wait_for_window():
        raise InfraFailure(
            "ax_wedge",
            "Simulator exposes 0 windows / -1719 after one automatic restart",
        )


def wait_for_window(retries: int = 30) -> bool:
    """Block until the Simulator process exposes a window (it briefly has none
    right after launch / `open`)."""
    for _ in range(retries):
        out = osascript(
            'tell application "System Events"\n'
            '  if not (exists process "Simulator") then return "0"\n'
            '  tell process "Simulator" to return (count of windows) as string\n'
            'end tell'
        )
        try:
            if int(out.strip()) >= 1:
                return True
        except ValueError:
            pass
        subprocess.run(["open", "-a", "Simulator"], check=False)
        time.sleep(0.5)
    return False


def ax_element_frame(role: str, desc: str | None, retries: int = 12) -> tuple[float, float, float, float] | None:
    """Return (x, y, w, h) in screen points for the first AX element matching
    role (and description, if given). Retries because `entire contents` is
    occasionally empty right after a view transition."""
    desc_match = "*" if desc is None else desc
    script = f'''
    tell application "System Events"
      if not (exists process "Simulator") then return "NOSIM"
      tell process "Simulator"
        set frontmost to true
        try
          set els to entire contents of window 1
        on error
          return "NOWINDOW"
        end try
        repeat with e in els
          set r to ""
          try
            set r to role of e
          end try
          if r is "{role}" then
            set d to ""
            try
              set d to (description of e) as string
            end try
            if "{desc_match}" is "*" or d is "{desc_match}" then
              set p to position of e
              set s to size of e
              return ((item 1 of p) as string) & " " & ((item 2 of p) as string) & " " & ((item 1 of s) as string) & " " & ((item 2 of s) as string)
            end if
          end if
        end repeat
      end tell
    end tell
    return "NOTFOUND"
    '''
    for attempt in range(retries):
        out = osascript(script)
        parts = out.split()
        if len(parts) == 4:
            try:
                return tuple(float(p) for p in parts)  # type: ignore[return-value]
            except ValueError:
                pass
        if out in ("NOWINDOW", "NOSIM") or out.startswith("ERR:"):
            wait_for_window()
        time.sleep(min(0.5 + 0.2 * attempt, 1.5))
    return None


def dump_ax_tree() -> str:
    script = '''
    tell application "System Events" to tell process "Simulator"
      set frontmost to true
      set out to ""
      try
        set els to entire contents of window 1
      on error errMsg
        return "no window: " & errMsg
      end try
      repeat with e in els
        set r to "?"
        try
          set r to role of e
        end try
        set d to "?"
        try
          set d to (description of e) as string
        end try
        set px to "?"
        set py to "?"
        try
          set p to position of e
          set px to (item 1 of p) as string
          set py to (item 2 of p) as string
        end try
        set out to out & r & " @" & px & "," & py & " [" & d & "]" & linefeed
      end repeat
      return out
    end tell
    '''
    return osascript(script)


# ---------------------------------------------------------------------------
# Quartz CGEvent helpers
# ---------------------------------------------------------------------------
def _post(ev) -> None:
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, ev)


def click(x: float, y: float) -> None:
    pt = Quartz.CGPointMake(x, y)
    _post(Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, pt, 0))
    time.sleep(0.05)
    _post(Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseDown, pt, 0))
    time.sleep(0.06)
    _post(Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventLeftMouseUp, pt, 0))


def _key(code: int, down: bool, flags: int = 0) -> None:
    ev = Quartz.CGEventCreateKeyboardEvent(None, code, down)
    if flags:
        Quartz.CGEventSetFlags(ev, flags)
    _post(ev)


def press_return() -> None:
    _key(KEY_RETURN, True)
    time.sleep(0.03)
    _key(KEY_RETURN, False)


def paste_cmd_v() -> None:
    # Explicit Cmd hold around 'v' — a lone Command flag is dropped and types "v".
    _key(KEY_CMD, True, CMD)
    time.sleep(0.05)
    _key(KEY_V, True, CMD)
    time.sleep(0.05)
    _key(KEY_V, False, CMD)
    time.sleep(0.05)
    _key(KEY_CMD, False, 0)


def click_ax(role: str, desc: str | None, label: str) -> None:
    frame = ax_element_frame(role, desc)
    if frame is None:
        raise InfraFailure(
            "ax_element_missing",
            f"could not locate {label} (role={role}, desc={desc}). "
            f"Run with --dump-ax to inspect the AX tree; is the app on screen?",
        )
    x, y, w, h = frame
    cx, cy = x + w / 2.0, y + h / 2.0
    log(f"click {label} at ({cx:.0f},{cy:.0f})")
    click(cx, cy)


def ax_static_text_dump() -> str:
    script = '''
    tell application "System Events" to tell process "Simulator"
      set frontmost to true
      set out to ""
      try
        set els to entire contents of window 1
      on error errMsg
        return "ERR:" & errMsg
      end try
      repeat with e in els
        set r to ""
        try
          set r to role of e
        end try
        if r is "AXStaticText" then
          set v to ""
          try
            set v to (value of e) as string
          end try
          if v is "" then
            try
              set v to (description of e) as string
            end try
          end if
          set out to out & v & linefeed
        end if
      end repeat
      return out
    end tell
    '''
    out = osascript(script)
    if out.startswith("ERR:"):
        raise InfraFailure("ax_read_failed", out)
    return out


def reset_repl_state() -> None:
    click_ax(RESET_ROLE, RESET_DESC, "REPL Reset button")
    time.sleep(0.8)
    text = ax_static_text_dump()
    if "[ms]" in text or " ms]" in text:
        raise InfraFailure(
            "repl_reset_unverified",
            "REPL output still contains timing text after Reset",
        )
    if "Julia REPL (" in text and "0 eval" not in text:
        raise InfraFailure(
            "repl_reset_unverified",
            "REPL eval counter did not reset to 0 before paste",
        )


# ---------------------------------------------------------------------------
# Main flow
# ---------------------------------------------------------------------------
def paste_and_run(args) -> None:
    code = args.code if args.code is not None else open(args.code_file, encoding="utf-8").read()

    if args.launch:
        log("launching app")
        launch_app(args.device, args.bundle_id, relaunch=True)
        time.sleep(3.0)

    log("setting device pasteboard")
    set_pasteboard(args.device, code)

    activate_simulator()
    time.sleep(0.3)

    # 1) REPL tab
    click_ax(REPL_TAB_ROLE, REPL_TAB_DESC, "REPL tab")
    time.sleep(1.0)
    reset_repl_state()

    # 2) input field
    click_ax(INPUT_ROLE, None, "REPL input field")
    time.sleep(0.6)

    # 3) paste
    log("pasting (Cmd+V)")
    paste_cmd_v()
    time.sleep(0.8)

    # 4) run
    if not args.no_run:
        log("pressing Enter to evaluate")
        press_return()

    if args.screenshot:
        log(f"waiting {args.wait}s for output, then screenshot")
        time.sleep(args.wait)
        screenshot(args.device, args.screenshot)
        log(f"screenshot -> {args.screenshot}")
    log("done")


def main() -> None:
    p = argparse.ArgumentParser(description="Paste & run Julia code in the iOS Simulator REPL.")
    p.add_argument("--device", default="booted", help='Simulator UDID or "booted" (default).')
    p.add_argument("--bundle-id", default=BUNDLE_ID_DEFAULT, help="App bundle id.")
    src = p.add_mutually_exclusive_group()
    src.add_argument("--code-file", help="Path to a .jl file to paste.")
    src.add_argument("--code", help="Inline Julia code to paste.")
    p.add_argument("--screenshot", help="Write a PNG screenshot here after running.")
    p.add_argument("--wait", type=float, default=6.0, help="Seconds to wait before the screenshot (default 6).")
    p.add_argument("--no-run", action="store_true", help="Paste but do not press Enter.")
    p.add_argument("--launch", action="store_true", help="Relaunch the app before pasting.")
    p.add_argument("--dump-ax", action="store_true", help="Print the Simulator window AX tree and exit.")
    args = p.parse_args()

    try:
        if args.dump_ax:
            activate_simulator()
            time.sleep(0.3)
            print(dump_ax_tree())
            return

        if args.code is None and args.code_file is None:
            die("provide --code-file or --code (or use --dump-ax).")

        paste_and_run(args)
    except InfraFailure as exc:
        die(str(exc), code=2)


if __name__ == "__main__":
    main()
