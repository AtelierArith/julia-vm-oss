#!/usr/bin/env python3
"""Differential fuzz runner for Issue #8716.

The runner consumes deterministic programs from scripts/differential_fuzz_generate.jl,
validates each generated program under upstream Julia, runs the same source under
sjulia, classifies divergences, and emits JSONL records with a stable fingerprint.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class Case:
    seed: int
    index: int
    depth: int
    source: str


@dataclass(frozen=True)
class RunResult:
    status: str
    stdout: str
    stderr: str
    exception_kind: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--count", type=int, required=True)
    parser.add_argument("--max-depth", type=int, default=4)
    parser.add_argument("--timeout-sec", type=int, default=10)
    parser.add_argument(
        "--budget-sec",
        type=int,
        default=0,
        help="Stop after this wall-clock budget. 0 means run all requested cases.",
    )
    parser.add_argument("--julia-bin", default=os.environ.get("JULIA_BIN", "julia"))
    parser.add_argument("--sjulia-bin", default=os.environ.get("SJULIA_BIN", "./target/release/sjulia"))
    parser.add_argument("--out-jsonl", required=True)
    parser.add_argument("--work-dir", default="target/differential-fuzz")
    parser.add_argument(
        "--inject-known-mismatch",
        action="store_true",
        help="Self-test hook: perturb the first sjulia result after execution.",
    )
    return parser.parse_args()


def generator_cmd(seed: int, count: int, max_depth: int, mode: str = "programs", case_index: int = 1) -> list[str]:
    return [
        "julia",
        "--startup-file=no",
        str(ROOT / "scripts" / "differential_fuzz_generate.jl"),
        "--seed",
        str(seed),
        "--count",
        str(count),
        "--max-depth",
        str(max_depth),
        "--mode",
        mode,
        "--case-index",
        str(case_index),
    ]


def load_cases(seed: int, count: int, max_depth: int, mode: str = "programs", case_index: int = 1) -> list[Case]:
    proc = subprocess.run(
        generator_cmd(seed, count, max_depth, mode, case_index),
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )
    rows = [line.split("\t") for line in proc.stdout.splitlines() if line.strip()]
    if not rows or rows[0] != ["case_seed", "case_index", "depth", "source_b64"]:
        raise RuntimeError(f"unexpected generator output: {proc.stdout!r}\n{proc.stderr}")
    cases: list[Case] = []
    for row in rows[1:]:
        if len(row) != 4:
            raise RuntimeError(f"unexpected generator row: {row!r}")
        source = base64.b64decode(row[3]).decode("utf-8")
        cases.append(Case(int(row[0]), int(row[1]), int(row[2]), source))
    return cases


def exception_kind(stderr: str, stdout: str) -> str:
    text = stderr + "\n" + stdout
    patterns = [
        r"ERROR:\s*([A-Za-z_][A-Za-z0-9_.]*)",
        r"Runtime error:\s*([A-Za-z_][A-Za-z0-9_.]*)",
        r"Pipeline error:\s*([A-Za-z_][A-Za-z0-9_.]*)",
        r"Compilation error:\s*([A-Za-z_][A-Za-z0-9_.]*)",
    ]
    for pattern in patterns:
        match = re.search(pattern, text)
        if match:
            return match.group(1).split(".")[-1]
    return ""


def run_source(cmd: list[str], source_path: Path, timeout_sec: int) -> RunResult:
    try:
        proc = subprocess.run(
            [*cmd, str(source_path)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_sec,
        )
    except subprocess.TimeoutExpired as exc:
        return RunResult("timeout", exc.stdout or "", exc.stderr or "", "timeout")
    status = "pass" if proc.returncode == 0 else "error"
    return RunResult(status, proc.stdout, proc.stderr, exception_kind(proc.stderr, proc.stdout))


def run_parse_checks(args: argparse.Namespace, source_path: Path) -> tuple[RunResult, RunResult]:
    upstream = run_source(
        [
            args.julia_bin,
            "--startup-file=no",
            "-e",
            'Meta.parseall(read(ARGS[1], String)); println("parse-ok")',
        ],
        source_path,
        args.timeout_sec,
    )
    sjulia = run_source([args.sjulia_bin, "--dump-ast"], source_path, args.timeout_sec)
    return (
        RunResult(upstream.status, "", upstream.stderr, upstream.exception_kind),
        RunResult(sjulia.status, "", sjulia.stderr, sjulia.exception_kind),
    )


def normalize_output(text: str) -> str:
    return text.replace("\r\n", "\n").strip()


def fingerprint(source: str, failure_kind: str) -> str:
    normalized = re.sub(r"\s+", " ", source).strip()
    payload = f"{failure_kind}\n{normalized}".encode("utf-8")
    return hashlib.sha256(payload).hexdigest()[:16]


def classify(upstream: RunResult, sjulia: RunResult) -> tuple[str, str]:
    if upstream.status == "timeout":
        return "skip", "upstream_timeout"
    if upstream.status != "pass":
        return "skip", "upstream_invalid"
    if sjulia.status == "timeout":
        return "fail", "sjulia_timeout"
    if sjulia.status != "pass":
        return "fail", "sjulia_error"
    if normalize_output(upstream.stdout) != normalize_output(sjulia.stdout):
        return "fail", "stdout_mismatch"
    return "pass", ""


def classify_parse(upstream: RunResult, sjulia: RunResult) -> tuple[str, str]:
    if upstream.status == "timeout":
        return "skip", "upstream_parse_timeout"
    if upstream.status != "pass":
        return "skip", "upstream_parse_invalid"
    if sjulia.status == "timeout":
        return "fail", "sjulia_parse_timeout"
    if sjulia.status != "pass":
        return "fail", "sjulia_parse_error"
    return "pass", ""


def write_source(work_dir: Path, case: Case, suffix: str = "") -> Path:
    path = work_dir / f"seed_{case.seed}_case_{case.index}{suffix}.jl"
    path.write_text(case.source, encoding="utf-8")
    return path


def reproduces_failure(case: Case, args: argparse.Namespace, failure_kind: str, work_dir: Path) -> bool:
    source_path = write_source(work_dir, case, suffix="_shrink")
    upstream_parse, sjulia_parse = run_parse_checks(args, source_path)
    status, kind = classify_parse(upstream_parse, sjulia_parse)
    if status == "pass":
        upstream = run_source([args.julia_bin, "--startup-file=no"], source_path, args.timeout_sec)
        sjulia = run_source([args.sjulia_bin], source_path, args.timeout_sec)
        status, kind = classify(upstream, sjulia)
    return status == "fail" and kind == failure_kind


def shrink(case: Case, args: argparse.Namespace, failure_kind: str, work_dir: Path) -> str:
    best = case
    changed = True
    while changed:
        changed = False
        for candidate in load_cases(best.seed, 1, args.max_depth, mode="shrinks", case_index=best.index):
            if len(candidate.source) >= len(best.source):
                continue
            if reproduces_failure(candidate, args, failure_kind, work_dir):
                best = candidate
                changed = True
                break
    return best.source


def record_for_case(case: Case, args: argparse.Namespace, work_dir: Path) -> dict[str, object]:
    source_path = write_source(work_dir, case)
    upstream_parse, sjulia_parse = run_parse_checks(args, source_path)
    status, failure_kind = classify_parse(upstream_parse, sjulia_parse)
    upstream = RunResult("skip", "", "", "")
    sjulia = RunResult("skip", "", "", "")
    if status == "pass":
        upstream = run_source([args.julia_bin, "--startup-file=no"], source_path, args.timeout_sec)
        sjulia = run_source([args.sjulia_bin], source_path, args.timeout_sec)
        if args.inject_known_mismatch and case.index == 1 and upstream.status == "pass" and sjulia.status == "pass":
            sjulia = RunResult(sjulia.status, sjulia.stdout + "\n# injected mismatch\n", sjulia.stderr, sjulia.exception_kind)
        status, failure_kind = classify(upstream, sjulia)
    shrunk_source = ""
    if status == "fail" and not args.inject_known_mismatch:
        shrunk_source = shrink(case, args, failure_kind, work_dir)
    elif status == "fail":
        shrunk_source = case.source
    return {
        "seed": case.seed,
        "case_index": case.index,
        "depth": case.depth,
        "status": status,
        "failure_kind": failure_kind,
        "fingerprint": fingerprint(shrunk_source or case.source, failure_kind),
        "source": case.source,
        "shrunk_source": shrunk_source,
        "upstream_parse": {
            "status": upstream_parse.status,
            "stdout": upstream_parse.stdout,
            "stderr": upstream_parse.stderr,
            "exception_kind": upstream_parse.exception_kind,
        },
        "sjulia_parse": {
            "status": sjulia_parse.status,
            "stdout": sjulia_parse.stdout,
            "stderr": sjulia_parse.stderr,
            "exception_kind": sjulia_parse.exception_kind,
        },
        "upstream": {
            "status": upstream.status,
            "stdout": upstream.stdout,
            "stderr": upstream.stderr,
            "exception_kind": upstream.exception_kind,
        },
        "sjulia": {
            "status": sjulia.status,
            "stdout": sjulia.stdout,
            "stderr": sjulia.stderr,
            "exception_kind": sjulia.exception_kind,
        },
    }


def main() -> int:
    args = parse_args()
    started_at = time.monotonic()
    work_dir = Path(args.work_dir)
    if not work_dir.is_absolute():
        work_dir = ROOT / work_dir
    work_dir.mkdir(parents=True, exist_ok=True)
    out_path = Path(args.out_jsonl)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    any_failure = False
    with out_path.open("w", encoding="utf-8") as out:
        for case in load_cases(args.seed, args.count, args.max_depth):
            if args.budget_sec > 0 and time.monotonic() - started_at >= args.budget_sec:
                break
            row = record_for_case(case, args, work_dir)
            if row["status"] == "fail":
                any_failure = True
            out.write(json.dumps(row, sort_keys=True) + "\n")

    return 1 if any_failure else 0


if __name__ == "__main__":
    sys.exit(main())
