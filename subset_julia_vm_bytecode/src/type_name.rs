//! Helpers for bytecode-level Julia type-name strings.

/// Parse type parameters from a parametric type string.
/// "Tuple{Int64, Float64}" -> ["Int64", "Float64"]
/// "Tuple{}" -> []
#[inline]
pub fn parse_parametric_params(type_str: &str) -> Vec<&str> {
    let start = match type_str.find('{') {
        Some(idx) => idx + 1,
        None => return vec![],
    };
    let end = match type_str.rfind('}') {
        Some(idx) => idx,
        None => return vec![],
    };
    let inner = &type_str[start..end];
    if inner.is_empty() {
        return vec![];
    }
    // Split by comma, respecting nested braces and value-parameter syntax.
    let mut result = Vec::new();
    let mut brace_depth = 0;
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut last_start = 0;
    for (i, c) in inner.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }

        match c {
            '\'' | '"' => quote = Some(c),
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                result.push(inner[last_start..i].trim());
                last_start = i + 1;
            }
            _ => {}
        }
    }
    result.push(inner[last_start..].trim());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_parametric_params_handles_simple_and_empty_inputs() {
        assert_eq!(
            parse_parametric_params("Tuple{Int64, Float64}"),
            vec!["Int64", "Float64"]
        );
        assert!(parse_parametric_params("Tuple{}").is_empty());
        assert!(parse_parametric_params("Int64").is_empty());
        assert!(parse_parametric_params("Tuple{Int64").is_empty());
    }

    #[test]
    fn parse_parametric_params_preserves_nested_value_params() {
        assert_eq!(parse_parametric_params("Val{(1, 2)}"), vec!["(1, 2)"]);
        assert_eq!(
            parse_parametric_params("Tuple{Val{(1, 2)}, Int64}"),
            vec!["Val{(1, 2)}", "Int64"]
        );
    }

    #[test]
    fn parse_parametric_params_respects_quotes_and_brackets() {
        assert_eq!(
            parse_parametric_params("Tuple{Val{\"a,b\"}, Vector{Int64}, NamedTuple{(:x,:y)}}"),
            vec!["Val{\"a,b\"}", "Vector{Int64}", "NamedTuple{(:x,:y)}"]
        );
    }
}
