#!/usr/bin/env bash
# Keep external Python helpers launched by source audits compatible with the
# repository's ambient Python 3.9 floor (Issue #11102).

set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import ast
import pathlib
import re
import shlex
import subprocess
import sys
import tempfile


FLOOR = (3, 9)
AUDIT_GLOBS = ("check_*.sh", "audit_*.sh")

# Imports currently verified against Python 3.9. New imports fail closed until
# their floor availability is reviewed and recorded here.
PY39_VERIFIED_IMPORTS = {
    "argparse",
    "csv",
    "fnmatch",
    "hashlib",
    "json",
    "re",
    "subprocess",
    "sys",
}
PY39_VERIFIED_FROM_IMPORTS = {
    ("__future__", "annotations"),
    ("collections", "Counter"),
    ("collections", "defaultdict"),
    ("dataclasses", "dataclass"),
    ("pathlib", "Path"),
    ("typing", "Iterable"),
    ("typing", "NamedTuple"),
    ("typing", "Optional"),
    ("typing", "Sequence"),
    ("typing", "Tuple"),
}
PY39_VERIFIED_MODULE_ATTRIBUTES = {
    ("argparse", "ArgumentParser"),
    ("argparse", "Namespace"),
    ("csv", "DictReader"),
    ("csv", "DictWriter"),
    ("hashlib", "sha256"),
    ("json", "JSONDecodeError"),
    ("json", "loads"),
    ("fnmatch", "fnmatchcase"),
    ("re", "DOTALL"),
    ("re", "compile"),
    ("re", "findall"),
    ("re", "finditer"),
    ("re", "match"),
    ("re", "search"),
    ("re", "sub"),
    ("subprocess", "STDOUT"),
    ("subprocess", "run"),
    ("sys", "exit"),
    ("sys", "argv"),
    ("sys", "stderr"),
}


def version_text(version):
    return ".".join(str(part) for part in version)


def strip_shell_comment(line):
    result = []
    single = False
    double = False
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            result.append(char)
            escaped = False
            continue
        if char == "\\" and not single:
            result.append(char)
            escaped = True
            continue
        if char == "'" and not double:
            single = not single
            result.append(char)
            continue
        if char == '"' and not single:
            double = not double
            result.append(char)
            continue
        if (
            char == "#"
            and not single
            and not double
            and (index == 0 or line[index - 1].isspace())
        ):
            break
        result.append(char)
    return "".join(result)


def executable_shell_text(source):
    """Remove heredoc bodies/comments and join shell continuations."""
    kept = []
    heredoc_delimiters = []
    heredoc_re = re.compile(r"<<-?\s*(?:'([^']+)'|\"([^\"]+)\"|([^\s;|&]+))")
    for raw_line in source.splitlines():
        if heredoc_delimiters:
            if raw_line.strip() == heredoc_delimiters[0]:
                heredoc_delimiters.pop(0)
            continue
        line = strip_shell_comment(raw_line)
        kept.append(line)
        heredoc_delimiters.extend(
            next(group for group in match.groups() if group is not None)
            for match in heredoc_re.finditer(line)
        )
    return "\n".join(kept).replace("\\\n", " ")


def helpers_from_wrapper(wrapper, source):
    helpers = set()
    failures = []
    shell_text = executable_shell_text(source)
    command_re = re.compile(r"(?<![A-Za-z0-9_])python3\b([^\n;|&)]+)")
    for match in command_re.finditer(shell_text):
        fragment = match.group(1)
        if ".py" not in fragment and "$" not in fragment:
            continue
        command = "python3" + fragment
        try:
            tokens = shlex.split(command, posix=True)
        except ValueError as exc:
            failures.append(f"{wrapper}: cannot parse Python invocation: {exc}")
            continue
        args = tokens[1:]
        if not args:
            continue
        if args[0] in {"-", "-c", "-m"}:
            continue
        if args[0].startswith("-"):
            if any(token.endswith(".py") or "$" in token for token in args[1:]):
                failures.append(
                    f"{wrapper}: Python options before an external helper are forbidden; "
                    "use exact `python3 scripts/<helper>.py` so discovery cannot be bypassed"
                )
            continue

        token = args[0]
        if "$" in token:
            failures.append(
                f"{wrapper}: dynamic Python helper path {token!r}; use literal "
                "`python3 scripts/<helper>.py` so floor discovery cannot be bypassed"
            )
        elif re.fullmatch(r"scripts/[A-Za-z0-9_./-]+\.py", token):
            helpers.add(pathlib.Path(token))
        else:
            failures.append(
                f"{wrapper}: external Python command target {token!r} is not a "
                "discoverable `scripts/<helper>.py` literal"
            )
    dynamic_interpreter_re = re.compile(
        r"(?m)(?:^|[;|&]\s*)['\"]?\$(?:\{[A-Za-z_][A-Za-z0-9_]*\}|"
        r"[A-Za-z_][A-Za-z0-9_]*)['\"]?\s+"
        r"(scripts/[A-Za-z0-9_./-]+\.py)\b"
    )
    for match in dynamic_interpreter_re.finditer(shell_text):
        failures.append(
            f"{wrapper}: dynamic interpreter launches {match.group(1)}; invoke the "
            "helper with literal `python3 scripts/<helper>.py` so discovery cannot be bypassed"
        )
    return helpers, failures


