#!/usr/bin/env python3
"""
Convert fixture tests to Test.jl style (v2).

This version correctly handles type definitions by placing them OUTSIDE
the @testset block (at module scope) while keeping test code INSIDE.

Type definitions (abstract type, struct, mutable struct) cannot be placed
inside @testset blocks due to Julia's scope rules.
"""

import tomllib
import os
import re
from pathlib import Path
from typing import Dict, Any, Optional, List, Tuple

FIXTURES_DIR = Path(__file__).parent.parent / "tests" / "fixtures"

# Patterns for type definitions that must stay at module level
TYPE_DEF_PATTERNS = [
    # abstract type ... end
    (r'^(\s*abstract\s+type\s+\w+.*?end\s*)$', True),
    # struct ... end (multiline)
    (r'^(\s*struct\s+\w+.*?end\s*)$', True),
    # mutable struct ... end (multiline)
    (r'^(\s*mutable\s+struct\s+\w+.*?end\s*)$', True),
]

def load_all_tests() -> Dict[str, Dict[str, Any]]:
    """Load all test definitions from distributed manifests."""
    tests = {}

    # Scan category directories
    for entry in FIXTURES_DIR.iterdir():
        if entry.is_dir():
            manifest_path = entry / "manifest.toml"
            if manifest_path.exists():
                category = entry.name
                with open(manifest_path, "rb") as f:
                    manifest = tomllib.load(f)
                for test in manifest.get("tests", []):
                    file_path = test["file"]
                    if "/" not in file_path:
                        file_path = f"{category}/{file_path}"
                    tests[file_path] = test

    return tests

def format_expected_for_test(expected: Any) -> str:
    """Format expected value for @test comparison."""
    if isinstance(expected, bool):
        return "true" if expected else "false"
    elif isinstance(expected, str):
        return f'"{expected}"'
    elif isinstance(expected, float):
        if expected == int(expected):
            return f"{int(expected)}.0"
        return str(expected)
    elif isinstance(expected, int):
        return str(expected)
    return str(expected)

def is_block_definition_start(line: str) -> Optional[str]:
    """Check if line starts a type, function, or module definition. Returns type or None."""
    stripped = line.strip()
    if stripped.startswith("abstract type "):
        return "abstract"
    elif stripped.startswith("struct "):
        return "struct"
    elif stripped.startswith("mutable struct "):
        return "mutable_struct"
    elif stripped.startswith("function "):
        return "function"
    elif stripped.startswith("macro "):
        return "macro"
    elif stripped.startswith("module "):
        return "module"
    elif stripped.startswith("baremodule "):
        return "baremodule"
    return None

def extract_block_definitions(lines: List[str]) -> Tuple[List[str], List[str], List[str]]:
    """
    Extract type and function definitions from code lines.
    Returns (definitions, using_statements, remaining_code).
    These definitions must be placed outside @testset blocks due to scope rules.
    """
    definitions = []
    using_statements = []
    remaining = []

    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        # Extract using/import statements (single line)
        if stripped.startswith("using ") or stripped.startswith("import "):
            using_statements.append(line)
            i += 1
            continue

        block_kind = is_block_definition_start(line)

        if block_kind:
            # Collect the entire definition
            def_lines = [line]
            depth = 0

            # Count the initial depth
            if "end" in line and line.strip().endswith("end"):
                # Single line definition like "abstract type Foo end"
                definitions.append("\n".join(def_lines))
                i += 1
                continue

            # Multiline definition
            depth = 1
            i += 1
            while i < len(lines) and depth > 0:
                def_lines.append(lines[i])
                stripped = lines[i].strip()

                # Check for nested blocks that increase depth
                if is_block_definition_start(lines[i]):
                    depth += 1
                elif any(stripped.startswith(kw) for kw in ["if ", "for ", "while ", "let ", "begin", "try "]):
                    depth += 1
                elif stripped == "begin":
                    depth += 1

                # Check for end that decreases depth
                if stripped == "end":
                    depth -= 1

                i += 1

            definitions.append("\n".join(def_lines))
        else:
            remaining.append(line)
            i += 1

    return definitions, using_statements, remaining

