#!/usr/bin/env python3
"""Generate subset_julia_vm/src/unicode.rs from Julia's REPL stdlib source files.

Source files (relative to repo root):
  julia/stdlib/REPL/src/latex_symbols.jl
  julia/stdlib/REPL/src/emoji_symbols.jl

Usage:
  python3 scripts/generate_unicode.py

The script writes to subset_julia_vm/src/unicode.rs.
Run from the repository root.
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent
LATEX_SRC = REPO_ROOT / "julia" / "stdlib" / "REPL" / "src" / "latex_symbols.jl"
EMOJI_SRC = REPO_ROOT / "julia" / "stdlib" / "REPL" / "src" / "emoji_symbols.jl"
OUTPUT = REPO_ROOT / "subset_julia_vm" / "src" / "unicode.rs"

# Pattern: "\\key" => "value",
ENTRY_RE = re.compile(r'"(\\[^"]+)"\s*=>\s*"([^"]+)"')

# Pattern: const NAME = "\\prefix"
CONST_RE = re.compile(r'^const\s+(\w+)\s*=\s*"([^"]+)"', re.MULTILINE)

# Pattern: NAME*"suffix" => "value"  (for computed keys like blackboard*"R")
COMPUTED_ENTRY_RE = re.compile(r'\b(\w+)\s*\*\s*"([^"]*)"\s*=>\s*"([^"]+)"')

# LaTeX aliases not in Julia stdlib but expected by tests and common in LaTeX usage
EXTRA_LATEX_ALIASES: list[tuple[str, str]] = [
    ("\\land", "∧"),  # alias for \wedge
    ("\\lor", "∨"),   # alias for \vee
]


def julia_unescape(s: str) -> str:
    """Unescape a Julia string literal content to its actual value.

    Julia string escapes handled here:
      \\\\  -> \\
      \\n   -> newline
      \\t   -> tab
      \\"   -> double-quote
      \\uXXXX     -> Unicode codepoint (4 hex digits)
      \\UXXXXXXXX -> Unicode codepoint (8 hex digits)
    """
    result: list[str] = []
    i = 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s):
            c = s[i + 1]
            if c == "\\":
                result.append("\\")
                i += 2
            elif c == "n":
                result.append("\n")
                i += 2
            elif c == "t":
                result.append("\t")
                i += 2
            elif c == '"':
                result.append('"')
                i += 2
            elif c == "u" and i + 5 <= len(s):
                hex_str = s[i + 2 : i + 6]
                if len(hex_str) == 4 and all(h in "0123456789abcdefABCDEF" for h in hex_str):
                    result.append(chr(int(hex_str, 16)))
                    i += 6
                else:
                    result.append(s[i])
                    i += 1
            elif c == "U" and i + 9 <= len(s):
                hex_str = s[i + 2 : i + 10]
                if len(hex_str) == 8 and all(h in "0123456789abcdefABCDEF" for h in hex_str):
                    result.append(chr(int(hex_str, 16)))
                    i += 10
                else:
                    result.append(s[i])
                    i += 1
            else:
                result.append(s[i])
                i += 1
        else:
            result.append(s[i])
            i += 1
    return "".join(result)


def parse_julia_dict(path: Path) -> list[tuple[str, str]]:
    """Extract (key, value) pairs from a Julia Dict literal file.

    Handles two entry forms found in Julia's latex_symbols.jl:
      1. Literal:  "\\\\key" => "value"
      2. Computed: constant_name*"suffix" => "value"
         where the constant is declared as: const constant_name = "\\\\prefix"
    """
    text = path.read_text(encoding="utf-8")

    # Build constant prefix table: e.g. blackboard -> "\\bb" (after unescape: \bb)
    constants: dict[str, str] = {
        name: julia_unescape(raw_val)
        for name, raw_val in CONST_RE.findall(text)
    }

    # Literal entries
    pairs: list[tuple[str, str]] = [
        (julia_unescape(k), julia_unescape(v))
        for k, v in ENTRY_RE.findall(text)
    ]

    # Computed entries: blackboard*"R" => "ℝ"  →  \bbR -> ℝ
    for const_name, suffix, raw_val in COMPUTED_ENTRY_RE.findall(text):
        if const_name in constants:
            key = constants[const_name] + julia_unescape(suffix)
            val = julia_unescape(raw_val)
            pairs.append((key, val))

    return pairs


def escape_rust_str(s: str) -> str:
    """Escape a string for use in a Rust string literal."""
    return s.replace("\\", "\\\\").replace('"', '\\"')


def emit_table(
    lines: list[str],
    name: str,
    pairs: list[tuple[str, str]],
    doc: str,
) -> None:
    """Emit a pub static HashMap table."""
    lines.append(f"/// {doc}")
    lines.append(
        f"pub static {name}: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {{"
    )
    lines.append(f"    let mut m = HashMap::with_capacity({len(pairs)});")
    for key, val in pairs:
        rkey = escape_rust_str(key)
        rval = escape_rust_str(val)
        lines.append(f'    m.insert("{rkey}", "{rval}");')
    lines.append("    m")
    lines.append("});")
    lines.append("")


HELPER_FUNCTIONS = '''\
/// Look up a LaTeX command or emoji name and return its Unicode representation
pub fn latex_to_unicode(latex: &str) -> Option<&'static str> {
    // First check LaTeX symbols
    if let Some(unicode) = LATEX_SYMBOLS.get(latex).copied() {
        return Some(unicode);
    }
    // Then check emoji symbols
    EMOJI_SYMBOLS.get(latex).copied()
}

/// Look up a Unicode character and return its LaTeX/emoji representation
pub fn unicode_to_latex(unicode: &str) -> Option<&'static str> {
    // First check LaTeX symbols
    if let Some(latex) = UNICODE_TO_LATEX.get(unicode).copied() {
        return Some(latex);
    }
    // Then check emoji symbols
    UNICODE_TO_EMOJI.get(unicode).copied()
}

/// Get all LaTeX commands and emoji names that start with a given prefix
pub fn completions_for_prefix(prefix: &str) -> Vec<(&'static str, &'static str)> {
    let mut results: Vec<_> = LATEX_SYMBOLS
        .iter()
        .filter(|(latex, _)| latex.starts_with(prefix))
        .map(|(&latex, &unicode)| (latex, unicode))
        .collect();

    // Also search emoji symbols
    results.extend(
        EMOJI_SYMBOLS
            .iter()
            .filter(|(emoji, _)| emoji.starts_with(prefix))
            .map(|(&emoji, &unicode)| (emoji, unicode)),
    );

    results.sort_by(|a, b| a.0.cmp(b.0));
    results
}

/// Apply LaTeX completions to a string
/// Replaces all LaTeX sequences (e.g., \\alpha) with their Unicode equivalents
pub fn expand_latex_in_string(input: &str) -> String {
    let mut result = input.to_string();

    // Sort by length descending to replace longer matches first
    let mut entries: Vec<_> = LATEX_SYMBOLS.iter().collect();
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for (&latex, &unicode) in entries {
        result = result.replace(latex, unicode);
    }

    result
}
'''

REVERSE_TABLES = '''\
/// Reverse mapping: Unicode to LaTeX command (canonical form, first-wins)
pub static UNICODE_TO_LATEX: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (&latex, &unicode) in LATEX_SYMBOLS.iter() {
        // Only insert if not already present (first one wins = canonical)
        m.entry(unicode).or_insert(latex);
    }
    m
});

/// Reverse mapping: Unicode to Emoji name (canonical form)
pub static UNICODE_TO_EMOJI: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (&emoji_name, &unicode) in EMOJI_SYMBOLS.iter() {
        // Only insert if not already present (first one wins = canonical)
        m.entry(unicode).or_insert(emoji_name);
    }
    m
});
'''


def main() -> None:
    if not LATEX_SRC.exists():
        print(f"ERROR: {LATEX_SRC} not found. Run from the repo root.", file=sys.stderr)
        sys.exit(1)

    latex_pairs = parse_julia_dict(LATEX_SRC) + EXTRA_LATEX_ALIASES
    emoji_pairs = parse_julia_dict(EMOJI_SRC)

    # Deduplicate each table, keeping first occurrence
    def dedup(pairs: list[tuple[str, str]]) -> list[tuple[str, str]]:
        seen: set[str] = set()
        result: list[tuple[str, str]] = []
        for key, val in pairs:
            if key not in seen:
                seen.add(key)
                result.append((key, val))
        return sorted(result, key=lambda kv: kv[0])

    latex_unique = dedup(latex_pairs)
    emoji_unique = dedup(emoji_pairs)

    lines: list[str] = []
    lines.append("// @generated — do not edit manually.")
    lines.append("// Re-generate with: python3 scripts/generate_unicode.py")
    lines.append("//")
    lines.append("// Source files (Julia stdlib, MIT License):")
    lines.append("//   julia/stdlib/REPL/src/latex_symbols.jl")
    lines.append("//   julia/stdlib/REPL/src/emoji_symbols.jl")
    lines.append("//")
    lines.append(
        f"// Entries: {len(latex_unique)} LaTeX, {len(emoji_unique)} emoji"
    )
    lines.append("")
    lines.append("use once_cell::sync::Lazy;")
    lines.append("use std::collections::HashMap;")
    lines.append("")

    emit_table(
        lines,
        "LATEX_SYMBOLS",
        latex_unique,
        "LaTeX to Unicode mapping. Ported from Julia's stdlib/REPL/src/latex_symbols.jl.",
    )

    emit_table(
        lines,
        "EMOJI_SYMBOLS",
        emoji_unique,
        "Emoji name to Unicode mapping. Ported from Julia's stdlib/REPL/src/emoji_symbols.jl.",
    )

    lines.append(REVERSE_TABLES)
    lines.append(HELPER_FUNCTIONS)

    content = "\n".join(lines)
    OUTPUT.write_text(content, encoding="utf-8")
    print(
        f"Wrote {len(latex_unique)} LaTeX + {len(emoji_unique)} emoji entries to {OUTPUT}"
    )


if __name__ == "__main__":
    main()
