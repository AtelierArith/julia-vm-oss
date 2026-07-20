//! Malformed/truncated/adversarial-input differential test (Issue #10904,
//! Phase 1a of the panic-debt retirement epic #10869).
//!
//! `subset_julia_vm_parser/src/parser` had ~105 `.unwrap()` call sites (plus
//! one real `.expect(...)`) that were converted to checked `ParseError`
//! returns as part of #10904 — most were proof-backed invariants (safe today
//! by control-flow construction, but not type-enforced) and two were
//! genuinely reachable panics on truncated input (`parse_identifier` in
//! `literals.rs` and `parse_field_identifier` in `expressions/field.rs`, both
//! triggered by a truncated `struct`/`abstract type`/`a.` with nothing
//! after). This file is the "malformed/truncated/adversarial Julia source
//! differential test" the epic's acceptance criteria call for: it asserts
//! that a broad set of malformed inputs return a typed `ParseError` (never a
//! Rust panic) through the crate's public entrypoints.
//!
//! `subset_julia_vm_parser::corpus::sweep_source` already runs the parse on a
//! dedicated large-stack thread wrapped in `std::panic::catch_unwind`
//! (Issue #8614/#8635's corpus-sweep infrastructure), converting any panic
//! into a `FileOutcome::Panic` value instead of crashing the test process, so
//! it is reused here as the panic-catching harness.

use std::panic::{self, AssertUnwindSafe};
use subset_julia_vm_parser::corpus::{sweep_source, FileOutcome};
use subset_julia_vm_parser::parse_with_errors;

/// Assert that parsing `source` never panics, regardless of whether it
/// ultimately succeeds or reports a `ParseError`.
fn assert_no_panic(label: &str, source: &str) {
    match sweep_source(label, source) {
        FileOutcome::Panic(record) => panic!(
            "parser panicked on malformed input {label:?} (source {source:?}): {}",
            record.message
        ),
        FileOutcome::Ok | FileOutcome::Errors(_) => {}
    }
}

/// Assert that parsing `source` never panics AND reports at least one typed
/// `ParseError` (used for inputs that are unambiguously malformed, as opposed
/// to merely-boundary cases like an empty file that legitimately parse OK).
fn assert_reports_typed_error_not_panic(label: &str, source: &str) {
    match sweep_source(label, source) {
        FileOutcome::Panic(record) => panic!(
            "parser panicked on malformed input {label:?} (source {source:?}): {}",
            record.message
        ),
        FileOutcome::Ok => panic!(
            "expected malformed input {label:?} (source {source:?}) to report a ParseError, but it parsed cleanly"
        ),
        FileOutcome::Errors(_) => {}
    }
}

/// Truncated expressions/statements: a keyword or partial construct with
/// nothing (or not enough) following it.
const TRUNCATED: &[&str] = &[
    "struct",
    "struct Foo",
    "mutable struct",
    "abstract type",
    "abstract type Foo",
    "primitive type",
    "primitive type Foo",
    "module",
    "baremodule",
    "function",
    "function f",
    "function f(",
    "macro",
    "macro m",
    "if true",
    "if true\n1",
    "for i in",
    "for i in 1:10",
    "while",
    "while true",
    "try",
    "let",
    "let x =",
    "quote",
    "begin",
    "using",
    "using Foo:",
    "import",
    "import Foo:",
    "export",
    "public",
    "const",
    "global",
    "local",
    // Note: bare `return` (no value) is valid upstream Julia (implicit
    // `return nothing`), so it is intentionally NOT in this truncated list.
    "x = (1 +",
    "x = [1, 2",
    "x = (1, 2",
    "x =",
    "1:2:",
    "a.",
    "df.",
    "a.b.",
    "a.:",
    "@",
    "T{",
    "f(x::Int, y::Int) where",
    "f(x) do",
    "\"unterminated",
    "\"\"\"unterminated triple",
    "'unterminated",
    "#= unterminated block comment",
    "#= nested #= comment =# still open",
];

// Note: `@macro` (a bare macro-call identifier with zero arguments) is
// intentionally NOT in `TRUNCATED` — it is valid Julia syntax upstream (a
// `MacrocallExpression` with no args; upstream only fails later, at
// *runtime*, with `UndefVarError` for the undefined macro), not a parse
// error.

