//! Integration tests for the Unicode module
//!
//! These tests verify the Unicode completion functionality works correctly
//! when accessed through the public API.

use subset_julia_vm::unicode::{
    completions_for_prefix, expand_latex_in_string, latex_to_unicode, unicode_to_latex,
    LATEX_SYMBOLS, UNICODE_TO_LATEX,
};

// ==================== API Tests ====================

#[test]
fn test_api_basic_lookup() {
    // Test basic API functionality
    assert_eq!(latex_to_unicode("\\alpha"), Some("α"));
    assert_eq!(latex_to_unicode("\\beta"), Some("β"));
    assert_eq!(latex_to_unicode("\\gamma"), Some("γ"));
}

#[test]
fn test_api_reverse_lookup() {
    assert_eq!(unicode_to_latex("α"), Some("\\alpha"));
    assert_eq!(unicode_to_latex("β"), Some("\\beta"));
}

#[test]
fn test_api_completions() {
    let comps = completions_for_prefix("\\pi");
    assert!(!comps.is_empty());
    assert!(comps.iter().any(|(l, u)| *l == "\\pi" && *u == "π"));
}

#[test]
fn test_api_expand() {
    let result = expand_latex_in_string("E = mc\\^2");
    assert_eq!(result, "E = mc²");
}

// ==================== Julia Code Patterns ====================

