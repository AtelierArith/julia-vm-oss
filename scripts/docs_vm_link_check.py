#!/usr/bin/env python3
"""Check docs/vm markdown links.

The checker is intentionally repo-native instead of depending on a networked
link service. It validates internal relative links and same-file/other-file
heading anchors, and it verifies that external URLs are covered by an explicit
host allowlist.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse


LINK_RE = re.compile(r"!?\[[^\]\n]*\]\(([^)\n]+)\)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
SCHEME_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")


def strip_code_fences(text: str) -> str:
    out: list[str] = []
    in_fence = False
    for line in text.splitlines(keepends=True):
        if line.startswith("```") or line.startswith("~~~"):
            in_fence = not in_fence
            out.append("\n")
            continue
        out.append("\n" if in_fence else line)
    return re.sub(r"`[^`\n]*`", "", "".join(out))


def github_anchor(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = text.strip().lower()
    text = re.sub(r"[^\w\s\-\u0080-\uffff]", "", text, flags=re.UNICODE)
    text = re.sub(r"\s+", "-", text)
    return text.strip("-")


def heading_anchors(path: Path) -> set[str]:
    anchors: set[str] = set()
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return anchors
    for line in text.splitlines():
        match = HEADING_RE.match(line)
        if match:
            anchors.add(github_anchor(match.group(2)))
    return anchors


def load_allowlist(path: Path | None) -> set[str]:
    if path is None:
        return set()
    allowed: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        item = line.strip()
        if not item or item.startswith("#"):
            continue
        allowed.add(item.lower())
    return allowed


def host_allowed(host: str, allowed: set[str]) -> bool:
    host = host.lower()
    return host in allowed or any(rule.startswith("*.") and host.endswith(rule[1:]) for rule in allowed)


def extract_target(raw: str) -> str:
    raw = raw.strip()
    if raw.startswith("<") and ">" in raw:
        return raw[1 : raw.index(">")]
    if " " in raw:
        raw = raw.split(" ", 1)[0]
    return raw


def check_file(path: Path, docs_root: Path, allow_external: set[str], repo: Path) -> list[str]:
    failures: list[str] = []
    text = strip_code_fences(path.read_text(encoding="utf-8"))
    anchors_cache: dict[Path, set[str]] = {}

    for line_no, line in enumerate(text.splitlines(), start=1):
        for match in LINK_RE.finditer(line):
            target = extract_target(match.group(1))
            if not target:
                continue

            parsed = urlparse(target)
            if parsed.scheme in {"http", "https"}:
                if not host_allowed(parsed.netloc, allow_external):
                    failures.append(
                        f"{path.relative_to(repo)}:{line_no}: external URL host not allowlisted: {target}"
                    )
                continue
            if parsed.scheme in {"mailto", "tel"}:
                continue
            if SCHEME_RE.match(target):
                continue

            path_part, _, fragment = target.partition("#")
            if not path_part:
                target_path = path
            else:
                decoded = unquote(path_part)
                target_path = (path.parent / decoded).resolve()

            try:
                target_path.relative_to(docs_root.parent.resolve())
            except ValueError:
                # Links out of docs/vm are still repository-internal; validate
                # only existence and skip anchor parsing for non-doc targets.
                pass

            if not target_path.exists():
                failures.append(f"{path}:{line_no}: missing link target: {target}")
                continue
            if target_path.is_dir():
                continue

            if fragment and target_path.suffix.lower() == ".md":
                anchor = unquote(fragment).lower()
                anchors = anchors_cache.setdefault(target_path, heading_anchors(target_path))
                if anchor not in anchors:
                    failures.append(
                        f"{path.relative_to(repo)}:{line_no}: missing heading anchor '#{fragment}' in {target_path.relative_to(repo)}"
                    )

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docs-root", default="docs/vm")
    parser.add_argument(
        "--include-archives",
        action="store_true",
        help="also check docs/vm/archive and docs/vm/archived historical records",
    )
    parser.add_argument(
        "--external-allowlist",
        default="docs/vm/EXTERNAL_LINK_ALLOWLIST.txt",
        help="newline-separated external host allowlist",
    )
    args = parser.parse_args()

    repo = Path.cwd()
    docs_root = (repo / args.docs_root).resolve()
    allowlist_path = repo / args.external_allowlist
    allow_external = load_allowlist(allowlist_path if allowlist_path.exists() else None)

    failures: list[str] = []
    for path in sorted(docs_root.rglob("*.md")):
        relative_parts = path.relative_to(docs_root).parts
        if not args.include_archives and relative_parts and relative_parts[0] in {"archive", "archived"}:
            continue
        failures.extend(check_file(path.resolve(), docs_root, allow_external, repo.resolve()))

    if failures:
        print("ERROR: docs/vm markdown link check failed:")
        for failure in failures:
            print(f"  {failure}")
        return 1

    print("OK: docs/vm markdown links and allowlisted external hosts are valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
