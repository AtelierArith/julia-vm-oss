#!/usr/bin/env -S uv run --quiet
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyobjc-framework-Quartz"]
# ///
"""
Full E2E sweep: run *every* shipped sample through the SubsetJuliaVM iOS app,
either via the Editor (Run button) or via the REPL (paste + Enter).

For each sample listed in `samples.json` this loads the sample's `.jl` source and:
  * `--mode editor` (default): switches to the Editor tab, replaces the code
    (select-all + paste), Clears the previous output, taps Run.
  * `--mode repl`: switches to the REPL tab, Resets the session, pastes the code
    into the input, presses Enter.
Then it waits, screenshots, and makes a best-effort read of the output to flag
samples whose output contains an error (UndefVarError / MethodError / parse error
/ stack overflow / …), printing a PASS/FAIL/UNKNOWN summary + a report.txt.

It reuses the low-level driver in `ios_repl_paste.py` (Quartz CGEvents + AX-tree
element lookup); see that file for the Accessibility/uv requirements. Build the
app once via `scripts/ios_repl_e2e.sh --build` (or pass --launch).

Examples
--------
    uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --launch
    uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --mode repl --launch
    uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --only plotting_2d,fizzbuzz
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time

# Reuse the proven low-level driver (same directory).
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ios_repl_paste as drv  # noqa: E402
import ios_e2e_report as e2e_report  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAMPLES_DIR = os.path.join(
    REPO_ROOT, "SubsetJuliaVMApp", "SubsetJuliaVMApp", "Resources", "Samples"
)
EDITOR_TAB_ROLE, EDITOR_TAB_DESC = "AXRadioButton", "Editor"
RUN_ROLE, RUN_DESC = "AXButton", "Run"
CLEAR_ROLE, CLEAR_DESC = "AXButton", "Clear"
KEY_A = 0  # 'a'

ERROR_MARKERS = (
    "Error:", "ERROR", "UndefVarError", "MethodError", "BoundsError",
    "TypeError", "DimensionMismatch", "StackOverflow", "not defined",
    "no method", "Parse error", "ParseError", "unsupported", "not implemented",
    "panic", "InexactError", "DomainError", "ArgumentError", "KeyError",
)


def enumerate_samples():
    data = json.load(open(os.path.join(SAMPLES_DIR, "samples.json"), encoding="utf-8"))
    found, seen = [], set()

    def walk(o):
        if isinstance(o, list):
            for x in o:
                walk(x)
        elif isinstance(o, dict):
            sid = o.get("id")
            if sid and sid not in seen and ("folder" in o or "category" in o):
                folder = o.get("folder", "")
                jl = os.path.join(SAMPLES_DIR, folder, f"{sid}.jl")
                if os.path.exists(jl):
                    seen.add(sid)
                    found.append({"id": sid, "name": o.get("name", sid), "file": jl})
            for v in o.values():
                walk(v)

    walk(data)
    return found


def select_all() -> None:
    drv._key(drv.KEY_CMD, True, drv.CMD)
    time.sleep(0.04)
    drv._key(KEY_A, True, drv.CMD)
    time.sleep(0.04)
    drv._key(KEY_A, False, drv.CMD)
    time.sleep(0.04)
    drv._key(drv.KEY_CMD, False, 0)


def window_frame():
    """(x, y, w, h) of the Simulator device window in screen points, or None."""
    out = drv.osascript(
        'tell application "System Events" to tell process "Simulator"\n'
        '  set p to position of window 1\n'
        '  set s to size of window 1\n'
        '  return ((item 1 of p) as string) & " " & ((item 2 of p) as string) & " " '
        '& ((item 1 of s) as string) & " " & ((item 2 of s) as string)\n'
        'end tell'
    )
    parts = out.split()
    if len(parts) == 4:
        try:
            return tuple(float(p) for p in parts)
        except ValueError:
            return None
    return None


def click_center(role, desc, label):
    frame = drv.ax_element_frame(role, desc)
    if frame is None:
        return False
    x, y, w, h = frame
    drv.click(x + w / 2.0, y + h / 2.0)
    return True


def focus_code_editor():
    """Click into the code editor. Prefer the code AXScrollArea; fall back to a
    window-relative point in the upper (code) region."""
    frame = drv.ax_element_frame("AXScrollArea", None)
    if frame is not None:
        x, y, w, h = frame
        drv.click(x + w / 2.0, y + h / 2.0)
        return
    wf = window_frame()
    if wf is None:
        drv.die("cannot locate the code editor (no AXScrollArea, no window frame)")
    x, y, w, h = wf
    drv.click(x + 0.5 * w, y + 0.50 * h)


def read_errors(threshold_frac):
    """Best-effort: read AXStaticText whose y is below `threshold_frac` of the
    window height (Editor: the Output region; REPL: pass 0.0 to scan the whole
    history). Returns the first error marker line, None if clean, or "UNKNOWN"
    if AX can't be read."""
    wf = window_frame()
    if wf is None:
        return "UNKNOWN"
    _, wy, _, wh = wf
    threshold = wy + threshold_frac * wh
    script = '''
    tell application "System Events" to tell process "Simulator"
      set out to ""
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
        if r is "AXStaticText" then
          set yy to -100000
          try
            set yy to (item 2 of (position of e))
          end try
          if yy > %d then
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
        end if
      end repeat
      return out
    end tell
    ''' % int(threshold)
    # `entire contents` is flaky (sometimes returns an empty/partial tree). Retry
    # until we get a non-trivial read, and treat a persistently empty read as
    # UNKNOWN rather than PASS — an empty read must NOT be reported as success.
    out = ""
    for _ in range(8):
        out = drv.osascript(script)
        if out and out not in ("NOWINDOW",) and not out.startswith("ERR:") and out.strip():
            break
        time.sleep(0.5)
    if out in ("NOWINDOW",) or out.startswith("ERR:") or not out.strip():
        return "UNKNOWN"
    for line in out.splitlines():
        for m in ERROR_MARKERS:
            if m in line:
                return line.strip()[:160]
    return None