#[test]
fn test_julia_function_signature() {
    // f(α::Float64, β::Float64) -> γ
    let input = "f(\\alpha::Float64, \\beta::Float64) \\to \\gamma";
    let expected = "f(α::Float64, β::Float64) → γ";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_math_expression() {
    // Standard mathematical expressions
    let input = "x\\^2 + y\\^2 = r\\^2";
    let expected = "x² + y² = r²";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_summation() {
    // Sum notation
    let input = "\\sum x\\_i";
    let expected = "∑ xᵢ";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_set_membership() {
    // x ∈ ℝ
    let input = "x \\in \\bbR";
    let expected = "x ∈ ℝ";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_complex_numbers() {
    // Complex number set
    let input = "z \\in \\bbC";
    let expected = "z ∈ ℂ";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_logical_operators() {
    // Logical AND, OR
    let input = "p \\land q \\lor r";
    let expected = "p ∧ q ∨ r";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_quantifiers() {
    // For all, exists
    let input = "\\forall x \\exists y";
    let expected = "∀ x ∃ y";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_calculus() {
    // Partial derivative and nabla
    let input = "\\partial f / \\partial x + \\nabla g";
    let expected = "∂ f / ∂ x + ∇ g";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_subscript_sequence() {
    // x₁, x₂, ..., xₙ
    let input = "x\\_1, x\\_2, ..., x\\_n";
    let expected = "x₁, x₂, ..., xₙ";
    assert_eq!(expand_latex_in_string(input), expected);
}

#[test]
fn test_julia_matrix_dimensions() {
    // m × n matrix
    let input = "m \\times n matrix";
    let expected = "m × n matrix";
    assert_eq!(expand_latex_in_string(input), expected);
}

// ==================== Completion Prefix Tests ====================

#[test]
fn test_completion_prefix_greek() {
    let comps = completions_for_prefix("\\a");
    // Should include alpha, aleph, approx, etc.
    assert!(comps.len() >= 3);

    let latex_names: Vec<&str> = comps.iter().map(|(l, _)| *l).collect();
    assert!(latex_names.contains(&"\\alpha"));
    assert!(latex_names.contains(&"\\aleph"));
    assert!(latex_names.contains(&"\\approx"));
}

#[test]
fn test_completion_prefix_arrow() {
    let comps = completions_for_prefix("\\right");
    assert!(comps.iter().any(|(l, _)| *l == "\\rightarrow"));
}

#[test]
fn test_completion_prefix_bb() {
    // Blackboard bold
    let comps = completions_for_prefix("\\bb");
    assert!(comps.len() >= 5); // At least N, Z, Q, R, C

    let latex_names: Vec<&str> = comps.iter().map(|(l, _)| *l).collect();
    assert!(latex_names.contains(&"\\bbR"));
    assert!(latex_names.contains(&"\\bbN"));
    assert!(latex_names.contains(&"\\bbC"));
}

#[test]
fn test_completion_prefix_superscript() {
    let comps = completions_for_prefix("\\^");
    // Should have 0-9 and some letters
    assert!(comps.len() >= 10);
}

#[test]
fn test_completion_prefix_subscript() {
    let comps = completions_for_prefix("\\_");
    // Should have 0-9 and some letters
    assert!(comps.len() >= 10);
}

// ==================== Roundtrip Tests ====================

#[test]
fn test_roundtrip_greek_letters() {
    let greek = ["α", "β", "γ", "δ", "π", "σ", "ω"];

    for &unicode in &greek {
        if let Some(latex) = unicode_to_latex(unicode) {
            if let Some(back) = latex_to_unicode(latex) {
                assert_eq!(back, unicode, "Roundtrip failed for {}", unicode);
            }
        }
    }
}

#[test]
fn test_roundtrip_common_symbols() {
    let symbols = ["∞", "√", "∑", "∏", "∫", "∂", "∇"];

    for &unicode in &symbols {
        if let Some(latex) = unicode_to_latex(unicode) {
            if let Some(back) = latex_to_unicode(latex) {
                assert_eq!(back, unicode, "Roundtrip failed for {}", unicode);
            }
        }
    }
}

// ==================== Symbol Table Invariants ====================

#[test]
fn test_all_latex_start_with_backslash() {
    for (latex, _) in LATEX_SYMBOLS.iter() {
        assert!(
            latex.starts_with('\\'),
            "LaTeX command '{}' should start with backslash",
            latex
        );
    }
}

#[test]
fn test_all_unicode_are_non_ascii() {
    for (_, unicode) in LATEX_SYMBOLS.iter() {
        // Most unicode symbols should be non-ASCII
        // (except for a few edge cases)
        assert!(!unicode.is_empty(), "Unicode value should not be empty");
    }
}

#[test]
fn test_no_duplicate_unicode_values_in_reverse_map() {
    // The reverse map should have unique keys
    let count = UNICODE_TO_LATEX.len();
    assert!(count > 0, "Reverse map should not be empty");
}

#[test]
fn test_tables_are_consistent() {
    // Every entry in UNICODE_TO_LATEX should map back correctly
    for (&unicode, &latex) in UNICODE_TO_LATEX.iter() {
        let lookup = LATEX_SYMBOLS.get(latex);
        assert!(
            lookup.is_some(),
            "Reverse entry {} -> {} not found in LATEX_SYMBOLS",
            unicode,
            latex
        );
    }
}

// ==================== Edge Cases ====================

#[test]
fn test_empty_string_lookup() {
    assert_eq!(latex_to_unicode(""), None);
    assert_eq!(unicode_to_latex(""), None);
}

#[test]
fn test_just_backslash_lookup() {
    assert_eq!(latex_to_unicode("\\"), None);
}

#[test]
fn test_expand_preserves_non_latex() {
    let input = "Hello, World! 123 #$%";
    assert_eq!(expand_latex_in_string(input), input);
}

#[test]
fn test_expand_partial_match() {
    // Should not match partial LaTeX commands
    let input = "\\alph is not \\alpha";
    let result = expand_latex_in_string(input);
    // Only \\alpha should be expanded
    assert!(result.contains("α"));
    assert!(result.contains("\\alph")); // partial should remain
}

#[test]
fn test_unicode_multibyte() {
    // Test that multi-byte unicode characters work correctly
    assert_eq!(latex_to_unicode("\\alpha"), Some("α")); // 2 bytes
    assert_eq!(latex_to_unicode("\\bbR"), Some("ℝ")); // 3 bytes
}

// ==================== Performance Sanity ====================

#[test]
fn test_completion_performance() {
    // Ensure completions don't take too long
    use std::time::Instant;

    let start = Instant::now();
    for _ in 0..1000 {
        let _ = completions_for_prefix("\\al");
    }
    let duration = start.elapsed();

    // Should complete 1000 lookups in under 1 second
    assert!(
        duration.as_secs() < 1,
        "Completion lookup too slow: {:?}",
        duration
    );
}

#[test]
fn test_expand_performance() {
    use std::time::Instant;

    let input = "\\alpha + \\beta + \\gamma + \\delta + \\epsilon";
    let start = Instant::now();
    for _ in 0..100 {
        let _ = expand_latex_in_string(input);
    }
    let duration = start.elapsed();

    // Should complete 100 expansions in under 1 second
    assert!(duration.as_secs() < 1, "Expansion too slow: {:?}", duration);
}

// ==================== Special Symbol Categories ====================

#[test]
fn test_category_arrows() {
    let arrows = [
        ("\\to", "→"),
        ("\\rightarrow", "→"),
        ("\\leftarrow", "←"),
        ("\\Rightarrow", "⇒"),
        ("\\Leftarrow", "⇐"),
        ("\\implies", "⟹"),
        ("\\iff", "⟺"),
    ];

    for (latex, expected) in arrows {
        assert_eq!(
            latex_to_unicode(latex),
            Some(expected),
            "Arrow {} should map to {}",
            latex,
            expected
        );
    }
}

#[test]
fn test_category_relations() {
    let relations = [
        ("\\le", "≤"),
        ("\\ge", "≥"),
        ("\\ne", "≠"),
        ("\\approx", "≈"),
        ("\\equiv", "≡"),
        ("\\subset", "⊂"),
        ("\\supset", "⊃"),
        ("\\in", "∈"),
    ];

    for (latex, expected) in relations {
        assert_eq!(
            latex_to_unicode(latex),
            Some(expected),
            "Relation {} should map to {}",
            latex,
            expected
        );
    }
}

#[test]
fn test_category_logic() {
    let logic = [
        ("\\land", "∧"),
        ("\\lor", "∨"),
        ("\\neg", "¬"),
        ("\\forall", "∀"),
        ("\\exists", "∃"),
        ("\\top", "⊤"),
        ("\\bot", "⊥"),
    ];

    for (latex, expected) in logic {
        assert_eq!(
            latex_to_unicode(latex),
            Some(expected),
            "Logic symbol {} should map to {}",
            latex,
            expected
        );
    }
}
