#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$ROOT_DIR" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
sample_dir = root / "SubsetJuliaVMApp" / "SubsetJuliaVMApp" / "Resources" / "Samples"
sample_json = sample_dir / "samples.json"
model_dir = root / "SubsetJuliaVMApp" / "SubsetJuliaVMApp" / "Models"
model_path = root / "SubsetJuliaVMApp" / "SubsetJuliaVMApp" / "Models" / "CodeSample.swift"
readme_path = root / "SubsetJuliaVMApp" / "README.md"

errors = []

def enum_values(source, enum_name):
    match = re.search(rf"enum\s+{enum_name}[^{{]*\{{(?P<body>.*?)\n\s*\}}", source, re.S)
    if not match:
        errors.append(f"Could not find CodeSample.{enum_name} in {model_path}")
        return set()
    return set(re.findall(r'case\s+\w+\s*=\s*"([^"]+)"', match.group("body")))

model_source = model_path.read_text(encoding="utf-8")
categories = enum_values(model_source, "Category")
difficulties = enum_values(model_source, "Difficulty")

try:
    samples = json.loads(sample_json.read_text(encoding="utf-8"))
except Exception as exc:
    raise SystemExit(f"ERROR: failed to parse {sample_json}: {exc}")

if not isinstance(samples, list):
    errors.append("samples.json must contain a top-level array")
    samples = []

required_fields = {"id", "name", "category", "description", "difficulty", "tags", "folder"}
seen_ids = set()
listed_files = set()

for index, sample in enumerate(samples):
    if not isinstance(sample, dict):
        errors.append(f"Entry {index} is not an object")
        continue

    missing = sorted(required_fields - set(sample))
    if missing:
        errors.append(f"Entry {index} is missing fields: {', '.join(missing)}")
        continue

    sample_id = sample["id"]
    folder = sample["folder"]
    category = sample["category"]
    difficulty = sample["difficulty"]

    if not isinstance(sample_id, str) or not re.fullmatch(r"[a-z0-9_]+", sample_id):
        errors.append(f"Entry {index} has invalid id {sample_id!r}; use lowercase snake_case")
    elif sample_id in seen_ids:
        errors.append(f"Duplicate sample id: {sample_id}")
    else:
        seen_ids.add(sample_id)

    if category not in categories:
        errors.append(
            f"{sample_id}: category {category!r} is not in CodeSample.Category raw values {sorted(categories)}"
        )

    if difficulty not in difficulties:
        errors.append(
            f"{sample_id}: difficulty {difficulty!r} is not in CodeSample.Difficulty raw values {sorted(difficulties)}"
        )

    expected_folder = difficulty.lower() if isinstance(difficulty, str) else None
    if folder != expected_folder:
        errors.append(f"{sample_id}: folder {folder!r} must match lowercase difficulty {expected_folder!r}")

    if not isinstance(sample.get("tags"), list) or not all(isinstance(tag, str) for tag in sample["tags"]):
        errors.append(f"{sample_id}: tags must be an array of strings")

    if not isinstance(sample.get("name"), str) or not sample["name"].strip():
        errors.append(f"{sample_id}: name must be a non-empty string")

    if not isinstance(sample.get("description"), str) or not sample["description"].strip():
        errors.append(f"{sample_id}: description must be a non-empty string")

    sample_file = sample_dir / folder / f"{sample_id}.jl"
    listed_files.add(sample_file.resolve())
    if not sample_file.is_file():
        errors.append(f"{sample_id}: missing sample source file {sample_file.relative_to(root)}")
    elif sample_id != "memo" and not sample_file.read_text(encoding="utf-8").strip():
        errors.append(f"{sample_id}: sample source file must not be empty")

actual_files = {path.resolve() for path in sample_dir.glob("*/*.jl")}
extra_files = sorted(actual_files - listed_files)
missing_entries = sorted(listed_files - actual_files)

for path in extra_files:
    errors.append(f"Unlisted sample source file: {path.relative_to(root)}")
for path in missing_entries:
    errors.append(f"Listed sample file does not exist: {path.relative_to(root)}")

sample_count = len(samples)
category_count = len({sample.get("category") for sample in samples if isinstance(sample, dict)})
json_names = {sample.get("name") for sample in samples if isinstance(sample, dict)}

embedded_names = set()
for path in sorted(model_dir.glob("CodeSamples+*.swift")):
    embedded_names.update(re.findall(r'name:\s*"([^"]+)"', path.read_text(encoding="utf-8")))

extra_embedded = sorted(embedded_names - json_names)
missing_embedded = sorted(json_names - embedded_names)
for name in extra_embedded:
    errors.append(f"Embedded fallback sample {name!r} is not listed in samples.json")
for name in missing_embedded:
    errors.append(f"samples.json sample {name!r} is missing from embedded fallback samples")

readme = readme_path.read_text(encoding="utf-8")
sample_count_mentions = [
    int(match.group(1))
    for match in re.finditer(
        r"\b(\d+)\s+(?:sample Julia programs|sample programs|sample codes|Sample Programs)",
        readme,
    )
]
for count in sample_count_mentions:
    if count != sample_count:
        errors.append(
            f"README sample count mentions {count}, but samples.json contains {sample_count} entries"
        )

category_count_mentions = [
    int(match.group(1))
    for match in re.finditer(r"\b(\d+)\s+categories", readme)
]
for count in category_count_mentions:
    if count != category_count:
        errors.append(
            f"README category count mentions {count}, but samples.json uses {category_count} categories"
        )

if errors:
    print("ERROR: iOS sample catalog validation failed:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    sys.exit(1)

print(
    f"OK: {sample_count} iOS samples, {category_count} categories, "
    f"{len(difficulties)} difficulties match CodeSample.swift (Issue #8457)"
)
PY