/// Unbalanced brackets/quotes and other structural mismatches.
const UNBALANCED: &[&str] = &[
    "]",
    "}",
    ")",
    "((()",
    "[[[",
    "{{{",
    "[1, 2)]",
    "(1, 2]",
    "{1, 2)",
    "f(1, 2]",
    "[1 2; 3 4",
    "(1, 2",
    "''",
    "'ab'",
    ")\n]\n",
];

/// Stray/garbage operator sequences with no valid operand. Every entry here
/// was verified against upstream `julia` 1.12.6 to actually be a `ParseError`
/// (`julia --startup-file=no -e 'include(...)'` on a file containing exactly
/// that source) before being added — several plausible-looking candidates
/// turned out to be valid Julia and are called out below instead.
///
/// Not included, because they are valid upstream syntax (operator-as-value
/// expressions, or — for `..`/`|>`/`==`/`===`/`!==` — simply undefined at
/// runtime, not a parse error; `;;;`/`:` alone are likewise syntactically
/// valid): `1 + + 2` (`1 + (+2)`, unary plus), `..`, `|>`, `==`, `===`,
/// `!==`, `:`, `;;;`. `->` and `end` are also excluded — both ARE genuine
/// upstream `ParseError`s, but sjulia currently parses them cleanly (known
/// wrong-parse gaps, Issues #10917 and #10918 respectively), so they are
/// exercised only by `known_wrong_parse_gaps_never_panic` below with the
/// weaker "never panics" assertion.
const STRAY_OPERATORS: &[&str] = &["= =", "?", ",", "else", "elseif", "catch", "finally"];

/// Empty / whitespace-only inputs. These legitimately parse to an empty
/// `SourceFile` — the point is only that they must not panic.
const EMPTY_OR_WHITESPACE: &[&str] = &["", " ", "\n\n\n", "\t\t\t", "\r\n", "# comment only\n"];

/// Valid-UTF-8-but-unusual byte/character forms: embedded NUL bytes, exotic
/// Unicode (zero-width joiners, RTL override, combining marks, astral-plane
/// emoji, BOM). Rust's `&str` guarantees valid UTF-8, so "invalid UTF-8" is
/// not directly constructible; these are the adjacent edge cases that are
/// syntactically bizarre without violating the `&str` invariant.
const UNICODE_AND_NUL: &[&str] = &[
    "x\0 = 1",
    "\0",
    "\"a\0b\"",
    "'\\0'",
    "\u{1F600} = 1",        // 😀 as an (invalid) identifier
    "\u{202E}x = 1",        // right-to-left override
    "\u{FEFF}x = 1",        // BOM
    "e\u{0301} = 1",        // bare combining acute accent after `e`
    "x = 1; y\u{200B} = 2", // zero-width space
];

#[test]
fn truncated_constructs_never_panic() {
    for source in TRUNCATED {
        assert_reports_typed_error_not_panic(source, source);
    }
}

#[test]
fn unbalanced_brackets_and_quotes_never_panic() {
    for source in UNBALANCED {
        assert_reports_typed_error_not_panic(source, source);
    }
}

#[test]
fn stray_operator_sequences_never_panic() {
    for source in STRAY_OPERATORS {
        assert_reports_typed_error_not_panic(source, source);
    }
}

/// Formerly known wrong-parse gaps, now fixed and asserted as typed errors:
/// `::::` (Issue #10915) is rejected as a premature-end-of-input
/// `ParseError` (the trailing `::` recurses into the unary-typed grammar and
/// hits EOF, matching upstream), and a bare `->` (Issue #10917) plus the bare
/// short-circuit syntactic operators (Issue #10932) are rejected as
/// `invalid identifier` (unlike other operators such as `+`/`-`/`..`/`|>`,
/// which upstream genuinely does accept as bare values).
///
/// `end` remains a known gap (Issue #10918): sjulia's `parse_primary` treats
/// `Token::KwEnd` as a plain identifier unconditionally (needed for `a[end]`
/// indexing), even at bare top level where upstream requires an enclosing
/// block/index context.
#[test]
fn known_wrong_parse_gaps_never_panic() {
    assert_reports_typed_error_not_panic("::::", "::::"); // Issue #10915 (fixed)
    assert_reports_typed_error_not_panic("->", "->"); // Issue #10917 (fixed)
    assert_reports_typed_error_not_panic("&&", "&&"); // Issue #10932 (fixed)
    assert_no_panic("end", "end"); // Issue #10918
}