def _classify(err, shot):
    if err is None:
        return (e2e_report.SAMPLE_PASS, shot)
    if err == "UNKNOWN":
        return (e2e_report.INFRA_FAILURE, f"AX output read unavailable ({shot})")
    return (e2e_report.SAMPLE_FAIL, f"{err}  ({shot})")


def run_one_editor(args, idx, sample):
    code = open(sample["file"], encoding="utf-8").read()
    drv.set_pasteboard(args.device, code)
    drv.activate_simulator()
    time.sleep(0.2)

    if not click_center(EDITOR_TAB_ROLE, EDITOR_TAB_DESC, "Editor tab"):
        return ("UNKNOWN", "could not click Editor tab")
    time.sleep(0.9)

    focus_code_editor()
    time.sleep(0.4)
    select_all()
    time.sleep(0.2)
    drv.paste_cmd_v()
    time.sleep(0.6)

    # Clear the previous sample's output so the screenshot + error scan reflect
    # only this sample (text output accumulates otherwise).
    click_center(CLEAR_ROLE, CLEAR_DESC, "Clear")
    time.sleep(0.3)

    if not click_center(RUN_ROLE, RUN_DESC, "Run"):
        return ("UNKNOWN", "could not click Run")

    time.sleep(args.wait)
    shot = os.path.join(args.out_dir, f"{idx:02d}_{sample['id']}.png")
    drv.screenshot(args.device, shot)
    return _classify(read_errors(0.78), shot)


def run_one_repl(args, idx, sample):
    code = open(sample["file"], encoding="utf-8").read()
    drv.set_pasteboard(args.device, code)

    # Relaunch the app for a guaranteed-clean session (the in-app Reset button is a
    # small target on a Simulator window that keeps moving, and Ctrl/Cmd-L's
    # modifier is dropped by the Simulator). After a fresh launch the app is on the
    # Editor with a light view tree, so the AX element lookups for the REPL tab and
    # input are both accurate and fast (the plot-heavy REPL tree is what made
    # `entire contents` slow before).
    drv.launch_app(args.device, args.bundle_id, relaunch=True)
    time.sleep(3.0)
    drv.activate_simulator()
    time.sleep(0.3)

    if not click_center("AXRadioButton", "REPL", "REPL tab"):
        return ("UNKNOWN", "could not click REPL tab")
    time.sleep(1.0)
    if not click_center("AXTextField", None, "REPL input"):
        return ("UNKNOWN", "could not click REPL input")
    time.sleep(0.5)
    drv.paste_cmd_v()
    time.sleep(0.8)
    drv.press_return()

    time.sleep(args.wait)
    shot = os.path.join(args.out_dir, f"{idx:02d}_{sample['id']}.png")
    drv.screenshot(args.device, shot)
    # AX text-reads of the REPL history are unreliable (the plot-heavy tree often
    # returns 0 elements), so screenshot capture is the source of truth here.
    return (e2e_report.SAMPLE_PASS, shot)


