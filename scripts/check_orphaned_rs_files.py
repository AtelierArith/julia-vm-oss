#!/usr/bin/env python3
"""check_orphaned_rs_files.py

Detect Rust source files under a workspace crate's `src/` tree that are
never reachable from that crate's compiled module tree — i.e. no `mod
<name>;` declaration, `#[path = "..."]` override, or `include!(...)` ever
points at them. Such a file compiles as part of the repo checkout (it is
valid UTF-8 sitting on disk) but is **never fed to rustc**, so edits to it
have zero runtime effect while still polluting `grep`/CodeGraph results
(Issue #10739 — the `subset_julia_vm/src/ir/core.rs` orphan).

Scope and false-positive avoidance (Issue #10739 Scope item 5):

- Only `<crate>/src/**/*.rs` is audited. `tests/`, `benches/`, `examples/`,
  and `build.rs` are separate compilation roots with their own inclusion
  rules and are out of scope.
- Crate roots are `src/lib.rs`, `src/main.rs`, and every `src/bin/*.rs`
  (Cargo auto-discovers `src/bin/*.rs` as binary targets unless `autobins =
  false`; none of this workspace's crates disable it).
- Module resolution follows the Rust 2018+ file-module rules: a root file
  or a `mod.rs` resolves siblings in its own directory; a leaf-style module
  file (`foo.rs`, reached via `mod foo;`) resolves ITS children in a
  same-named subdirectory (`foo/`). `#[path = "..."]` overrides are resolved
  relative to the directory of the file containing the attribute (per the
  Rust reference), regardless of the leaf/mod.rs distinction above.
- `include!(...)` of a literal string path is also treated as a reachability
  edge (conservatively; this script does not attempt to resolve
  `concat!(env!(...), ...)`-style dynamic include paths — those are assumed
  reachable rather than risking a false positive).
- Any `mod <name>;` this script cannot resolve to an existing file is
  reported as a distinct "UNRESOLVED" finding (exit 1) rather than silently
  treated as either reachable or orphaned — an unresolved reference means
  this script's model of the module tree is incomplete for that file, and a
  human should look rather than trust a possibly-wrong orphan/clean verdict.

This is a heuristic, not a reimplementation of rustc's module resolver.
Its bias is intentionally toward under-reporting (fewer files flagged) over
false positives: anything ambiguous is treated as reachable.
"""

from __future__ import annotations  # Python 3.9 evaluates `str | None` otherwise (#11093).

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

MOD_RE = re.compile(
    r"(?:^|\n)[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\s*;",
)
PATH_ATTR_RE = re.compile(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]')
INCLUDE_RE = re.compile(r'include!\s*\(\s*"([^"]+)"\s*\)')

# Characters that terminate a preceding attribute's applicability to a mod
# statement further down the file (i.e. "real code" happened in between).
STOP_CHARS = set(";{}")


def workspace_members():
    text = (REPO_ROOT / "Cargo.toml").read_text()
    m = re.search(r"members\s*=\s*\[(.*?)\]", text, re.DOTALL)
    if not m:
        raise SystemExit("could not find [workspace] members in Cargo.toml")
    return re.findall(r'"([^"]+)"', m.group(1))


def find_path_override(content: str, mod_match: "re.Match") -> str | None:
    """Look backward from a `mod name;` match for an applicable `#[path=...]`.

    Scans backward from the match start; stops at the first "stop" char
    (`;`, `{`, `}`) that isn't part of a `#[path = "..."]` attribute itself,
    since that marks the end of unrelated preceding code. Returns the
    closest (last) `#[path = "..."]` value found before that boundary, if any.
    """
    window_start = max(0, mod_match.start() - 500)
    window = content[window_start : mod_match.start()]

    # Find the rightmost boundary (stop char) that is not inside a #[path=...]
    # attribute; everything after that boundary is "attributes only" territory.
    # Mask out attribute spans before scanning for stop chars, so stop chars
    # *inside* an attribute (e.g. `#[cfg(feature = "x")]`) don't count.
    boundary = -1
    masked = list(window)
    for am in re.finditer(r"#\[[^\]]*\]", window):
        for i in range(am.start(), am.end()):
            masked[i] = " "
    masked_str = "".join(masked)
    for i, ch in enumerate(masked_str):
        if ch in STOP_CHARS:
            boundary = i
    attr_zone = window[boundary + 1 :]
    matches = PATH_ATTR_RE.findall(attr_zone)
    return matches[-1] if matches else None


def resolve_children_dir(file_path: Path, is_root: bool) -> Path:
    if is_root or file_path.name == "mod.rs":
        return file_path.parent
    return file_path.parent / file_path.stem


def audit_crate(crate_dir: Path):
    """Returns (orphans, unresolved) as lists of relative-path strings."""
    src = (crate_dir / "src").resolve()
    if not src.is_dir():
        return [], []

    all_files = {p for p in src.rglob("*.rs") if p.is_file()}

    roots = []
    lib_rs = src / "lib.rs"
    main_rs = src / "main.rs"
    if lib_rs.is_file():
        roots.append(lib_rs)
    if main_rs.is_file():
        roots.append(main_rs)
    bin_dir = src / "bin"
    if bin_dir.is_dir():
        roots.extend(sorted(p for p in bin_dir.glob("*.rs") if p.is_file()))

    reachable = set(roots)
    queue = list(roots)
    unresolved = []

    while queue:
        f = queue.pop()
        is_root = f in roots
        children_dir = resolve_children_dir(f, is_root)
        content = f.read_text(errors="replace")

        for m in MOD_RE.finditer(content):
            name = m.group(1)
            override = find_path_override(content, m)
            if override is not None:
                candidate = (f.parent / override).resolve()
                candidates = [candidate]
            else:
                candidates = [
                    (children_dir / f"{name}.rs").resolve(),
                    (children_dir / name / "mod.rs").resolve(),
                ]
            resolved = next((c for c in candidates if c.is_file()), None)
            if resolved is None:
                unresolved.append(
                    f"{f.relative_to(REPO_ROOT)}: `mod {name};` "
                    f"(candidates: {[str(c) for c in candidates]})"
                )
                continue
            if resolved not in reachable:
                reachable.add(resolved)
                queue.append(resolved)

        for m in INCLUDE_RE.finditer(content):
            target = (f.parent / m.group(1)).resolve()
            if target.is_file():
                reachable.add(target)
                if target.suffix == ".rs" and target not in queue:
                    queue.append(target)

    orphans = sorted(
        str(p.relative_to(REPO_ROOT)) for p in (all_files - reachable)
    )
    return orphans, unresolved


def main() -> int:
    members = workspace_members()
    all_orphans = []
    all_unresolved = []
    for member in members:
        crate_dir = REPO_ROOT / member
        orphans, unresolved = audit_crate(crate_dir)
        all_orphans.extend(orphans)
        all_unresolved.extend(unresolved)

    if all_unresolved:
        print("UNRESOLVED mod declarations (script's module model is incomplete"
              " for these — investigate before trusting orphan results):")
        for u in all_unresolved:
            print(f"  - {u}")
        print()

    if all_orphans:
        print("Orphaned .rs files found (present under a crate's src/ tree but"
              " unreachable from any mod/#[path]/include! — never compiled):")
        for o in all_orphans:
            print(f"  - {o}")
        print()
        print(
            "If this file is genuinely dead, delete it (see Issue #10739 for "
            "the precedent). If it should be live, add the missing `mod "
            "<name>;` (or `#[path]`) declaration."
        )

    if all_orphans or all_unresolved:
        return 1

    print(f"OK: no orphaned .rs files found under {len(members)} crate src/ trees.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
