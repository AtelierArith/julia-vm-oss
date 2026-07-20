#!/usr/bin/env bash
# Keep the code BODY of every code sample identical across delivery surfaces
# (Issue #9278). A sample ships from up to four places and they drift silently:
#
#   iOS resource (canonical, loaded at runtime)
#     SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/<folder>/<id>.jl
#   iOS Swift fallback (used only if the resource is missing)
#     SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSamples+<Difficulty>.swift  (code: #"""…"""#)
#   Flutter / mobile
#     mobile/assets/samples/<folder>/<id>.jl
#   Web
#     web/samples_ir.js
#
# The Mandelbrot heatmap shipped as vertical stripes on iOS (Issue #9277)
# because only the iOS .jl resource still had the buggy `reshape` version while
# the other surfaces had already moved to the correct broadcast. Nothing caught
# it: scripts/check_ios_sample_catalog.sh validates METADATA only (ids, names,
# folders, counts) — never the code bodies.
#
# Source of truth: the iOS .jl RESOURCE is canonical. The Swift `code:` fallback
# and the mobile .jl must match it byte-for-byte after normalization.
#
# Behaviour:
#   - FAIL (exit 1) on any body mismatch between a present surface and the
#     canonical iOS .jl, on a missing canonical resource, on an orphan mobile
#     .jl with no catalog entry, or on a malformed allowlist row.
#   - WARN (exit 0) when a surface is simply missing a sample (e.g. an iOS-only
#     package demo not shipped on mobile). Record intentional omissions in
#     docs/vm/SAMPLE_BODY_MISSING_ALLOWLIST.tsv (id / surface / issue / reason)
#     to silence the warning with a linked Issue.
#
# Web (web/samples_ir.js) stores lowered IR, not raw .jl bodies, so it is OUT OF
# SCOPE for this body-diff gate and is intentionally not compared here (deferred;
# tracked alongside the follow-up Issue in the allowlist header).
#
# bash 3.2 compatible: all logic runs in a single python3 heredoc, exactly like
# scripts/check_ios_sample_catalog.sh (Issue #3771 bans mapfile/readarray in the
# bash layer; there is no array logic in the bash layer here).

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$ROOT_DIR" <<'PY'
import difflib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
ios_sample_dir = root / "SubsetJuliaVMApp" / "SubsetJuliaVMApp" / "Resources" / "Samples"
ios_json = ios_sample_dir / "samples.json"
model_dir = root / "SubsetJuliaVMApp" / "SubsetJuliaVMApp" / "Models"
mobile_dir = root / "mobile" / "assets" / "samples"
allowlist_path = root / "docs" / "vm" / "SAMPLE_BODY_MISSING_ALLOWLIST.tsv"

errors = []
warnings = []


def normalize(text):
    """Normalize a sample body for cross-surface comparison.

    - strip trailing whitespace from every line
    - drop trailing blank lines / the final newline

    Leading and interior blank lines are preserved (they are meaningful layout).
    """
    lines = [ln.rstrip() for ln in text.split("\n")]
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines)


def parse_swift_bodies(path):
    # Extract {name: normalized_code} for every CodeSample(...) in a Swift file.
    #
    # Two `code:` shapes appear. The multiline raw string opens with
    # `code: #` followed by three double-quotes; its content is indented to
    # match the closing delimiter and that indent is stripped. The inline
    # single-line string is `code: "..."` (e.g. the empty memo scratchpad).
    import re

    src_lines = path.read_text(encoding="utf-8").split("\n")
    bodies = {}
    current_name = None
    i = 0
    n = len(src_lines)
    while i < n:
        line = src_lines[i]
        m_name = re.search(r'name:\s*"((?:[^"\\]|\\.)*)"', line)
        if m_name:
            current_name = m_name.group(1)
        m_code = re.match(r'\s*code:\s*(.*)$', line)
        if m_code and current_name is not None:
            rest = m_code.group(1)
            if rest.startswith('#"""'):
                body_lines = []
                dedent = 0
                i += 1
                while i < n:
                    cl = src_lines[i]
                    if cl.lstrip(' ').startswith('"""#'):
                        dedent = len(cl) - len(cl.lstrip(' '))
                        break
                    body_lines.append(cl)
                    i += 1
                dedented = []
                for bl in body_lines:
                    if bl[:dedent] == ' ' * dedent:
                        dedented.append(bl[dedent:])
                    else:
                        dedented.append(bl.lstrip(' '))
                bodies[current_name] = normalize("\n".join(dedented))
                current_name = None
            else:
                m_inline = re.match(r'"((?:[^"\\]|\\.)*)"', rest)
                if m_inline:
                    raw = m_inline.group(1)
                    # Minimal Swift unescape; the only inline sample is the
                    # empty memo scratchpad, so keep this deliberately small.
                    val = (
                        raw.replace('\\\\', '\x00')
                        .replace('\\"', '"')
                        .replace('\\n', '\n')
                        .replace('\\t', '\t')
                        .replace('\x00', '\\')
                    )
                    bodies[current_name] = normalize(val)
                current_name = None
        i += 1
    return bodies


def short_diff(canonical, other, canonical_label, other_label):
    diff = list(
        difflib.unified_diff(
            canonical.split("\n"),
            other.split("\n"),
            fromfile=canonical_label,
            tofile=other_label,
            lineterm="",
        )
    )
    capped = diff[:24]
    if len(diff) > len(capped):
        capped.append(f"    ... ({len(diff) - len(capped)} more diff lines)")
    return "\n".join("      " + ln for ln in capped)