def run_with_infra_retry(args, idx, sample, run_one):
    max_attempts = args.infra_retries + 1
    detail = ""
    infra_failure_type = getattr(drv, "InfraFailure", RuntimeError)
    for attempt in range(1, max_attempts + 1):
        try:
            status, detail = run_one(args, idx, sample)
            status = e2e_report.normalize_status(status)
        except infra_failure_type as exc:
            status, detail = e2e_report.INFRA_FAILURE, str(exc)
        except Exception as exc:  # harness failures are infra, not sample verdicts
            status, detail = e2e_report.INFRA_FAILURE, repr(exc)

        if not e2e_report.should_retry(status, attempt=attempt, max_attempts=max_attempts):
            return e2e_report.ReportRow(sample["id"], status, detail, attempt)
        drv.log(
            f"infra failure for {sample['id']} (attempt {attempt}/{max_attempts}): {detail}; retrying"
        )
    return e2e_report.ReportRow(sample["id"], e2e_report.INFRA_FAILURE, detail, max_attempts)


def main():
    p = argparse.ArgumentParser(description="Run every Editor sample through the iOS app.")
    p.add_argument("--device", default="booted")
    p.add_argument("--bundle-id", default=drv.BUNDLE_ID_DEFAULT)
    p.add_argument("--out-dir", required=True, help="Directory for per-sample screenshots + report.")
    p.add_argument("--mode", choices=["editor", "repl"], default="editor",
                   help="Run each sample via the Editor (Run button) or the REPL (paste+Enter).")
    p.add_argument("--wait", type=float, default=10.0,
                   help="Seconds to wait after Run before screenshotting (default 10). Heavy "
                        "samples (animations, ODE solves, many plots) may need more, or the "
                        "screenshot/next relaunch can catch them mid-run.")
    p.add_argument("--only", help="Comma-separated sample ids to run (default: all).")
    p.add_argument("--launch", action="store_true", help="Relaunch the app first.")
    p.add_argument("--infra-retries", type=int, default=1,
                   help="Retry only infra failures this many times (default 1). Sample failures are never retried.")
    args = p.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    samples = enumerate_samples()
    if args.only:
        wanted = {s.strip() for s in args.only.split(",")}
        samples = [s for s in samples if s["id"] in wanted]
    if not samples:
        drv.die("no samples matched")

    if args.launch:
        drv.log("launching app")
        drv.launch_app(args.device, args.bundle_id, relaunch=True)
        time.sleep(3.0)
    drv.activate_simulator()
    # Warm up: wait until the tab bar is accessible so the first sample's tab
    # switch doesn't race the app's initial render.
    for _ in range(20):
        if drv.ax_element_frame(EDITOR_TAB_ROLE, EDITOR_TAB_DESC, retries=1) is not None:
            break
        time.sleep(0.5)

    run_one = run_one_repl if args.mode == "repl" else run_one_editor
    drv.log(f"running {len(samples)} samples in {args.mode} mode; screenshots -> {args.out_dir}")
    results = []
    for i, s in enumerate(samples, 1):
        drv.log(f"[{i}/{len(samples)}] {s['id']}")
        row = run_with_infra_retry(args, i, s, run_one)
        results.append(row)
        print(f"    {row.status}: {row.detail}", flush=True)

    # Summary + report file
    report = os.path.join(args.out_dir, "report.txt")
    summary = e2e_report.write_report(report, results)
    print("\n=== E2E summary ===", flush=True)
    print(f"  sample_pass: {summary.sample_pass}", flush=True)
    print(f"  sample_fail: {summary.sample_fail}", flush=True)
    print(f"  infra_failure: {summary.infra_failure}", flush=True)
    print(f"  sample_rate: {summary.sample_rate:.2f}%", flush=True)
    print(f"  infra_rate: {summary.infra_rate:.2f}%", flush=True)
    print(f"  report: {report}", flush=True)
    sys.exit(1 if summary.sample_fail or summary.infra_failure else 0)


if __name__ == "__main__":
    main()
