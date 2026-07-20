#!/usr/bin/env python3
"""Audit duplicate same-signature definitions in bundled Julia Base.

The goal is not to implement a complete Julia parser. It is a conservative
definition scanner for the subset of method-definition syntax used under
subset_julia_vm/src/julia/base:

  * long form:  function f(args...) ... end
  * short form: f(args...) = ...

It strips comments, handles multi-line signatures by delimiter balance, keeps
`where` bounds in the normalized signature, and only treats a top-level single
`=` as short-form assignment so comparisons such as `x == y` are not methods.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
BASE_ROOT = REPO_ROOT / "subset_julia_vm/src/julia/base"
ALLOWLIST = REPO_ROOT / "docs/vm/BASE_DUPLICATE_SIGNATURE_ALLOWLIST.tsv"

KEYWORDS = {
    "if",
    "elseif",
    "while",
    "for",
    "return",
    "try",
    "catch",
    "finally",
    "let",
    "begin",
    "quote",
    "macro",
    "struct",
    "mutable",
    "const",
    "global",
    "local",
}


@dataclass(frozen=True)
class Definition:
    signature: str
    file: str
    line: int
    header: str

    @property
    def location(self) -> str:
        return f"{self.file}:{self.line}"


def strip_comment(line: str) -> str:
    out: list[str] = []
    in_string = False
    escaped = False
    for char in line:
        if in_string:
            out.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == "#":
            break
        out.append(char)
        if char == '"':
            in_string = True
    return "".join(out)


def delimiter_delta(text: str) -> int:
    depth = 0
    in_string = False
    escaped = False
    for char in text:
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
    return depth


def collect_header(lines: list[str], index: int, start: str) -> tuple[str, int]:
    header = start.strip()
    depth = delimiter_delta(header)
    cursor = index
    while depth > 0 and cursor + 1 < len(lines):
        cursor += 1
        part = strip_comment(lines[cursor]).strip()
        header += " " + part
        depth += delimiter_delta(part)
    return re.sub(r"\s+", " ", header).strip(), cursor


def find_arglist(signature_text: str) -> tuple[int, int] | None:
    candidates: list[int] = []
    depth = 0
    in_string = False
    escaped = False
    for index, char in enumerate(signature_text):
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            continue
        if char == "(":
            if depth == 0:
                prefix = signature_text[:index].strip()
                if (
                    prefix
                    and not re.search(r"\s", prefix)
                    and prefix.split(".")[0] not in KEYWORDS
                ):
                    candidates.append(index)
            depth += 1
        elif char == ")":
            depth -= 1

    for start in reversed(candidates):
        depth = 0
        in_string = False
        escaped = False
        for index in range(start, len(signature_text)):
            char = signature_text[index]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
                continue
            if char == '"':
                in_string = True
                continue
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    return start, index
    return None


def has_short_form_assignment(after_arglist: str) -> bool:
    text = after_arglist.strip()
    depth = 0
    in_string = False
    escaped = False
    for index, char in enumerate(text):
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            continue
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "=" and depth == 0:
            prev_char = text[index - 1] if index > 0 else ""
            next_char = text[index + 1] if index + 1 < len(text) else ""
            return prev_char not in "!<>=" and next_char not in "=>="
    return False


def split_params(params: str) -> list[str]:
    result: list[str] = []
    current: list[str] = []
    depth = 0
    in_string = False
    escaped = False
    for char in params:
        if in_string:
            current.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
            current.append(char)
        elif char in "([{":
            depth += 1
            current.append(char)
        elif char in ")]}":
            depth -= 1
            current.append(char)
        elif char == "," and depth == 0:
            param = "".join(current).strip()
            if param:
                result.append(param)
            current = []
        else:
            current.append(char)
    param = "".join(current).strip()
    if param:
        result.append(param)
    return result


def strip_top_level_default(param: str) -> str:
    depth = 0
    for index, char in enumerate(param):
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "=" and depth == 0:
            next_char = param[index + 1] if index + 1 < len(param) else ""
            if next_char != "=":
                return param[:index].strip()
    return param


def normalize_param(param: str) -> str | None:
    param = re.sub(r"\s+", " ", param.strip()).split(";", 1)[0].strip()
    if not param:
        return None
    param = strip_top_level_default(param)
    if param.startswith("::"):
        return param
    typed = re.match(r"(?:[A-Za-z_][\w!]*|\.\.\.)\s*::\s*(.+)$", param)
    if typed:
        return "::" + typed.group(1).strip()
    untyped = re.match(r"([A-Za-z_][\w!]*)(\.\.\.)?$", param)
    if untyped:
        return "...Any" if untyped.group(2) else "Any"
    return param


def normalize_signature(signature_text: str, require_assignment: bool) -> str | None:
    arglist = find_arglist(signature_text)
    if arglist is None:
        return None
    start, end = arglist
    name = signature_text[:start].strip()
    after = signature_text[end + 1 :].strip()
    if name in KEYWORDS or name.startswith("@") or re.search(r"\s", name):
        return None
    if require_assignment and not has_short_form_assignment(after):
        return None

    params = [
        normalized
        for normalized in (normalize_param(p) for p in split_params(signature_text[start + 1 : end]))
        if normalized
    ]
    where_suffix = ""
    where_match = re.search(r"\bwhere\b\s*(.*?)(?:\s*=|\s*$)", after)
    if where_match:
        where_suffix = " where " + re.sub(r"\s+", " ", where_match.group(1).strip())
    return f"{name}({', '.join(params)}){where_suffix}"


def collect_definitions() -> list[Definition]:
    definitions: list[Definition] = []
    for path in sorted(BASE_ROOT.rglob("*.jl")):
        rel_path = path.relative_to(BASE_ROOT).as_posix()
        lines = path.read_text(encoding="utf-8").splitlines()
        index = 0
        while index < len(lines):
            stripped = strip_comment(lines[index]).strip()
            if not stripped:
                index += 1
                continue
            if stripped.startswith("function "):
                header, end_index = collect_header(lines, index, stripped[len("function ") :])
                signature = normalize_signature(header, require_assignment=False)
                if signature:
                    definitions.append(Definition(signature, rel_path, index + 1, header))
                index = end_index + 1
                continue
            if "(" in stripped:
                header, end_index = collect_header(lines, index, stripped)
                signature = normalize_signature(header, require_assignment=True)
                if signature:
                    definitions.append(Definition(signature, rel_path, index + 1, header))
                index = end_index + 1
                continue
            index += 1
    return definitions


def duplicate_groups(definitions: list[Definition]) -> dict[str, list[Definition]]:
    grouped: dict[str, list[Definition]] = {}
    for definition in definitions:
        grouped.setdefault(definition.signature, []).append(definition)
    return {sig: defs for sig, defs in grouped.items() if len(defs) > 1}


def location_key(definitions: list[Definition]) -> str:
    return ";".join(definition.location for definition in sorted(definitions, key=lambda d: (d.file, d.line)))


def load_allowlist() -> dict[tuple[str, str], tuple[str, str]]:
    if not ALLOWLIST.is_file():
        raise SystemExit(f"ERROR: allowlist not found: {ALLOWLIST.relative_to(REPO_ROOT)}")
    rows: dict[tuple[str, str], tuple[str, str]] = {}
    lines = ALLOWLIST.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "signature\tlocations\tissue\treason":
        raise SystemExit("ERROR: BASE_DUPLICATE_SIGNATURE_ALLOWLIST.tsv has an invalid header")
    for line_number, line in enumerate(lines[1:], start=2):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 4:
            raise SystemExit(
                f"ERROR: malformed allowlist row {line_number}: expected 4 tab-separated fields"
            )
        signature, locations, issue, reason = parts
        if not signature or not locations or not issue or not reason:
            raise SystemExit(f"ERROR: malformed allowlist row {line_number}: empty field")
        key = (signature, locations)
        if key in rows:
            raise SystemExit(f"ERROR: duplicate allowlist row {line_number}: {signature} {locations}")
        rows[key] = (issue, reason)
    return rows


def main() -> int:
    current_groups = duplicate_groups(collect_definitions())
    current = {
        (signature, location_key(definitions)): definitions
        for signature, definitions in current_groups.items()
    }
    allowlist = load_allowlist()

    ok = True
    unclassified = sorted(set(current) - set(allowlist))
    stale = sorted(set(allowlist) - set(current))

    if unclassified:
        ok = False
        print("ERROR: unclassified duplicate same-signature Base definitions (Issue #10185):")
        for signature, locations in unclassified:
            print(f"  signature: {signature}")
            for definition in current[(signature, locations)]:
                print(f"    {definition.location}: {definition.header}")
            print("    add a justified row to docs/vm/BASE_DUPLICATE_SIGNATURE_ALLOWLIST.tsv")

    if stale:
        ok = False
        print("ERROR: stale duplicate-signature allowlist rows (Issue #10185):")
        for signature, locations in stale:
            print(f"  {signature}\t{locations}")
            print("    remove or update this row after verifying the duplicate was retired/moved")

    if not ok:
        return 1

    print(
        "OK: bundled Base duplicate same-signature definitions are classified "
        f"({len(current)} groups, Issue #10185)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
