#!/usr/bin/env python3
"""Run REPL session fixtures against upstream Julia."""

from __future__ import annotations

import argparse
import base64
import json
import subprocess
import sys
import tempfile
import textwrap
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE_DIR = ROOT / "subset_julia_vm" / "tests" / "fixtures" / "repl_session"


def fixture_paths(paths: list[Path]) -> list[Path]:
    if not paths:
        paths = [DEFAULT_FIXTURE_DIR]
    expanded: list[Path] = []
    for path in paths:
        if path.is_dir():
            expanded.extend(sorted(path.glob("*.toml")))
        else:
            expanded.append(path)
    return expanded


def load_fixture(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def b64_text(value: str) -> str:
    return base64.b64encode(value.encode()).decode()


def build_julia_driver(fixture: dict) -> str:
    encoded_steps = [b64_text(step["input"]) for step in fixture["steps"]]
    encoded_literal = ", ".join(json.dumps(step) for step in encoded_steps)
    return textwrap.dedent(
        f"""
        using Base64
        using REPL

        const __sjulia_repl_module = Module(:SJULIAReplSessionFixture)
        const __sjulia_steps = String[{encoded_literal}]

        function __sjulia_b64(s)
            base64encode(String(s))
        end

        function __sjulia_display(value)
            value === nothing && return ""
            sprint(show, MIME("text/plain"), value)
        end

        function __sjulia_suppresses_display(source)
            endswith(rstrip(source), ";")
        end

        function __sjulia_assign_ans(value)
            value === nothing && return
            Core.eval(__sjulia_repl_module, Expr(:(=), :ans, value))
        end

        function __sjulia_run_step(index, encoded)
            source = String(base64decode(encoded))
            old_stdout = stdout
            rd, wr = redirect_stdout()
            try
                expr = Meta.parseall(source; filename="repl_session_step_$index")
                result = Core.eval(__sjulia_repl_module, REPL.softscope(expr))
                redirect_stdout(old_stdout)
                close(wr)
                captured = String(read(rd))
                __sjulia_assign_ans(result)
                display = __sjulia_suppresses_display(source) ? "" : __sjulia_display(result)
                println(
                    "__SJULIA_REPL_STEP__\\t", index, "\\ttrue\\t",
                    __sjulia_b64(captured), "\\t",
                    __sjulia_b64(display), "\\t",
                    __sjulia_b64(""),
                )
            catch err
                redirect_stdout(old_stdout)
                close(wr)
                captured = String(read(rd))
                println(
                    "__SJULIA_REPL_STEP__\\t", index, "\\tfalse\\t",
                    __sjulia_b64(captured), "\\t",
                    __sjulia_b64(""), "\\t",
                    __sjulia_b64(sprint(showerror, err)),
                )
            end
        end

        for (index, encoded) in enumerate(__sjulia_steps)
            __sjulia_run_step(index, encoded)
        end
        """
    )


def decode_field(value: str) -> str:
    return base64.b64decode(value.encode()).decode()


def run_fixture(path: Path, julia: str) -> list[dict]:
    fixture = load_fixture(path)
    driver = build_julia_driver(fixture)
    with tempfile.NamedTemporaryFile("w", suffix=".jl", delete=False) as handle:
        handle.write(driver)
        driver_path = Path(handle.name)
    try:
        proc = subprocess.run(
            [julia, "--startup-file=no", "--color=no", str(driver_path)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    finally:
        driver_path.unlink(missing_ok=True)

    if proc.returncode != 0:
        raise RuntimeError(
            f"julia driver failed for {path} with exit {proc.returncode}\\n"
            f"stdout:\\n{proc.stdout}\\nstderr:\\n{proc.stderr}"
        )

    observed = []
    for line in proc.stdout.splitlines():
        if not line.startswith("__SJULIA_REPL_STEP__\t"):
            continue
        _, index, success, stdout, display, error = line.split("\t", 5)
        observed.append(
            {
                "index": int(index),
                "success": success == "true",
                "stdout": decode_field(stdout),
                "display": decode_field(display) or None,
                "error": decode_field(error) or None,
            }
        )
    if len(observed) != len(fixture["steps"]):
        raise RuntimeError(
            f"{path}: expected {len(fixture['steps'])} observed steps, got {len(observed)}"
        )
    return observed


def check_fixture(path: Path, observed: list[dict]) -> list[str]:
    fixture = load_fixture(path)
    failures: list[str] = []
    for step, actual in zip(fixture["steps"], observed, strict=True):
        label = f"{path.name} step {actual['index']} ({step['name']})"
        if actual["success"] != step["success"]:
            failures.append(f"{label}: success {actual['success']} != {step['success']}")
        if actual["stdout"] != step.get("stdout", ""):
            failures.append(f"{label}: stdout {actual['stdout']!r} != {step.get('stdout', '')!r}")
        if step["success"]:
            if actual["display"] != step.get("display"):
                failures.append(
                    f"{label}: display {actual['display']!r} != {step.get('display')!r}"
                )
        else:
            expected = step.get("error_contains")
            if expected and expected not in (actual["error"] or ""):
                failures.append(f"{label}: error {actual['error']!r} lacks {expected!r}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixtures", nargs="*", type=Path)
    parser.add_argument("--julia", default="julia")
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare upstream Julia to fixture expectations",
    )
    args = parser.parse_args()

    all_failures: list[str] = []
    for path in fixture_paths(args.fixtures):
        observed = run_fixture(path, args.julia)
        print(json.dumps({"fixture": str(path), "steps": observed}, ensure_ascii=False))
        if args.check:
            all_failures.extend(check_fixture(path, observed))

    if all_failures:
        for failure in all_failures:
            print(failure, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
