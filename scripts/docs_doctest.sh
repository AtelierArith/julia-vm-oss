#!/usr/bin/env bash
# docs_doctest.sh
#
# Extract ```julia-doctest fences from docs/vm/*.md, run each snippet with
# sjulia, and compare stdout with the text below a `# output` marker. When
# upstream julia is available, run the same snippet there as an additional
# documentation-parity check. Set DOCS_DOCTEST_SKIP_UPSTREAM=1 to skip the
# upstream run.
#
# Usage:
#   bash scripts/docs_doctest.sh [docs/vm/FILE.md ...]
#
# Environment:
#   SJULIA_BIN                  sjulia executable (default: ./target/release/sjulia)
#   DOCS_DOCTEST_SKIP_UPSTREAM  set to 1 to skip upstream julia comparison

set -euo pipefail

if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: python3 not found on PATH." >&2
    exit 2
fi

python3 - "$@" <<'PY'
import difflib
import glob
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional


@dataclass
class Block:
    path: Path
    start_line: int
    code: str
    expected: str


def normalize_output(text: str) -> str:
    return text.replace("\r\n", "\n").rstrip("\n")


def discover_files(args: List[str]) -> List[Path]:
    if args:
        files = [Path(arg) for arg in args]
    else:
        files = [Path(path) for path in sorted(glob.glob("docs/vm/*.md"))]
    missing = [str(path) for path in files if not path.is_file()]
    if missing:
        print("ERROR: markdown file(s) not found:", file=sys.stderr)
        for path in missing:
            print(f"  {path}", file=sys.stderr)
        sys.exit(2)
    return files


def extract_blocks(path: Path) -> List[Block]:
    lines = path.read_text(encoding="utf-8").splitlines()
    blocks: List[Block] = []
    in_block = False
    start_line = 0
    code_lines: List[str] = []
    output_lines: List[str] = []
    seen_output = False

    for line_number, line in enumerate(lines, start=1):
        if not in_block:
            if line.strip() == "```julia-doctest":
                in_block = True
                start_line = line_number
                code_lines = []
                output_lines = []
                seen_output = False
            continue

        if line.strip() == "```":
            if not seen_output:
                print(
                    f"ERROR: {path}:{start_line}: julia-doctest block is missing '# output'",
                    file=sys.stderr,
                )
                sys.exit(1)
            blocks.append(
                Block(
                    path=path,
                    start_line=start_line,
                    code="\n".join(code_lines).rstrip("\n") + "\n",
                    expected=normalize_output("\n".join(output_lines)),
                )
            )
            in_block = False
            continue

        if line.strip() == "# output" and not seen_output:
            seen_output = True
            continue

        if seen_output:
            output_lines.append(line)
        else:
            code_lines.append(line)

    if in_block:
        print(f"ERROR: {path}:{start_line}: unterminated julia-doctest block", file=sys.stderr)
        sys.exit(1)
    return blocks


def run_snippet(command: List[str], code: str, tempdir: Path):
    script = tempdir / "snippet.jl"
    script.write_text(code, encoding="utf-8")
    proc = subprocess.run(
        [*command, str(script)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    return proc.returncode, normalize_output(proc.stdout), proc.stderr


def report_mismatch(block: Block, runner: str, expected: str, actual: str) -> None:
    print(f"ERROR: {block.path}:{block.start_line}: {runner} output mismatch", file=sys.stderr)
    print("expected:", file=sys.stderr)
    print(expected, file=sys.stderr)
    print("actual:", file=sys.stderr)
    print(actual, file=sys.stderr)
    diff = difflib.unified_diff(
        expected.splitlines(),
        actual.splitlines(),
        fromfile="expected",
        tofile=runner,
        lineterm="",
    )
    for line in diff:
        print(line, file=sys.stderr)


def main() -> int:
    files = discover_files(sys.argv[1:])
    blocks = [block for path in files for block in extract_blocks(path)]
    if not blocks:
        print("OK: 0 julia-doctest block(s) passed.")
        return 0

    sjulia_bin = os.environ.get("SJULIA_BIN", "./target/release/sjulia")
    if not os.access(sjulia_bin, os.X_OK):
        print(f"ERROR: sjulia binary not executable: {sjulia_bin}", file=sys.stderr)
        print("Build it or set SJULIA_BIN to an existing executable.", file=sys.stderr)
        return 2

    upstream_cmd: Optional[List[str]] = None
    if os.environ.get("DOCS_DOCTEST_SKIP_UPSTREAM") != "1":
        julia = shutil.which("julia")
        if julia:
            upstream_cmd = [julia, "--startup-file=no"]
        else:
            print("SKIP: upstream julia not on PATH; sjulia doctests still run.", file=sys.stderr)

    failures = 0
    with tempfile.TemporaryDirectory(prefix="docs-doctest-") as tmp:
        tempdir = Path(tmp)
        for block in blocks:
            code = block.code
            expected = block.expected
            status, actual, stderr = run_snippet([sjulia_bin], code, tempdir)
            if status != 0:
                failures += 1
                print(
                    f"ERROR: {block.path}:{block.start_line}: sjulia exited with {status}",
                    file=sys.stderr,
                )
                print(stderr, file=sys.stderr)
                continue
            if actual != expected:
                failures += 1
                report_mismatch(block, "sjulia", expected, actual)
                continue

            if upstream_cmd is not None:
                status, upstream_actual, stderr = run_snippet(upstream_cmd, code, tempdir)
                if status != 0:
                    failures += 1
                    print(
                        f"ERROR: {block.path}:{block.start_line}: upstream julia exited with {status}",
                        file=sys.stderr,
                    )
                    print(stderr, file=sys.stderr)
                elif upstream_actual != expected:
                    failures += 1
                    report_mismatch(block, "julia", expected, upstream_actual)

    if failures:
        print(f"ERROR: {failures} julia-doctest block(s) failed.", file=sys.stderr)
        return 1

    print(f"OK: {len(blocks)} julia-doctest block(s) passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
PY
