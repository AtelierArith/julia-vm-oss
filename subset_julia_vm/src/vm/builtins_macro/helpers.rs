//! Helper functions for macro system builtins.
//!
//! Meta.isidentifier, Meta.isoperator, and related functions.

/// Boolean literal keywords that are not valid identifiers.
/// Julia's Meta.isidentifier rejects only "true" and "false" (boolean literals),
/// not other keywords like "if", "for", "function" etc.
/// See: julia/base/meta.jl `isidentifier` function.
pub(super) const BOOLEAN_LITERAL_KEYWORDS: &[&str] = &["true", "false"];

/// Unary operators in Julia
pub(super) const UNARY_OPERATORS: &[&str] = &["+", "-", "!", "~", "¬", "√", "∛", "∜"];

/// Binary operators in Julia
pub(super) const BINARY_OPERATORS: &[&str] = &[
    // Arithmetic
    "+", "-", "*", "/", "\\", "^", "%", "÷", "⋅", "×", // Comparison
    "<", ">", "<=", ">=", "==", "!=", "===", "!==", "≤", "≥", "≠", // Logical/Bitwise
    "&", "|", "⊻", "<<", ">>", ">>>", // Other
    "in", "isa", "∈", "∉", "⊂", "⊃", "⊆", "⊇", // Range
    ":", "..", // Assignment (compound)
    "+=", "-=", "*=", "/=", "\\=", "^=", "%=", "&=", "|=", "⊻=", // Type
    "<:", ">:",
];

/// Check if character is a valid identifier start character
pub(super) fn is_id_start_char(c: char) -> bool {
    // Underscore
    if c == '_' {
        return true;
    }
    // ASCII letters
    if c.is_ascii_alphabetic() {
        return true;
    }
    // Greek letters (basic range)
    let code = c as u32;
    // Greek lowercase α-ω (945-969)
    if (945..=969).contains(&code) {
        return true;
    }
    // Greek uppercase Α-Ω (913-937)
    if (913..=937).contains(&code) {
        return true;
    }
    // Additional Unicode ID_Start characters
    c.is_alphabetic()
}

/// Check if character is a valid identifier continuation character
pub(super) fn is_id_char(c: char) -> bool {
    if is_id_start_char(c) {
        return true;
    }
    // ASCII digits
    if c.is_ascii_digit() {
        return true;
    }
    // Exclamation mark (for function names like push!)
    if c == '!' {
        return true;
    }
    false
}

/// Check if a string is a valid Julia identifier.
///
/// Matches Julia's `Meta.isidentifier` behavior:
/// - Returns `false` for empty strings
/// - Returns `false` for boolean literals "true" and "false"
/// - Returns `true` for other keywords like "if", "for", "function" (Julia allows these)
/// - First character must be a letter, underscore, or Unicode letter
/// - Remaining characters can be letters, digits, underscores, `!`, or Unicode letters
pub(super) fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Only boolean literals "true" and "false" are rejected by Julia's Meta.isidentifier.
    // Other keywords like "if", "for", "function" ARE valid identifiers per Julia semantics.
    if BOOLEAN_LITERAL_KEYWORDS.contains(&s) {
        return false;
    }
    // Check first character
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !is_id_start_char(first) {
        return false;
    }
    // Check remaining characters
    for c in chars {
        if !is_id_char(c) {
            return false;
        }
    }
    true
}

/// Check if a string is any operator
pub(super) fn is_operator(s: &str) -> bool {
    is_unary_operator(s) || is_binary_operator(s) || is_postfix_operator(s)
}

/// Check if a string is a unary operator
pub(super) fn is_unary_operator(s: &str) -> bool {
    UNARY_OPERATORS.contains(&s)
}

/// Check if a string is a binary operator
pub(super) fn is_binary_operator(s: &str) -> bool {
    BINARY_OPERATORS.contains(&s)
}