# Files that have complex structures and should not be converted to @testset style
SKIP_FILES = {
    # Macro tests - complex macro definitions
    "macro/identity_macro.jl",
    "macro/add_one_macro.jl",
    "macro/double_macro.jl",
    "macro/gensym_test.jl",
    "macro/gensym_in_macro.jl",
    "macro/hygiene_test.jl",
    "macro/macroexpand_test.jl",
    "macro/nested_macro_test.jl",
    "macro/esc_test.jl",
    "macro/inline_macro.jl",
    "macro/base_inline.jl",
    "macro/multi_statement_quote.jl",
    "macro/local_in_quote.jl",
    "macro/eval_test.jl",
    "macro/juxtaposition_quote.jl",
    "macro/expr_fields.jl",
    "macro/splat_interpolation_basic.jl",
    "macro/splat_interpolation_call.jl",
    "macro/something_macro.jl",
    "macro/assert_pure_julia.jl",
    "macro/show_pure_julia.jl",
    "macro/dispatch_arity.jl",
    "macro/dispatch_type.jl",
    "macro/dispatch_specificity.jl",
    "macro/dispatch_int.jl",
    "macro/dispatch_literals.jl",
    "macro/varargs_macro.jl",
    "macro/not_in_macro.jl",
    "macro/assert_with_message.jl",
    # Metaprogramming tests
    "metaprogramming/runtime_splat.jl",
    "metaprogramming/quote_for_loop.jl",
    # Module tests - baremodule
    "module/baremodule_basic.jl",
    # Function tests with complex dispatch
    "function/operator_method.jl",
    "function/type_specialization.jl",
    # Broadcast tests
    "broadcast/broadcast_function.jl",
    "broadcast/broadcast_shapes.jl",
    "broadcast/broadcast_inplace.jl",
    "broadcast/tuple_binary_add.jl",
    "broadcast/tuple_binary_mul.jl",
    "broadcast/tuple_scalar_sub.jl",
    "broadcast/array_tuple_broadcast.jl",
    "broadcast/tuple_extra_args.jl",
    # Collections tests
    "collections/foldl_foldr.jl",
    "collections/mapreduce.jl",
    "collections/mapfoldl_mapfoldr.jl",
    # KwDef tests
    "kwdef/basic.jl",
    "kwdef/mutable.jl",
    "kwdef/required.jl",
    # Other complex tests
    "global_arrays/const_array_modify.jl",
    "dispatch/array_type_dispatch.jl",
    "array/findall_basic.jl",
    "bigint/string_macro.jl",
    "linalg/inv_basic.jl",
    "operators/compose_operator.jl",
    "operators/pipe.jl",
    "reflection/nameof_type.jl",
    "timing/timing_timed_basic.jl",
    "tuple/ntuple_basic.jl",
    "types/hof_type_inference.jl",
    "types/typeof_reassignment.jl",
    "where/short_function.jl",
}