#[test]
fn empty_or_whitespace_only_inputs_never_panic() {
    for source in EMPTY_OR_WHITESPACE {
        assert_no_panic(source, source);
    }
}

#[test]
fn unicode_and_nul_byte_edge_cases_never_panic() {
    for source in UNICODE_AND_NUL {
        assert_no_panic(source, source);
    }
}

/// Bounded deep nesting. Real native-stack-overflow robustness for
/// pathologically deep recursive-descent input is out of scope for #10904
/// (that is a SIGABRT/SIGSEGV, not a catchable `panic!`, and is tracked
/// separately per `docs/vm/PANIC_FREE.md`'s "Bounding Host Recursion"
/// section) — this only exercises that moderately deep nesting does not
/// panic through any of the sites converted in this issue.
#[test]
fn bounded_deep_nesting_never_panics() {
    let deep_parens: String = "(".repeat(500) + "1" + &")".repeat(500);
    let deep_brackets: String = "[".repeat(500) + "1" + &"]".repeat(500);
    let deep_quote: String = "quote\n".repeat(200) + "1\n" + &"end\n".repeat(200);
    assert_no_panic("deep_parens", &deep_parens);
    assert_no_panic("deep_brackets", &deep_brackets);
    assert_no_panic("deep_quote", &deep_quote);
}

/// A deterministic single-character-substitution fuzz sweep over every
/// TRUNCATED/UNBALANCED/STRAY_OPERATORS input: for each byte position, swap
/// in a small set of "garbage" characters (mismatched brackets/quotes/
/// operators) and re-parse. Not exhaustive fuzzing, but a bounded, repeatable
/// sweep that goes beyond simple prefix truncation — this is how the two
/// real bugs fixed by #10904 (`literals.rs` `parse_identifier`,
/// `expressions/field.rs` `parse_field_identifier`) were originally found.
#[test]
fn single_character_substitution_fuzz_never_panics() {
    const GARBAGE: &[char] = &[
        ')', ']', '}', '"', '\'', '@', '$', '#', '.', ',', ':', ';', '=', '<', '>', '+', '-', '*',
        '/', '\\', '0',
    ];
    let base_sources: Vec<&str> = TRUNCATED
        .iter()
        .chain(UNBALANCED.iter())
        .chain(STRAY_OPERATORS.iter())
        .copied()
        .collect();

    for source in &base_sources {
        let chars: Vec<char> = source.chars().collect();
        for i in 0..chars.len() {
            for &g in GARBAGE {
                let mut mutated = chars.clone();
                mutated[i] = g;
                let mutated: String = mutated.into_iter().collect();
                assert_no_panic(&mutated, &mutated);
            }
        }
    }
}

/// Direct regression coverage for the two genuinely reachable panics fixed by
/// #10904, using `parse_with_errors` (the public entrypoint most callers
/// use) directly under `catch_unwind` rather than through `sweep_source`, so
/// this test also documents the exact call path a caller hits.
#[test]
fn regression_struct_and_field_truncation_return_typed_error_not_panic_issue_10904() {
    for source in [
        "struct",
        "abstract type",
        "primitive type",
        "module",
        "a.",
        "df.",
    ] {
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_with_errors(source)));
        let (_, errors) = result
            .unwrap_or_else(|_| panic!("parser panicked on {source:?} (Issue #10904 regression)"));
        assert!(
            !errors.is_empty(),
            "expected {source:?} to report a ParseError, got a clean parse"
        );
    }
}

/// The malformed-source corpus feeds into Phase 3's broader cross-crate
/// corpus (Issue #10908); this asserts the whole curated set above is
/// simultaneously panic-free through the crate's top-level `parse_with_errors`
/// entrypoint too, not just through `corpus::sweep_source`.
#[test]
fn full_curated_corpus_never_panics_through_parse_with_errors() {
    for source in TRUNCATED
        .iter()
        .chain(UNBALANCED.iter())
        .chain(STRAY_OPERATORS.iter())
        .chain(EMPTY_OR_WHITESPACE.iter())
        .chain(UNICODE_AND_NUL.iter())
    {
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_with_errors(source)));
        assert!(
            result.is_ok(),
            "parser panicked on malformed input {source:?} via parse_with_errors"
        );
    }
}
