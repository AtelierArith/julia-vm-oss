#!/usr/bin/env python3
"""
Split the monolithic manifest.toml into distributed category manifests.

This script:
1. Reads the root manifest.toml
2. Groups tests by category (first directory in file path)
3. Creates manifest.toml in each category directory
4. Updates root manifest.toml to contain only config
"""

import tomllib
import os
from pathlib import Path
from collections import defaultdict

FIXTURES_DIR = Path(__file__).parent.parent / "tests" / "fixtures"
ROOT_MANIFEST = FIXTURES_DIR / "manifest.toml"

def load_manifest():
    with open(ROOT_MANIFEST, "rb") as f:
        return tomllib.load(f)

def group_tests_by_category(tests):
    """Group tests by their category (first directory in file path)."""
    categories = defaultdict(list)
    for test in tests:
        file_path = test["file"]
        category = file_path.split("/")[0]
        categories[category].append(test)
    return categories

def format_expected(value):
    """Format expected value for TOML output."""
    if isinstance(value, bool):
        return "true" if value else "false"
    elif isinstance(value, str):
        return f'"{value}"'
    elif isinstance(value, float):
        # Check if it's a whole number
        if value == int(value):
            return str(int(value)) + ".0"
        return str(value)
    elif isinstance(value, int):
        return str(value)
    return str(value)

def generate_category_manifest(tests, category):
    """Generate manifest.toml content for a category."""
    lines = [f"# Test manifest for {category} category"]
    lines.append("")

    for test in tests:
        lines.append("[[tests]]")
        lines.append(f'name = "{test["name"]}"')

        # Use relative file path (without category prefix)
        file_path = test["file"]
        if file_path.startswith(f"{category}/"):
            file_path = file_path[len(category) + 1:]
        lines.append(f'file = "{file_path}"')

        lines.append(f'expected = {format_expected(test["expected"])}')

        if test.get("description"):
            lines.append(f'description = "{test["description"]}"')

        if test.get("skip"):
            lines.append("skip = true")

        lines.append("")

    return "\n".join(lines)

def generate_root_manifest(config):
    """Generate root manifest.toml with config only."""
    lines = [
        "# Test Manifest for SubsetJuliaVM",
        "# ",
        "# This manifest contains global configuration.",
        "# Test definitions are distributed in each category's manifest.toml.",
        "#",
        "# Test discovery:",
        "# - tests/fixtures/<category>/manifest.toml - category-specific tests",
        "#",
        "# Supported by:",
        "# - cargo test (Rust): fixture_tests.rs",
        "# - julia: scripts/run_julia_tests.jl",
        "#",
        "# All tests must produce identical results in both Julia and SubsetJuliaVM.",
        "",
        "[config]",
        "# Tolerance for floating-point comparison",
        f'epsilon = {config["epsilon"]}',
        "",
        "# Tests are now distributed in each category directory's manifest.toml",
        "# Example: tests/fixtures/arithmetic/manifest.toml",
    ]
    return "\n".join(lines)

def main():
    print("Loading manifest.toml...")
    manifest = load_manifest()
    config = manifest["config"]
    tests = manifest.get("tests", [])

    print(f"Found {len(tests)} tests")

    # Group tests by category
    categories = group_tests_by_category(tests)
    print(f"Found {len(categories)} categories")

    # Create category manifests
    for category, cat_tests in sorted(categories.items()):
        category_dir = FIXTURES_DIR / category
        category_manifest = category_dir / "manifest.toml"

        if not category_dir.exists():
            print(f"  Warning: Category directory {category} does not exist, skipping")
            continue

        content = generate_category_manifest(cat_tests, category)

        print(f"  Writing {category}/manifest.toml ({len(cat_tests)} tests)")
        with open(category_manifest, "w") as f:
            f.write(content)

    # Update root manifest
    print("Updating root manifest.toml...")
    root_content = generate_root_manifest(config)
    with open(ROOT_MANIFEST, "w") as f:
        f.write(root_content)

    print("Done!")
    print(f"\nSplit {len(tests)} tests into {len(categories)} category manifests.")

if __name__ == "__main__":
    main()