def discovery_parser_selftest():
    cases = (
        (
            "inline comment",
            "python3 scripts/live.py # python3 scripts/ignored.py",
            {pathlib.Path("scripts/live.py")},
            False,
        ),
        (
            "multiple heredocs",
            "cat <<ONE <<'TWO-CODE'\npython3 scripts/ignored1.py\nONE\n"
            "python3 scripts/ignored2.py\nTWO-CODE\npython3 scripts/live.py",
            {pathlib.Path("scripts/live.py")},
            False,
        ),
        (
            "option continuation",
            "python3 -I \\\n  scripts/blocked.py",
            set(),
            True,
        ),
        (
            "dynamic option path",
            'python3 -I "$HELPER"',
            set(),
            True,
        ),
        (
            "dynamic interpreter",
            'PYTHON=python3\n"$PYTHON" scripts/blocked.py',
            set(),
            True,
        ),
    )
    failures = []
    for name, source, expected_helpers, expect_failure in cases:
        helpers, case_failures = helpers_from_wrapper(
            pathlib.Path(f"<discovery-selftest:{name}>"), source
        )
        if helpers != expected_helpers or bool(case_failures) != expect_failure:
            failures.append(
                f"internal discovery parser self-test {name!r} failed: "
                f"helpers={sorted(str(path) for path in helpers)}, failures={case_failures}"
            )
    return failures


def discover_helpers():
    helpers = set()
    failures = []
    for pattern in AUDIT_GLOBS:
        for wrapper in sorted(pathlib.Path("scripts").glob(pattern)):
            found, wrapper_failures = helpers_from_wrapper(
                wrapper, wrapper.read_text(encoding="utf-8")
            )
            helpers.update(found)
            failures.extend(wrapper_failures)
    return sorted(helpers), failures


def annotation_nodes(tree):
    for node in ast.walk(tree):
        if isinstance(node, ast.arg) and node.annotation is not None:
            yield node.annotation
        elif isinstance(node, ast.AnnAssign):
            yield node.annotation
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.returns is not None:
                yield node.returns


def has_future_annotations(tree):
    return any(
        isinstance(node, ast.ImportFrom)
        and node.module == "__future__"
        and any(alias.name == "annotations" for alias in node.names)
        for node in tree.body
    )


def looks_like_type_operand(node):
    if isinstance(node, ast.Constant):
        return node.value is None
    if isinstance(node, ast.Name):
        return node.id in {
            "bool", "bytes", "complex", "dict", "float", "frozenset", "int",
            "list", "memoryview", "object", "range", "set", "str", "tuple", "type",
        } or node.id[:1].isupper()
    if isinstance(node, ast.Subscript):
        return looks_like_type_operand(node.value)
    if isinstance(node, ast.Attribute):
        return node.attr[:1].isupper() and not node.attr.isupper()
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        return looks_like_type_operand(node.left) and looks_like_type_operand(node.right)
    return False