# ---- load allowlist: {(id, surface): issue} ----
allow = {}
if allowlist_path.is_file():
    for lineno, raw in enumerate(
        allowlist_path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        cols = raw.split("\t")
        if cols[0] == "id":  # header row
            continue
        if len(cols) < 4 or not cols[0] or not cols[1] or not cols[2] or not cols[3].strip():
            errors.append(
                f"{allowlist_path.relative_to(root)}:{lineno}: malformed row "
                f"(need id<TAB>surface<TAB>issue<TAB>reason): {raw!r}"
            )
            continue
        sid, surface, issue = cols[0], cols[1], cols[2]
        if surface not in ("mobile", "swift"):
            errors.append(
                f"{allowlist_path.relative_to(root)}:{lineno}: unknown surface "
                f"{surface!r} (expected 'mobile' or 'swift')"
            )
            continue
        if not issue.lstrip().startswith("#"):
            errors.append(
                f"{allowlist_path.relative_to(root)}:{lineno}: issue column "
                f"{issue!r} must be a '#NNNN' Issue reference"
            )
            continue
        allow[(sid, surface)] = issue

# ---- load canonical catalog ----
try:
    samples = json.loads(ios_json.read_text(encoding="utf-8"))
except Exception as exc:  # noqa: BLE001 - surface any JSON error verbatim
    raise SystemExit(f"ERROR: failed to parse {ios_json}: {exc}")

swift_bodies = {}
for p in sorted(model_dir.glob("CodeSamples+*.swift")):
    swift_bodies.update(parse_swift_bodies(p))

catalog_ids = set()
mobile_present = set()
mobile_compared = 0
swift_compared = 0
allowlisted_used = set()

for sample in samples:
    sid = sample["id"]
    folder = sample["folder"]
    name = sample["name"]
    catalog_ids.add(sid)

    ios_path = ios_sample_dir / folder / f"{sid}.jl"
    if not ios_path.is_file():
        errors.append(f"{sid}: canonical iOS resource missing: {ios_path.relative_to(root)}")
        continue
    ios_body = normalize(ios_path.read_text(encoding="utf-8"))

    # --- Swift fallback (code: block, keyed by name) ---
    if name in swift_bodies:
        swift_body = swift_bodies[name]
        swift_compared += 1
        if swift_body != ios_body:
            errors.append(
                f"{sid}: Swift fallback body (name={name!r}) differs from canonical iOS "
                f".jl ({ios_path.relative_to(root)}).\n"
                + short_diff(ios_body, swift_body, "ios.jl", "swift-fallback")
            )
    elif (sid, "swift") in allow:
        allowlisted_used.add((sid, "swift"))
    else:
        warnings.append(
            f"{sid}: no Swift fallback `code:` block found for name {name!r}"
        )

    # --- Mobile .jl ---
    mobile_path = mobile_dir / folder / f"{sid}.jl"
    if mobile_path.is_file():
        mobile_present.add(sid)
        mobile_compared += 1
        mobile_body = normalize(mobile_path.read_text(encoding="utf-8"))
        if mobile_body != ios_body:
            errors.append(
                f"{sid}: mobile body differs from canonical iOS .jl.\n"
                f"    canonical: {ios_path.relative_to(root)}\n"
                f"    mobile:    {mobile_path.relative_to(root)}\n"
                + short_diff(ios_body, mobile_body, "ios.jl", "mobile.jl")
            )
        if (sid, "mobile") in allow:
            warnings.append(
                f"{sid}: allowlisted as missing on mobile, but "
                f"{mobile_path.relative_to(root)} exists — remove the stale "
                f"allowlist row."
            )
    elif (sid, "mobile") in allow:
        allowlisted_used.add((sid, "mobile"))
    else:
        warnings.append(
            f"{sid}: missing from mobile ({mobile_path.relative_to(root)}). "
            f"Add the sample to mobile, or allowlist (id<TAB>mobile<TAB>#Issue) "
            f"in {allowlist_path.relative_to(root)}."
        )

# ---- orphan mobile files with no catalog entry ----
for path in sorted(mobile_dir.glob("*/*.jl")):
    if path.stem not in catalog_ids:
        errors.append(
            f"orphan mobile sample with no iOS catalog entry: {path.relative_to(root)}"
        )

# ---- stale allowlist rows (id no longer in catalog) ----
for (sid, surface), issue in sorted(allow.items()):
    if sid not in catalog_ids:
        warnings.append(
            f"stale allowlist row: {sid!r}/{surface} ({issue}) — id is not in "
            f"{ios_json.relative_to(root)}."
        )

if warnings:
    print("WARN: sample-body consistency warnings:", file=sys.stderr)
    for w in warnings:
        print(f"  - {w}", file=sys.stderr)

if errors:
    print("ERROR: sample body drift detected across delivery surfaces (Issue #9278):", file=sys.stderr)
    for e in errors:
        print(f"  - {e}", file=sys.stderr)
    print(
        "\nThe iOS .jl resource is canonical; make the Swift `code:` fallback and "
        "the mobile .jl match it, or record an intentional per-surface omission in "
        f"{allowlist_path.relative_to(root)}.",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    f"OK: {len(catalog_ids)} samples — {swift_compared} Swift fallbacks and "
    f"{mobile_compared} mobile bodies match the canonical iOS .jl "
    f"({len(allowlisted_used)} allowlisted omissions) (Issue #9278)"
)
PY
