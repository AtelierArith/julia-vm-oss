#!/usr/bin/env python3
"""Inventory panic-prone Rust sources and FFI unwind boundaries.

This is a measurement tool for Issue #8705, not a ratchet. It consumes optional
`cargo clippy --message-format=json` output for lint counts and always scans the
native FFI crate for exported C ABI functions that lack a catch_unwind boundary.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

LINTS = (
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::indexing_slicing",
    "clippy::panic",
)

RUST_ROOTS = (
    Path("subset_julia_vm/src"),
    Path("subset_julia_vm_ffi/src"),
    Path("subset_julia_vm_parser/src"),
    Path("subset_julia_vm_runtime/src"),
    Path("subset_julia_vm_web/src"),
)

FFI_ROOT = Path("subset_julia_vm_ffi/src")


@dataclass(frozen=True)
class LintHit:
    lint: str
    file: str
    line: int
    message: str


@dataclass(frozen=True)
class FfiExport:
    file: str
    line: int
    name: str
    signature: str
    return_type: str
    has_raw_pointer_arg: bool
    returns_raw_pointer: bool
    has_catch_unwind: bool
    boundary_class: str


def rust_files() -> list[Path]:
    files: list[Path] = []
    for root in RUST_ROOTS:
        if root.exists():
            files.extend(sorted(root.rglob("*.rs")))
    for extra in (Path("build.rs"), Path("subset_julia_vm/build.rs")):
        if extra.exists():
            files.append(extra)
    return files


def classify_path(path: str) -> str:
    p = path.replace("\\", "/")
    if "/tests/" in p or p.endswith("_tests.rs") or "/benches/" in p:
        return "test_or_bench"
    if p.startswith("subset_julia_vm_ffi/src/"):
        return "ffi_boundary"
    if (
        p.startswith("subset_julia_vm_vm/src/vm/exec/")
        or p == "subset_julia_vm_vm/src/vm/mod.rs"
        or "dispatch" in p
        or p == "subset_julia_vm_vm/src/vm/formatting.rs"
    ):
        return "runtime_hot_path"
    if (
        p.startswith("subset_julia_vm_compile/src/compile/cache")
        or p.startswith("subset_julia_vm/src/bin/")
        or p.endswith("/build.rs")
        or p == "build.rs"
    ):
        return "startup_or_invariant"
    if p.startswith("subset_julia_vm_vm/src/vm/"):
        return "runtime_other"
    return "other"


def module_key(path: str) -> str:
    parts = path.replace("\\", "/").split("/")
    if len(parts) >= 4 and parts[0] == "subset_julia_vm" and parts[1] == "src":
        if parts[2] == "vm" and len(parts) >= 5:
            return "/".join(parts[:5]) if parts[3] == "exec" else "/".join(parts[:4])
        return "/".join(parts[:3])
    if len(parts) >= 3 and parts[0].startswith("subset_julia_vm"):
        return "/".join(parts[:3])
    return "/".join(parts[:2])


def run_clippy(jsonl_path: Path) -> int:
    cmd = [
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--message-format=json",
        "--",
        "-W",
        "clippy::unwrap_used",
        "-W",
        "clippy::expect_used",
        "-W",
        "clippy::indexing_slicing",
        "-W",
        "clippy::panic",
    ]
    with jsonl_path.open("w", encoding="utf-8") as out:
        proc = subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT, check=False)
    return proc.returncode


def parse_clippy_jsonl(path: Path) -> list[LintHit]:
    hits: list[LintHit] = []
    if not path.exists():
        return hits
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        raw = raw.strip()
        if not raw.startswith("{"):
            continue
        try:
            event = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if event.get("reason") != "compiler-message":
            continue
        message = event.get("message") or {}
        code = (message.get("code") or {}).get("code")
        if code not in LINTS:
            continue
        spans = [s for s in message.get("spans") or [] if s.get("is_primary")]
        if not spans:
            spans = message.get("spans") or []
        if not spans:
            continue
        span = spans[0]
        hits.append(
            LintHit(
                lint=code,
                file=span.get("file_name", "?"),
                line=int(span.get("line_start") or 0),
                message=message.get("message", ""),
            )
        )
    return hits


def static_panic_counts(files: Iterable[Path]) -> Counter[str]:
    patterns = {
        "unwrap_call": re.compile(r"\.unwrap\s*\("),
        "expect_call": re.compile(r"\.expect\s*\("),
        "panic_macro": re.compile(r"(?<![A-Za-z0-9_])panic!\s*\("),
        "todo_macro": re.compile(r"(?<![A-Za-z0-9_])todo!\s*\("),
        "unimplemented_macro": re.compile(r"(?<![A-Za-z0-9_])unimplemented!\s*\("),
    }
    counts: Counter[str] = Counter()
    for path in files:
        text = path.read_text(encoding="utf-8", errors="ignore")
        for name, pattern in patterns.items():
            counts[name] += len(pattern.findall(text))
    return counts


def collect_function_body(lines: list[str], start: int) -> str:
    depth = 0
    seen_open = False
    body: list[str] = []
    for line in lines[start:]:
        body.append(line)
        for ch in line:
            if ch == "{":
                depth += 1
                seen_open = True
            elif ch == "}":
                depth -= 1
        if seen_open and depth <= 0:
            break
    return "\n".join(body)


def normalize_signature(lines: list[str], start: int) -> str:
    sig_parts: list[str] = []
    for line in lines[start:]:
        sig_parts.append(line.strip())
        if "{" in line or ";" in line:
            break
    return " ".join(sig_parts)


def return_type(signature: str) -> str:
    before_brace = signature.split("{", 1)[0].strip()
    if "->" not in before_brace:
        return "()"
    return before_brace.rsplit("->", 1)[1].strip()


def ffi_boundary_class(name: str, signature: str) -> str:
    if name.startswith("free_"):
        return "destructor"
    if "result_" in name or name.startswith("execution_result_"):
        return "result_accessor"
    if name.startswith("repl_session_"):
        return "stateful_repl"
    if name.startswith("unicode_"):
        return "unicode_helper"
    if name.startswith("compile") or name.startswith("run_ir"):
        return "compile_or_execute"
    if name.startswith("vm_"):
        return "cancel_control"
    return "misc"


def scan_ffi_exports() -> list[FfiExport]:
    exports: list[FfiExport] = []
    if not FFI_ROOT.exists():
        return exports
    fn_re = re.compile(r'pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+([A-Za-z0-9_]+)')
    for path in sorted(FFI_ROOT.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8", errors="ignore").splitlines()
        for idx, line in enumerate(lines):
            if 'extern "C" fn' not in line:
                continue
            signature = normalize_signature(lines, idx)
            match = fn_re.search(signature)
            if not match:
                continue
            name = match.group(1)
            body = collect_function_body(lines, idx)
            ret = return_type(signature)
            exports.append(
                FfiExport(
                    file=str(path),
                    line=idx + 1,
                    name=name,
                    signature=signature.split("{", 1)[0].strip(),
                    return_type=ret,
                    has_raw_pointer_arg=bool(re.search(r":\s*\*(?:const|mut)\b", signature)),
                    returns_raw_pointer=ret.startswith("*"),
                    has_catch_unwind="catch_unwind" in body,
                    boundary_class=ffi_boundary_class(name, signature),
                )
            )
    return exports


def write_lint_tsv(path: Path, hits: list[LintHit]) -> None:
    with path.open("w", encoding="utf-8") as f:
        f.write("lint\tclass\tmodule\tfile\tline\tmessage\n")
        for h in hits:
            f.write(
                "\t".join(
                    [
                        h.lint,
                        classify_path(h.file),
                        module_key(h.file),
                        h.file,
                        str(h.line),
                        h.message.replace("\t", " "),
                    ]
                )
                + "\n"
            )


def write_ffi_tsv(path: Path, exports: list[FfiExport]) -> None:
    with path.open("w", encoding="utf-8") as f:
        f.write(
            "class\tfile\tline\tfunction\treturn_type\traw_pointer_arg\t"
            "returns_raw_pointer\tcatch_unwind\tsignature\n"
        )
        for e in exports:
            f.write(
                "\t".join(
                    [
                        e.boundary_class,
                        e.file,
                        str(e.line),
                        e.name,
                        e.return_type,
                        str(e.has_raw_pointer_arg).lower(),
                        str(e.returns_raw_pointer).lower(),
                        str(e.has_catch_unwind).lower(),
                        e.signature.replace("\t", " "),
                    ]
                )
                + "\n"
            )


def markdown_table(counter: Counter[tuple[str, ...]], headers: tuple[str, ...], limit: int = 25) -> list[str]:
    rows = ["| " + " | ".join(headers) + " |", "|" + "|".join(["---"] * len(headers)) + "|"]
    for key, count in counter.most_common(limit):
        values = key if isinstance(key, tuple) else (key,)
        rows.append("| " + " | ".join([*values, str(count)]) + " |")
    if len(counter) > limit:
        rows.append(f"| ... | {len(counter) - limit} more | |")
    return rows


def write_report(path: Path, hits: list[LintHit], exports: list[FfiExport], static_counts: Counter[str], clippy_status: str) -> None:
    lint_counts: Counter[tuple[str, ...]] = Counter((h.lint,) for h in hits)
    class_counts: Counter[tuple[str, ...]] = Counter((h.lint, classify_path(h.file)) for h in hits)
    module_counts: Counter[tuple[str, ...]] = Counter((h.lint, module_key(h.file)) for h in hits)
    ffi_by_class: Counter[tuple[str, ...]] = Counter(
        (e.boundary_class, "catch_unwind" if e.has_catch_unwind else "missing") for e in exports
    )
    missing = [e for e in exports if not e.has_catch_unwind]

    lines: list[str] = [
        "# Panic-Free Inventory Report",
        "",
        "Issue #8705 measurement output. This report is generated; do not commit target/ copies.",
        "",
        f"- Clippy source: {clippy_status}",
        f"- Clippy lint hits: {len(hits)}",
        f"- FFI extern C exports: {len(exports)}",
        f"- FFI exports without catch_unwind: {len(missing)}",
        "",
        "## Static Panic-Prone Tokens",
        "",
        "| token | count |",
        "|---|---:|",
    ]
    for name, count in sorted(static_counts.items()):
        lines.append(f"| {name} | {count} |")

    lines.extend(["", "## Clippy Counts By Lint", ""])
    lines.extend(markdown_table(lint_counts, ("lint", "count")))
    lines.extend(["", "## Clippy Counts By Class", ""])
    lines.extend(markdown_table(class_counts, ("lint", "class", "count")))
    lines.extend(["", "## Clippy Counts By Module", ""])
    lines.extend(markdown_table(module_counts, ("lint", "module", "count"), limit=40))
    lines.extend(["", "## FFI catch_unwind Inventory", ""])
    lines.extend(markdown_table(ffi_by_class, ("class", "boundary", "count")))
    lines.extend(["", "### Missing catch_unwind Boundaries", ""])
    lines.append("| class | function | file:line | return |")
    lines.append("|---|---|---|---|")
    for e in missing:
        lines.append(f"| {e.boundary_class} | `{e.name}` | `{e.file}:{e.line}` | `{e.return_type}` |")
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", default="target/panic-free-inventory", help="output directory")
    parser.add_argument("--clippy-jsonl", help="parse an existing cargo clippy JSONL log")
    parser.add_argument("--run-clippy", action="store_true", help="run cargo clippy before parsing")
    parser.add_argument("--skip-clippy", action="store_true", help="only run static + FFI inventory")
    args = parser.parse_args()

    if args.run_clippy and args.skip_clippy:
        parser.error("--run-clippy and --skip-clippy are mutually exclusive")

    repo_root = Path.cwd()
    if not (repo_root / "Cargo.toml").exists() or not FFI_ROOT.exists():
        print("ERROR: run from the repository root", file=sys.stderr)
        return 2

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    clippy_path = Path(args.clippy_jsonl) if args.clippy_jsonl else out_dir / "clippy.jsonl"
    clippy_status = "skipped"
    clippy_exit = 0
    if args.run_clippy:
        clippy_exit = run_clippy(clippy_path)
        clippy_status = f"ran cargo clippy (exit {clippy_exit}) -> {clippy_path}"
    elif args.clippy_jsonl:
        clippy_status = f"parsed existing log -> {clippy_path}"
    elif not args.skip_clippy and clippy_path.exists():
        clippy_status = f"parsed existing log -> {clippy_path}"

    hits = [] if args.skip_clippy else parse_clippy_jsonl(clippy_path)
    exports = scan_ffi_exports()
    static_counts = static_panic_counts(rust_files())

    write_lint_tsv(out_dir / "clippy_lints.tsv", hits)
    write_ffi_tsv(out_dir / "ffi_catch_unwind.tsv", exports)
    write_report(out_dir / "report.md", hits, exports, static_counts, clippy_status)

    print(f"wrote {out_dir / 'report.md'}")
    print(f"wrote {out_dir / 'clippy_lints.tsv'}")
    print(f"wrote {out_dir / 'ffi_catch_unwind.tsv'}")
    print(f"clippy_lint_hits={len(hits)}")
    print(f"ffi_exports={len(exports)}")
    print(f"ffi_missing_catch_unwind={sum(1 for e in exports if not e.has_catch_unwind)}")
    return clippy_exit


if __name__ == "__main__":
    raise SystemExit(main())