def check_helper(path):
    failures = []
    if not path.is_file():
        return [f"{path}: wrapper invokes a missing Python helper"]

    source = path.read_text(encoding="utf-8")
    try:
        tree = ast.parse(source, filename=str(path), feature_version=FLOOR)
    except SyntaxError as exc:
        return [
            f"{path}:{exc.lineno}: syntax requires Python newer than "
            f"{version_text(FLOOR)}: {exc.msg}"
        ]

    annotations = list(annotation_nodes(tree))
    annotation_node_ids = {
        id(node) for annotation in annotations for node in ast.walk(annotation)
    }
    eager_union_lines = []
    for annotation in annotations:
        if any(
            isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr)
            for node in ast.walk(annotation)
        ):
            eager_union_lines.append(getattr(annotation, "lineno", 0))
    if eager_union_lines and not has_future_annotations(tree):
        lines = ", ".join(str(line) for line in sorted(set(eager_union_lines)))
        failures.append(
            f"{path}:{lines}: evaluated PEP 604 annotation needs "
            f"`from __future__ import annotations` on Python {version_text(FLOOR)}"
        )

    for node in ast.walk(tree):
        if (
            isinstance(node, ast.BinOp)
            and isinstance(node.op, ast.BitOr)
            and id(node) not in annotation_node_ids
            and looks_like_type_operand(node.left)
            and looks_like_type_operand(node.right)
        ):
            failures.append(
                f"{path}:{node.lineno}: evaluated PEP 604-style expression needs "
                f"a Python {version_text(FLOOR)} compatible representation; future "
                "annotations do not postpone ordinary assignment expressions"
            )

    module_aliases = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                module = alias.name.split(".", 1)[0]
                module_aliases[alias.asname or module] = module
                if module not in PY39_VERIFIED_IMPORTS:
                    failures.append(
                        f"{path}:{node.lineno}: import {module!r} is not in the "
                        f"Python {version_text(FLOOR)} verified import set"
                    )
        elif isinstance(node, ast.ImportFrom):
            module = node.module or ""
            for alias in node.names:
                if (module, alias.name) not in PY39_VERIFIED_FROM_IMPORTS:
                    failures.append(
                        f"{path}:{node.lineno}: from {module} import {alias.name} is not "
                        f"in the Python {version_text(FLOOR)} verified import set"
                    )
        else:
            continue

    changed = True
    while changed:
        changed = False
        for node in ast.walk(tree):
            if not isinstance(node, ast.Assign) or not isinstance(node.value, ast.Name):
                continue
            module = module_aliases.get(node.value.id)
            if module is None:
                continue
            for target in node.targets:
                if isinstance(target, ast.Name) and module_aliases.get(target.id) != module:
                    module_aliases[target.id] = module
                    changed = True

    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Attribute)
            and isinstance(node.value, ast.Name)
            and node.value.id in module_aliases
        ):
            member = (module_aliases[node.value.id], node.attr)
            if member not in PY39_VERIFIED_MODULE_ATTRIBUTES:
                failures.append(
                    f"{path}:{node.lineno}: {member[0]}.{member[1]} is not in the "
                    f"Python {version_text(FLOOR)} verified module-attribute set"
                )
        elif (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id == "__import__"
        ):
            failures.append(
                f"{path}:{node.lineno}: dynamic __import__ bypasses the Python "
                f"{version_text(FLOOR)} verified import set"
            )
        elif (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id in {"getattr", "hasattr"}
            and node.args
            and isinstance(node.args[0], ast.Name)
            and node.args[0].id in module_aliases
        ):
            failures.append(
                f"{path}:{node.lineno}: reflective {node.func.id} on module "
                f"{module_aliases[node.args[0].id]} bypasses the Python "
                f"{version_text(FLOOR)} verified module-attribute set"
            )

    return failures


helpers, failures = discover_helpers()
failures.extend(discovery_parser_selftest())
if not helpers:
    print("ERROR: no external Python helpers discovered in source-audit wrappers", file=sys.stderr)
    raise SystemExit(1)

for helper in helpers:
    failures.extend(check_helper(helper))

if failures:
    print(
        f"ERROR: ambient Python {version_text(FLOOR)} audit-helper compatibility failed "
        "(Issue #11102):",
        file=sys.stderr,
    )
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    print(
        "Use floor-compatible syntax/imports, postpone evaluated annotations, or invoke "
        "the tool through `uv` with explicit PEP 723 requires-python metadata.",
        file=sys.stderr,
    )
    raise SystemExit(1)

# Import-smoke every helper without entering its command-line main path. Each
# smoke gets its own process, environment copy, and temporary working directory
# so top-level cwd/global/environment changes cannot leak into another helper.
for helper in helpers:
    helper = helper.resolve()
    smoke = (
        "import runpy, sys\n"
        "try:\n"
        "    runpy.run_path(sys.argv[1], run_name='__sjulia_audit_import_smoke__')\n"
        "except BaseException as exc:\n"
        "    print(f'{type(exc).__name__}: {exc}', file=sys.stderr)\n"
        "    raise SystemExit(97)\n"
    )
    with tempfile.TemporaryDirectory(prefix="sjulia-python-audit-") as cwd:
        result = subprocess.run(
            [sys.executable, "-I", "-c", smoke, str(helper)],
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=15,
        )
    if result.returncode != 0:
        print(
            f"ERROR: {helper}: import smoke failed under Python "
            f"{sys.version_info.major}.{sys.version_info.minor} (exit {result.returncode}): "
            f"{result.stderr.strip() or result.stdout.strip() or 'no diagnostic'}",
            file=sys.stderr,
        )
        raise SystemExit(1)

print(
    f"OK: {len(helpers)} external source-audit Python helpers satisfy the "
    f"Python {version_text(FLOOR)} floor and import smoke."
)
PY