def convert_file(file_path: Path, test_info: Dict[str, Any]) -> Optional[str]:
    """Convert a single test file to Test.jl style."""
    if not file_path.exists():
        return None

    content = file_path.read_text()
    lines = content.strip().split("\n")

    # Skip already converted files
    if "using Test" in content and "@testset" in content:
        return None

    # Skip files that are already using Test for other reasons
    if "@test " in content and "using Test" in content:
        return None

    expected = test_info["expected"]
    description = test_info.get("description", test_info["name"])
    expected_str = format_expected_for_test(expected)

    # Extract comments at the beginning
    comments = []
    code_lines = []
    in_comments = True

    for line in lines:
        stripped = line.strip()
        if in_comments and stripped.startswith("#"):
            comments.append(line)
        else:
            in_comments = False
            code_lines.append(line)

    # Remove trailing empty lines from code
    while code_lines and not code_lines[-1].strip():
        code_lines.pop()

    if not code_lines:
        return None

    # Extract type and function definitions that must stay at module level
    block_defs, using_stmts, remaining_code = extract_block_definitions(code_lines)

    # Build new content
    new_lines = []

    # Add comments
    new_lines.extend(comments)
    if comments:
        new_lines.append("")

    # Add using Test
    new_lines.append("using Test")

    # Add any other using/import statements extracted from code
    for using_stmt in using_stmts:
        new_lines.append(using_stmt)

    new_lines.append("")

    # Add block definitions BEFORE @testset (at module level)
    if block_defs:
        for block_def in block_defs:
            new_lines.append(block_def)
            new_lines.append("")

    # Add @testset
    new_lines.append(f'@testset "{description}" begin')

    # Remove trailing empty lines from remaining_code
    while remaining_code and not remaining_code[-1].strip():
        remaining_code.pop()

    # Add the remaining code with proper indentation
    if remaining_code:
        # All lines except the last one stay as-is (indented)
        for line in remaining_code[:-1]:
            if line.strip():
                new_lines.append(f"    {line}")
            else:
                new_lines.append("")

        # Convert last line to @test
        last_line = remaining_code[-1].strip()
        # Remove any trailing comment from last line
        if "#" in last_line:
            last_line = last_line.split("#")[0].strip()
        if last_line:
            # Check if we need approximate comparison
            if isinstance(expected, float) and expected != int(expected):
                # Use isapprox for non-integer floats
                new_lines.append(f"    @test isapprox(({last_line}), {expected_str})")
            elif isinstance(expected, bool):
                if expected:
                    new_lines.append(f"    @test ({last_line})")
                else:
                    new_lines.append(f"    @test !({last_line})")
            else:
                # Wrap expression in parentheses to avoid precedence issues with ternary
                new_lines.append(f"    @test ({last_line}) == {expected_str}")

    new_lines.append("end")
    new_lines.append("")
    new_lines.append("true  # Test passed")
    new_lines.append("")

    return "\n".join(new_lines)

def update_manifest(category_dir: Path, converted_files: set):
    """Update manifest.toml to set expected values to true only for converted files."""
    manifest_path = category_dir / "manifest.toml"
    if not manifest_path.exists():
        return

    with open(manifest_path, "rb") as f:
        import tomllib
        manifest = tomllib.load(f)

    content = manifest_path.read_text()
    category = category_dir.name

    # Only update expected values for converted files
    for test in manifest.get("tests", []):
        file_path = test["file"]
        full_path = f"{category}/{file_path}"

        if full_path in converted_files:
            # Find and replace this specific test's expected value
            # This is a simple approach - replace the line after finding the file
            # We need to be careful with regex to only match within the right test block
            pass

    # Simple approach: replace all expected values for now
    # A more sophisticated approach would parse TOML and selectively update
    # For simplicity, we replace all but then we need to not update skipped files
    # Let's use a different approach - update expected only for converted files

    lines = content.split('\n')
    new_lines = []
    current_file = None

    for line in lines:
        if line.strip().startswith('file = '):
            # Extract file name
            match = re.search(r'file = "([^"]+)"', line)
            if match:
                current_file = f"{category}/{match.group(1)}"

        if line.strip().startswith('expected = ') and current_file:
            if current_file in converted_files:
                new_lines.append('expected = true')
            else:
                new_lines.append(line)
            current_file = None
        else:
            new_lines.append(line)

    manifest_path.write_text('\n'.join(new_lines))

def main():
    print("Loading test definitions...")
    tests = load_all_tests()
    print(f"Found {len(tests)} tests")

    converted = 0
    skipped = 0
    errors = 0
    converted_files = set()

    for file_path_str, test_info in tests.items():
        file_path = FIXTURES_DIR / file_path_str

        # Skip files in the skip list
        if file_path_str in SKIP_FILES:
            skipped += 1
            print(f"  Skipped (complex structure): {file_path_str}")
            continue

        try:
            new_content = convert_file(file_path, test_info)
            if new_content:
                file_path.write_text(new_content)
                converted += 1
                converted_files.add(file_path_str)
                print(f"  Converted: {file_path_str}")
            else:
                skipped += 1
        except Exception as e:
            print(f"  Error converting {file_path_str}: {e}")
            errors += 1

    print(f"\nConverted: {converted}, Skipped: {skipped}, Errors: {errors}")

    # Update manifests only for converted files
    print("\nUpdating manifest.toml files...")
    for entry in FIXTURES_DIR.iterdir():
        if entry.is_dir():
            update_manifest(entry, converted_files)

    print("Done!")

if __name__ == "__main__":
    main()