/// Check if a string is a postfix operator (e.g., ')
pub(super) fn is_postfix_operator(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Postfix operators start with ' (transpose/adjoint)
    s.starts_with('\'')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_id_start_char ──────────────────────────────────────────────────────

    #[test]
    fn test_is_id_start_char_ascii_letters() {
        assert!(is_id_start_char('a'), "lowercase letter should be valid start");
        assert!(is_id_start_char('Z'), "uppercase letter should be valid start");
    }

    #[test]
    fn test_is_id_start_char_underscore() {
        assert!(is_id_start_char('_'), "underscore should be valid start");
    }

    #[test]
    fn test_is_id_start_char_digits_not_valid() {
        assert!(!is_id_start_char('0'), "digit should not be valid start");
        assert!(!is_id_start_char('9'), "digit should not be valid start");
    }

    #[test]
    fn test_is_id_start_char_greek_letters() {
        assert!(is_id_start_char('α'), "Greek alpha should be valid start");
        assert!(is_id_start_char('Ω'), "Greek Omega should be valid start");
    }

    // ── is_id_char ────────────────────────────────────────────────────────────

    #[test]
    fn test_is_id_char_includes_digits() {
        assert!(is_id_char('5'), "digit should be valid id continuation");
        assert!(is_id_char('0'), "zero should be valid id continuation");
    }

    #[test]
    fn test_is_id_char_includes_exclamation() {
        // push!, take! etc.
        assert!(is_id_char('!'), "! should be valid id continuation");
    }

    #[test]
    fn test_is_id_char_excludes_operators() {
        assert!(!is_id_char('+'), "+ should not be valid id char");
        assert!(!is_id_char(' '), "space should not be valid id char");
    }

    // ── is_valid_identifier ───────────────────────────────────────────────────

    #[test]
    fn test_is_valid_identifier_simple_names() {
        assert!(is_valid_identifier("x"), "single letter is valid");
        assert!(is_valid_identifier("foo"), "simple name is valid");
        assert!(is_valid_identifier("foo_bar"), "underscore-separated is valid");
        assert!(is_valid_identifier("_private"), "leading underscore is valid");
    }

    #[test]
    fn test_is_valid_identifier_with_bang() {
        assert!(is_valid_identifier("push!"), "push! is a valid identifier");
        assert!(is_valid_identifier("take!"), "take! is a valid identifier");
    }

    #[test]
    fn test_is_valid_identifier_empty_returns_false() {
        assert!(!is_valid_identifier(""), "empty string is not valid");
    }

    #[test]
    fn test_is_valid_identifier_digits_start_returns_false() {
        assert!(!is_valid_identifier("1foo"), "digit-start is invalid");
        assert!(!is_valid_identifier("42"), "pure number is invalid");
    }

    #[test]
    fn test_is_valid_identifier_boolean_literals_rejected() {
        // Julia's Meta.isidentifier rejects "true" and "false" (only these two)
        assert!(!is_valid_identifier("true"), "\"true\" is not a valid identifier");
        assert!(!is_valid_identifier("false"), "\"false\" is not a valid identifier");
    }

    #[test]
    fn test_is_valid_identifier_other_keywords_accepted() {
        // Julia keywords (other than booleans) ARE valid identifiers per Meta.isidentifier
        assert!(is_valid_identifier("if"), "\"if\" is valid per Julia semantics");
        assert!(is_valid_identifier("for"), "\"for\" is valid per Julia semantics");
        assert!(is_valid_identifier("function"), "\"function\" is valid per Julia semantics");
    }

    // ── is_operator / is_unary_operator / is_binary_operator / is_postfix_operator ─

    #[test]
    fn test_is_unary_operator_standard() {
        assert!(is_unary_operator("+"), "+ is a unary operator");
        assert!(is_unary_operator("-"), "- is a unary operator");
        assert!(is_unary_operator("!"), "! is a unary operator");
        assert!(is_unary_operator("~"), "~ is a unary operator");
    }

    #[test]
    fn test_is_unary_operator_unicode() {
        assert!(is_unary_operator("√"), "√ is a unary operator");
        assert!(is_unary_operator("¬"), "¬ is a unary operator");
    }

    #[test]
    fn test_is_binary_operator_arithmetic() {
        assert!(is_binary_operator("*"), "* is a binary operator");
        assert!(is_binary_operator("/"), "/ is a binary operator");
        assert!(is_binary_operator("=="), "== is a binary operator");
    }

    #[test]
    fn test_is_binary_operator_non_operators() {
        assert!(!is_binary_operator(""), "empty string is not a binary operator");
        assert!(!is_binary_operator("foo"), "identifier is not a binary operator");
    }

    #[test]
    fn test_is_postfix_operator_transpose() {
        assert!(is_postfix_operator("'"), "' is a postfix operator");
    }

    #[test]
    fn test_is_postfix_operator_empty_returns_false() {
        assert!(!is_postfix_operator(""), "empty string is not a postfix operator");
    }

    #[test]
    fn test_is_operator_delegates_to_all_three() {
        // Unary
        assert!(is_operator("√"), "√ should be recognized as operator");
        // Binary
        assert!(is_operator("=="), "== should be recognized as operator");
        // Postfix
        assert!(is_operator("'"), "' should be recognized as operator");
        // Not an operator
        assert!(!is_operator("foo"), "foo is not an operator");
    }
}
