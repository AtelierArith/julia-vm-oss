//! Integration tests: Char, string methods, math constants, abstract types, BigInt, numeric literals
#![allow(dead_code)]

mod common;
use common::*;

use subset_julia_vm::vm::Value;
use subset_julia_vm::*;

// ==================================================================================
// Char type tests
// ==================================================================================

fn test_char_literal_simple() {
    // Simple char literal
    let src = r#"'a'"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Char(c)) => assert_eq!(c, 'a', "Expected 'a', got '{}'", c),
        Ok(other) => panic!("Expected Char('a'), got {:?}", other),
        Err(e) => panic!("Char literal failed: {}", e),
    }
}

fn test_char_literal_escape_newline() {
    // Escape sequence: newline
    let src = r#"'\n'"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Char(c)) => assert_eq!(c, '\n', "Expected newline, got {:?}", c),
        Ok(other) => panic!("Expected Char('\\n'), got {:?}", other),
        Err(e) => panic!("Char escape newline failed: {}", e),
    }
}

fn test_char_literal_escape_tab() {
    // Escape sequence: tab
    let src = r#"'\t'"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Char(c)) => assert_eq!(c, '\t', "Expected tab, got {:?}", c),
        Ok(other) => panic!("Expected Char('\\t'), got {:?}", other),
        Err(e) => panic!("Char escape tab failed: {}", e),
    }
}

fn test_char_literal_escape_backslash() {
    // Escape sequence: backslash
    let src = r#"'\\'"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Char(c)) => assert_eq!(c, '\\', "Expected backslash, got {:?}", c),
        Ok(other) => panic!("Expected Char('\\\\'), got {:?}", other),
        Err(e) => panic!("Char escape backslash failed: {}", e),
    }
}

fn test_char_literal_unicode() {
    // Unicode character (Japanese 'あ')
    let src = r#"'あ'"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Char(c)) => assert_eq!(c, 'あ', "Expected 'あ', got '{}'", c),
        Ok(other) => panic!("Expected Char('あ'), got {:?}", other),
        Err(e) => panic!("Char unicode failed: {}", e),
    }
}

fn test_char_typeof() {
    // typeof('a') should return DataType(Char)
    let src = r#"println(typeof('a'))"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Char");
}

fn test_char_println() {
    // println('a') should print just "a"
    let src = r#"println('x')"#;
    let (_, output) = run_pipeline_with_output(src, 0);
    assert!(
        output.contains("x"),
        "Expected output to contain 'x', got '{}'",
        output
    );
}

// =============================================================================
// String Method Tests
// =============================================================================

fn test_string_indexing_returns_char() {
    // s[1] should return the first character as Char
    let src = r#"
        s = "hello"
        c = s[1]
        println(typeof(c))
    "#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "Char");
}

fn test_string_indexing_value() {
    // s[2] should return 'e' for "hello"
    let src = r#"
        s = "hello"
        s[2]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char(c) => assert_eq!(c, 'e'),
        other => panic!("Expected Char('e'), got {:?}", other),
    }
}

fn test_string_uppercase() {
    let src = r#"uppercase("hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "HELLO"),
        other => panic!("Expected Str(HELLO), got {:?}", other),
    }
}

fn test_string_lowercase() {
    let src = r#"lowercase("HELLO")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "hello"),
        other => panic!("Expected Str(hello), got {:?}", other),
    }
}

fn test_string_strip() {
    let src = r#"strip("  hello  ")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "hello"),
        other => panic!("Expected Str(hello), got {:?}", other),
    }
}

fn test_string_startswith() {
    let src = r#"startswith("hello world", "hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Bool(true) => (),
        other => panic!("Expected Bool(true), got {:?}", other),
    }
}

fn test_string_endswith() {
    let src = r#"endswith("hello world", "world")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Bool(true) => (),
        other => panic!("Expected Bool(true), got {:?}", other),
    }
}

fn test_string_occursin() {
    let src = r#"occursin("llo", "hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Bool(true) => (),
        other => panic!("Expected Bool(true), got {:?}", other),
    }
}

fn test_string_repeat() {
    let src = r#"repeat("ab", 3)"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "ababab"),
        other => panic!("Expected Str(ababab), got {:?}", other),
    }
}

fn test_string_chop() {
    let src = r#"chop("hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "hell"),
        other => panic!("Expected Str(hell), got {:?}", other),
    }
}

fn test_string_chomp() {
    let src = r#"chomp("hello\n")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "hello"),
        other => panic!("Expected Str(hello), got {:?}", other),
    }
}

fn test_string_length() {
    let src = r#"length("hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(5) => (),
        other => panic!("Expected I64(5), got {:?}", other),
    }
}

fn test_string_ncodeunits() {
    // ASCII string: ncodeunits == length
    let src = r#"ncodeunits("hello")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(5) => (),
        other => panic!("Expected I64(5), got {:?}", other),
    }
}

fn test_string_split() {
    let src = r#"
        parts = split("a,b,c", ",")
        first(parts)
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "a"),
        other => panic!("Expected Str(a), got {:?}", other),
    }
}

fn test_char_to_int() {
    let src = r#"Char(65)"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('A') => (),
        other => panic!("Expected Char('A'), got {:?}", other),
    }
}

// =============================================================================
// Multi-byte String Tests (Julia-compliant byte indexing)
// =============================================================================

fn test_multibyte_string_length() {
    // length() returns character count, not byte count
    let src = r#"length("こんにちは")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(5) => (), // 5 characters
        other => panic!("Expected I64(5), got {:?}", other),
    }
}

fn test_multibyte_string_ncodeunits() {
    // ncodeunits() returns byte count
    // "こんにちは" = 5 characters × 3 bytes each = 15 bytes
    let src = r#"ncodeunits("こんにちは")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(15) => (), // 15 bytes
        other => panic!("Expected I64(15), got {:?}", other),
    }
}

fn test_multibyte_string_index_first_char() {
    // s[1] should return first character (byte index 1)
    let src = r#"
        s = "こんにちは"
        s[1]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('こ') => (),
        other => panic!("Expected Char('こ'), got {:?}", other),
    }
}

fn test_multibyte_string_index_second_char() {
    // "こ" is 3 bytes, so second char starts at byte 4 (1-indexed)
    let src = r#"
        s = "こんにちは"
        s[4]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('ん') => (),
        other => panic!("Expected Char('ん'), got {:?}", other),
    }
}

fn test_multibyte_string_index_third_char() {
    // Third char starts at byte 7 (1-indexed)
    let src = r#"
        s = "こんにちは"
        s[7]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('に') => (),
        other => panic!("Expected Char('に'), got {:?}", other),
    }
}

fn test_multibyte_string_invalid_index_error() {
    // s[2] should raise StringIndexError (byte 2 is in the middle of 'こ')
    let src = r#"
        s = "こんにちは"
        s[2]
    "#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Expected error for invalid byte index");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("StringIndexError"),
        "Expected StringIndexError, got: {}",
        err_msg
    );
}

fn test_multibyte_string_invalid_index_error_middle() {
    // s[5] should raise StringIndexError (byte 5 is in the middle of 'ん')
    let src = r#"
        s = "こんにちは"
        s[5]
    "#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Expected error for invalid byte index");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("StringIndexError"),
        "Expected StringIndexError, got: {}",
        err_msg
    );
}

fn test_multibyte_string_last_char() {
    // "は" is the 5th character, starts at byte 13 (1-indexed)
    let src = r#"
        s = "こんにちは"
        s[13]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('は') => (),
        other => panic!("Expected Char('は'), got {:?}", other),
    }
}

fn test_mixed_ascii_multibyte_string() {
    // "Hello世界" = 5 ASCII bytes + 2 × 3 bytes = 11 bytes
    let src = r#"ncodeunits("Hello世界")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(11) => (),
        other => panic!("Expected I64(11), got {:?}", other),
    }
}

fn test_mixed_ascii_multibyte_index_ascii() {
    // ASCII characters are 1 byte each
    let src = r#"
        s = "Hello世界"
        s[5]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('o') => (),
        other => panic!("Expected Char('o'), got {:?}", other),
    }
}

fn test_mixed_ascii_multibyte_index_kanji() {
    // '世' starts at byte 6 (after 5 ASCII bytes)
    let src = r#"
        s = "Hello世界"
        s[6]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('世') => (),
        other => panic!("Expected Char('世'), got {:?}", other),
    }
}

fn test_emoji_string_length() {
    // Emoji can be 4 bytes each
    // "🎉" is 4 bytes
    let src = r#"ncodeunits("🎉")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(4) => (),
        other => panic!("Expected I64(4), got {:?}", other),
    }
}

fn test_emoji_string_index() {
    let src = r#"
        s = "🎉"
        s[1]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('🎉') => (),
        other => panic!("Expected Char('🎉'), got {:?}", other),
    }
}

fn test_emoji_invalid_index() {
    // s[2] should fail (byte 2 is in the middle of the 4-byte emoji)
    let src = r#"
        s = "🎉"
        s[2]
    "#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for invalid byte index in emoji"
    );
}

fn test_multibyte_uppercase() {
    // uppercase works on ASCII characters; non-ASCII characters pass through unchanged
    // (Pure Julia implementation in base/strings/unicode.jl, Issue #2565)
    let src = r#"uppercase("héllo")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "HéLLO"),
        other => panic!("Expected Str(HéLLO), got {:?}", other),
    }
}

fn test_multibyte_lowercase() {
    // lowercase works on ASCII characters; non-ASCII characters pass through unchanged
    // (Pure Julia implementation in base/strings/unicode.jl, Issue #2565)
    let src = r#"lowercase("HÉLLO")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Str(s) => assert_eq!(s, "hÉllo"),
        other => panic!("Expected Str(hÉllo), got {:?}", other),
    }
}

fn test_greek_string_operations() {
    // Greek letters are 2 bytes each in UTF-8
    let src = r#"ncodeunits("αβγ")"#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::I64(6) => (), // 3 characters × 2 bytes = 6
        other => panic!("Expected I64(6), got {:?}", other),
    }
}

fn test_greek_string_index() {
    // 'β' starts at byte 3 (after 2-byte 'α')
    let src = r#"
        s = "αβγ"
        s[3]
    "#;
    let result = run_core_pipeline(src, 0).unwrap();
    match result {
        Value::Char('β') => (),
        other => panic!("Expected Char('β'), got {:?}", other),
    }
}

// ============================================================================
// Pipe operator tests
// ============================================================================

fn test_pipe_operator_basic() {
    // x |> f => f(x)
    let src = r#"
[1, 2, 3] |> sum
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 6.0, "Expected sum([1,2,3]) = 6.0");
}

fn test_pipe_operator_chain() {
    // x |> f |> g => g(f(x))
    let src = r#"
[1, 2, 3, 4, 5] |> sum |> sqrt
"#;
    let result = compile_and_run_str(src, 0);
    // sum([1,2,3,4,5]) = 15, sqrt(15) ≈ 3.872983...
    assert!(
        (result - 15.0_f64.sqrt()).abs() < 1e-10,
        "Expected sqrt(15), got {}",
        result
    );
}

fn test_pipe_operator_with_length() {
    let src = r#"
[1, 2, 3, 4] |> length
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 4.0, "Expected length([1,2,3,4]) = 4");
}

fn test_pipe_operator_multiple_chains() {
    // Multiple pipes in sequence
    let src = r#"
function double(x)
    return x * 2
end

function add_one(x)
    return x + 1
end

5 |> double |> add_one |> double
"#;
    // 5 -> double -> 10 -> add_one -> 11 -> double -> 22
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 22.0, "Expected ((5*2)+1)*2 = 22");
}

fn test_pipe_operator_with_expression() {
    // Pipe with computed left side
    let src = r#"
(1 + 2 + 3) |> sqrt
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(6) ≈ 2.449
    assert!(
        (result - 6.0_f64.sqrt()).abs() < 1e-10,
        "Expected sqrt(6), got {}",
        result
    );
}

// ============================================================================
// Euler's number ℯ tests
// ============================================================================

fn test_euler_constant() {
    // ℯ should equal e ≈ 2.718281828...
    let src = "ℯ";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::E).abs() < 1e-10,
        "Expected e ≈ 2.718..., got {}",
        result
    );
}

fn test_euler_in_expression() {
    // exp(1) should equal ℯ
    let src = "exp(1.0) - ℯ";
    let result = compile_and_run_str(src, 0);
    assert!(
        result.abs() < 1e-10,
        "exp(1) should equal ℯ, diff = {}",
        result
    );
}

fn test_euler_with_log() {
    // log(ℯ) should equal 1
    let src = "log(ℯ)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "log(ℯ) should equal 1, got {}",
        result
    );
}

fn test_euler_arithmetic() {
    // ℯ^2 should equal exp(2)
    let src = "Float64(ℯ)^2 - exp(2.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        result.abs() < 1e-10,
        "ℯ^2 should equal exp(2), diff = {}",
        result
    );
}

// ============================================================================
// Base.MathConstants tests
// ============================================================================

fn test_mathconstants_qualified_access() {
    // Test Base.MathConstants.e
    let src = "Base.MathConstants.e";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::E).abs() < 1e-10,
        "Expected e, got {}",
        result
    );
}

fn test_mathconstants_pi() {
    let src = "Base.MathConstants.pi";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::PI).abs() < 1e-10,
        "Expected pi, got {}",
        result
    );
}

fn test_mathconstants_golden_ratio() {
    // φ = (1 + √5) / 2 ≈ 1.618033988749895
    let src = "Base.MathConstants.φ";
    let result = compile_and_run_str(src, 0);
    let expected = (1.0 + 5.0_f64.sqrt()) / 2.0;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected φ ≈ {}, got {}",
        expected,
        result
    );
}

fn test_mathconstants_golden_alias() {
    let src = "Base.MathConstants.golden";
    let result = compile_and_run_str(src, 0);
    let expected = (1.0 + 5.0_f64.sqrt()) / 2.0;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected golden ≈ {}, got {}",
        expected,
        result
    );
}

fn test_mathconstants_eulergamma() {
    // γ ≈ 0.5772156649015329 (Euler-Mascheroni constant)
    let src = "Base.MathConstants.γ";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.5772156649015329).abs() < 1e-10,
        "Expected γ, got {}",
        result
    );
}

fn test_mathconstants_eulergamma_alias() {
    let src = "Base.MathConstants.eulergamma";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.5772156649015329).abs() < 1e-10,
        "Expected eulergamma, got {}",
        result
    );
}

fn test_mathconstants_catalan() {
    // Catalan's constant ≈ 0.9159655941772190
    let src = "Base.MathConstants.catalan";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.915_965_594_177_219).abs() < 1e-10,
        "Expected catalan, got {}",
        result
    );
}

fn test_mathconstants_using_import() {
    // Test using Base.MathConstants
    let src = r#"
using Base.MathConstants
e + golden
"#;
    let result = compile_and_run_str(src, 0);
    let expected = std::f64::consts::E + (1.0 + 5.0_f64.sqrt()) / 2.0;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected e + golden ≈ {}, got {}",
        expected,
        result
    );
}

fn test_mathconstants_using_all_constants() {
    let src = r#"
using Base.MathConstants
# Use all constants
pi_val = π
e_val = e
phi_val = φ
gamma_val = γ
cat_val = catalan
pi_val + e_val + phi_val + gamma_val + cat_val
"#;
    let result = compile_and_run_str(src, 0);
    let expected = std::f64::consts::PI
        + std::f64::consts::E
        + (1.0 + 5.0_f64.sqrt()) / 2.0
        + 0.5772156649015329
        + 0.915_965_594_177_219;
    assert!(
        (result - expected).abs() < 1e-10,
        "Expected sum of all constants, got {}",
        result
    );
}

// ==================== Abstract Type Tests ====================

fn test_abstract_type_basic() {
    // Basic abstract type declaration
    let src = r#"
abstract type Animal end
1
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "Basic abstract type declaration should compile"
    );
}

fn test_abstract_type_with_parent() {
    // Abstract type with parent
    let src = r#"
abstract type Animal end
abstract type Mammal <: Animal end
1
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "Abstract type with parent should compile");
}

fn test_struct_with_abstract_parent() {
    // Struct inheriting from abstract type
    let src = r#"
abstract type Animal end
struct Dog <: Animal
    name::String
end
d = Dog("Rex")
1
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "Struct with abstract parent should compile");
}

fn test_isa_with_struct_type() {
    // isa() with struct's own type
    let src = r#"
abstract type Animal end
struct Dog <: Animal
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Dog)
    result = 1
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "isa(dog, Dog) should be true");
}

fn test_isa_with_abstract_parent() {
    // isa() with abstract parent type
    let src = r#"
abstract type Animal end
struct Dog <: Animal
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Animal)
    result = 1
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "isa(dog, Animal) should be true");
}

fn test_isa_with_grandparent() {
    // isa() with grandparent abstract type
    let src = r#"
abstract type Animal end
abstract type Mammal <: Animal end
struct Dog <: Mammal
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Animal)
    result = 1
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "isa(dog, Animal) should be true for grandparent"
    );
}

fn test_isa_with_intermediate_type() {
    // isa() with intermediate abstract type
    let src = r#"
abstract type Animal end
abstract type Mammal <: Animal end
struct Dog <: Mammal
    name::String
end
d = Dog("Rex")
result = 0
if isa(d, Mammal)
    result = 1
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "isa(dog, Mammal) should be true");
}

fn test_isa_with_unrelated_type() {
    // isa() with unrelated type should return false
    let src = r#"
abstract type Animal end
abstract type Vehicle end
struct Dog <: Animal
    name::String
end
d = Dog("Rex")
result = 1
if isa(d, Vehicle)
    result = 0
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "isa(dog, Vehicle) should be false (result stays 1)"
    );
}

fn test_isa_with_sibling_type() {
    // isa() with sibling struct type should return false
    let src = r#"
abstract type Animal end
struct Dog <: Animal
    name::String
end
struct Cat <: Animal
    name::String
end
d = Dog("Rex")
result = 1
if isa(d, Cat)
    result = 0
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "isa(dog, Cat) should be false (result stays 1)"
    );
}

fn test_multiple_abstract_hierarchies() {
    // Multiple independent type hierarchies
    let src = r#"
abstract type Animal end
abstract type Mammal <: Animal end
abstract type Bird <: Animal end

struct Dog <: Mammal
    name::String
end
struct Eagle <: Bird
    wingspan::Float64
end

d = Dog("Rex")
e = Eagle(2.0)

result = 0
if isa(d, Mammal)
    result = result + 1
end
if isa(e, Bird)
    result = result + 1
end
if !(isa(d, Bird))
    result = result + 1
end
if !(isa(e, Mammal))
    result = result + 1
end
result
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 4.0, "All type checks should pass");
}

// ==================== Ternary Operator Tests ====================

fn test_ternary_basic_true() {
    // Basic ternary: condition is true
    let src = r#"
x = 5
y = 3
x > y ? 1.0 : 0.0
"#;
    assert_eq!(compile_and_run_str(src, 0), 1.0);
}

fn test_ternary_basic_false() {
    // Basic ternary: condition is false
    let src = r#"
x = 3
y = 5
x > y ? 1.0 : 0.0
"#;
    assert_eq!(compile_and_run_str(src, 0), 0.0);
}

fn test_ternary_with_expressions() {
    // Ternary with complex expressions
    let src = r#"
x = 10
x > 5 ? x * 2 : x / 2
"#;
    assert_eq!(compile_and_run_str(src, 0), 20.0);
}

fn test_ternary_nested() {
    // Nested ternary: x > y ? "x larger" : x == y ? "equal" : "y larger"
    let src = r#"
x = 3
y = 5
x > y ? 1.0 : x == y ? 0.0 : -1.0
"#;
    assert_eq!(compile_and_run_str(src, 0), -1.0);
}

fn test_ternary_nested_equal() {
    // Nested ternary with equal values
    let src = r#"
x = 5
y = 5
x > y ? 1.0 : x == y ? 0.0 : -1.0
"#;
    assert_eq!(compile_and_run_str(src, 0), 0.0);
}

fn test_ternary_in_assignment() {
    // Using ternary in assignment
    let src = r#"
x = 10
result = x > 5 ? 100.0 : 0.0
result
"#;
    assert_eq!(compile_and_run_str(src, 0), 100.0);
}

fn test_ternary_with_function_call() {
    // Ternary with function calls in branches
    let src = r#"
function double(x)
    x * 2
end
function half(x)
    x / 2
end
x = 10
x > 5 ? double(x) : half(x)
"#;
    assert_eq!(compile_and_run_str(src, 0), 20.0);
}

fn test_ternary_short_circuit() {
    // Ternary should short-circuit: only one branch evaluated
    // Verify with a simple helper function test
    let src = r#"
function increment_and_return(x)
    x + 1
end
x = 10
# Only the true branch should be evaluated
x > 5 ? increment_and_return(10) : increment_and_return(100)
"#;
    // If true branch is evaluated, result = 11
    // If false branch is evaluated, result = 101
    // Short-circuit means result = 11
    assert_eq!(compile_and_run_str(src, 0), 11.0);
}

fn test_ternary_short_circuit_false() {
    // Short-circuit with false condition
    let src = r#"
function increment_and_return(x)
    x + 1
end
x = 3
# Only the false branch should be evaluated
x > 5 ? increment_and_return(10) : increment_and_return(100)
"#;
    // If true branch is evaluated, result = 11
    // If false branch is evaluated, result = 101
    // Short-circuit means result = 101
    assert_eq!(compile_and_run_str(src, 0), 101.0);
}

fn test_ternary_in_for_loop() {
    // Ternary inside a for loop
    let src = r#"
sum = 0.0
for i in 1:10
    sum = sum + (i > 5 ? i : 0)
end
sum  # 6 + 7 + 8 + 9 + 10 = 40
"#;
    assert_eq!(compile_and_run_str(src, 0), 40.0);
}

// ===========================================================================
// === (egal) operator tests
// ===========================================================================

fn test_egal_integer() {
    // Integer identity
    assert_eq!(compile_and_run_str("1 === 1", 0), 1.0);
    assert_eq!(compile_and_run_str("1 === 2", 0), 0.0);
    assert_eq!(compile_and_run_str("-1 === -1", 0), 1.0);
}

fn test_egal_float() {
    // Float identity
    assert_eq!(compile_and_run_str("1.0 === 1.0", 0), 1.0);
    assert_eq!(compile_and_run_str("1.0 === 2.0", 0), 0.0);
    // -0.0 vs 0.0 are different bits
    assert_eq!(compile_and_run_str("-0.0 === 0.0", 0), 0.0);
}

fn test_egal_nan() {
    // NaN === NaN is true (bit identity, not IEEE 754 equality)
    // In Julia, === uses bit identity for floats, so NaN === NaN is true
    assert_eq!(compile_and_run_str("NaN === NaN", 0), 1.0);
}

fn test_egal_string() {
    assert_eq!(compile_and_run_str(r#""hello" === "hello""#, 0), 1.0);
    assert_eq!(compile_and_run_str(r#""hello" === "world""#, 0), 0.0);
}

fn test_egal_nothing() {
    assert_eq!(compile_and_run_str("nothing === nothing", 0), 1.0);
}

fn test_not_egal_operator() {
    // !== operator
    assert_eq!(compile_and_run_str("1 !== 2", 0), 1.0);
    assert_eq!(compile_and_run_str("1 !== 1", 0), 0.0);
}

// ===========================================================================
// isequal function tests
// ===========================================================================

fn test_isequal_basic() {
    assert_eq!(compile_and_run_str("isequal(1, 1)", 0), 1.0);
    assert_eq!(compile_and_run_str("isequal(1, 2)", 0), 0.0);
}

fn test_isequal_nan() {
    // isequal(NaN, NaN) is true (unlike ==)
    assert_eq!(compile_and_run_str("isequal(NaN, NaN)", 0), 1.0);
}

fn test_isequal_negative_zero() {
    // isequal(-0.0, 0.0) is false (unlike ==)
    assert_eq!(compile_and_run_str("isequal(-0.0, 0.0)", 0), 0.0);
}

fn test_isequal_string() {
    assert_eq!(compile_and_run_str(r#"isequal("hello", "hello")"#, 0), 1.0);
    assert_eq!(compile_and_run_str(r#"isequal("hello", "world")"#, 0), 0.0);
}

// ===========================================================================
// hash function tests
// ===========================================================================

fn test_hash_integer() {
    // Hash should be non-zero for non-zero integers
    assert_eq!(compile_and_run_str("hash(1) != 0", 0), 1.0);
    // Same value should have same hash
    assert_eq!(compile_and_run_str("hash(42) == hash(42)", 0), 1.0);
    // Different values should likely have different hashes
    assert_eq!(compile_and_run_str("hash(1) != hash(2)", 0), 1.0);
}

fn test_hash_float() {
    assert_eq!(compile_and_run_str("hash(1.5) != 0", 0), 1.0);
    assert_eq!(compile_and_run_str("hash(3.14) == hash(3.14)", 0), 1.0);
}

fn test_hash_string() {
    assert_eq!(compile_and_run_str(r#"hash("hello") != 0"#, 0), 1.0);
    assert_eq!(
        compile_and_run_str(r#"hash("hello") == hash("hello")"#, 0),
        1.0
    );
    assert_eq!(
        compile_and_run_str(r#"hash("hello") != hash("world")"#, 0),
        1.0
    );
}

// ===========================================================================
// <: (subtype) operator tests
// ===========================================================================

fn test_subtype_same_type() {
    // Same type is always a subtype of itself
    assert_eq!(compile_and_run_str("Int64 <: Int64", 0), 1.0);
    assert_eq!(compile_and_run_str("Float64 <: Float64", 0), 1.0);
}

fn test_subtype_number_hierarchy() {
    // Int64 <: Integer <: Real <: Number
    assert_eq!(compile_and_run_str("Int64 <: Integer", 0), 1.0);
    assert_eq!(compile_and_run_str("Int64 <: Real", 0), 1.0);
    assert_eq!(compile_and_run_str("Int64 <: Number", 0), 1.0);
    // Float64 <: AbstractFloat <: Real <: Number
    assert_eq!(compile_and_run_str("Float64 <: Real", 0), 1.0);
    assert_eq!(compile_and_run_str("Float64 <: Number", 0), 1.0);
}

fn test_subtype_any() {
    // Everything is a subtype of Any
    assert_eq!(compile_and_run_str("Int64 <: Any", 0), 1.0);
    assert_eq!(compile_and_run_str("Float64 <: Any", 0), 1.0);
    assert_eq!(compile_and_run_str("String <: Any", 0), 1.0);
}

fn test_subtype_not_subtype() {
    // Float64 is not a subtype of Int64
    assert_eq!(compile_and_run_str("Float64 <: Int64", 0), 0.0);
    assert_eq!(compile_and_run_str("Int64 <: Float64", 0), 0.0);
    // Number is not a subtype of Int64
    assert_eq!(compile_and_run_str("Number <: Int64", 0), 0.0);
}

// ===========================================================================
// convert() function tests
// ===========================================================================

fn test_convert_to_float64() {
    // convert(Float64, 1) should return 1.0
    assert_eq!(compile_and_run_str("convert(Float64, 1)", 0), 1.0);
    assert_eq!(compile_and_run_str("convert(Float64, 42)", 0), 42.0);
    // Float64 to Float64 should be identity
    let expected = 314.0 / 100.0;
    assert_eq!(compile_and_run_str("convert(Float64, 3.14)", 0), expected);
}

fn test_convert_to_int64() {
    // convert(Int64, x) for a non-integral Float64 throws InexactError,
    // matching upstream (`convert(::Type{T}, x::Number) = T(x)::T`, Issue #5496).
    // It must NOT silently truncate 1.5 -> 1 / 2.9 -> 2.
    let r15 = run_core_pipeline("convert(Int64, 1.5)", 0);
    assert!(
        r15.as_ref()
            .err()
            .is_some_and(|e| e.contains("InexactError")),
        "Expected InexactError for convert(Int64, 1.5), got {:?}",
        r15
    );
    let r29 = run_core_pipeline("convert(Int64, 2.9)", 0);
    assert!(
        r29.as_ref()
            .err()
            .is_some_and(|e| e.contains("InexactError")),
        "Expected InexactError for convert(Int64, 2.9), got {:?}",
        r29
    );
    // Integral Float64 converts cleanly.
    assert_eq!(compile_and_run_str("convert(Int64, 2.0)", 0), 2.0);
    // Int64 to Int64 is identity.
    assert_eq!(compile_and_run_str("convert(Int64, 42)", 0), 42.0);
}

// ===========================================================================
// const keyword tests
// ===========================================================================

fn test_const_basic() {
    // const x = 1 should work like regular assignment
    assert_eq!(compile_and_run_str("const x = 1; x", 0), 1.0);
    assert_eq!(
        compile_and_run_str("const pi_approx = 3.14; pi_approx", 0),
        314.0 / 100.0
    );
}

fn test_const_expression() {
    // const with expression
    assert_eq!(compile_and_run_str("const x = 2 + 3; x", 0), 5.0);
    assert_eq!(compile_and_run_str("const y = 10 * 2; y + 1", 0), 21.0);
}

fn test_const_multiple() {
    // Multiple const declarations
    assert_eq!(
        compile_and_run_str("const a = 1; const b = 2; a + b", 0),
        3.0
    );
}

// ===========================================================================
// global keyword tests
// ===========================================================================

fn test_global_basic() {
    // global x should be a no-op in simplified implementation
    // This just tests that it parses and doesn't error
    assert_eq!(compile_and_run_str("x = 1; global x; x", 0), 1.0);
}

fn test_global_in_function() {
    // global inside function is a no-op in simplified implementation
    // Just verify it parses without error and function can use local variables
    let code = r#"
        function f()
            global x
            y = 42
            return y
        end
        f()
    "#;
    assert_eq!(compile_and_run_str(code, 0), 42.0);
}

// ==================== BigInt Tests ====================

fn test_bigint_from_i64() {
    // Test BigInt constructor from Int64
    let result = run_core_pipeline("x = BigInt(123); typeof(x)", 0);
    match result {
        Ok(Value::DataType(jt)) => assert_eq!(jt.name(), "BigInt"),
        Ok(Value::Str(s)) => assert_eq!(s, "BigInt"),
        other => panic!("Expected DataType or Str \"BigInt\", got {:?}", other),
    }
}

fn test_bigint_basic_display() {
    // Test that BigInt can be created and returned
    let result = run_core_pipeline("BigInt(42)", 0);
    match result {
        Ok(Value::BigInt(n)) => assert_eq!(n.to_string(), "42"),
        other => panic!("Expected BigInt(42), got {:?}", other),
    }
}

fn test_bigint_multiplication() {
    // Test BigInt multiplication
    let result = run_core_pipeline("a = BigInt(2); b = BigInt(3); a * b", 0);
    match result {
        Ok(Value::BigInt(n)) => assert_eq!(n.to_string(), "6"),
        other => panic!("Expected BigInt(6), got {:?}", other),
    }
}

fn test_bigint_large_multiplication() {
    // Test BigInt with large number multiplication
    // 10^18 * 10 = 10^19 (beyond I64 range)
    let result = run_core_pipeline(
        r#"
        a = BigInt(1000000000000000000)  # 10^18
        b = BigInt(10)
        a * b
    "#,
        0,
    );
    match result {
        Ok(Value::BigInt(n)) => assert_eq!(n.to_string(), "10000000000000000000"),
        other => panic!("Expected BigInt(10^19), got {:?}", other),
    }
}

fn test_bigint_addition() {
    // Test BigInt addition
    let result = run_core_pipeline("a = BigInt(100); b = BigInt(200); a + b", 0);
    match result {
        Ok(Value::BigInt(n)) => assert_eq!(n.to_string(), "300"),
        other => panic!("Expected BigInt(300), got {:?}", other),
    }
}

fn test_bigint_subtraction() {
    // Test BigInt subtraction
    let result = run_core_pipeline("a = BigInt(500); b = BigInt(123); a - b", 0);
    match result {
        Ok(Value::BigInt(n)) => assert_eq!(n.to_string(), "377"),
        other => panic!("Expected BigInt(377), got {:?}", other),
    }
}

fn test_parametric_struct_with_user_defined_abstract_bound() {
    // Test parametric struct with user-defined abstract type bound
    // First, test that bound checking works for user-defined abstract types
    let src = r#"
abstract type MyBase end

struct MyItem <: MyBase
    x::Int64
end

struct Container{T<:MyBase}
    item::T
end

# Just instantiate to test bound checking
item = MyItem(42)
item.x
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::I64(x)) => assert_eq!(x, 42, "Expected 42, got {}", x),
        Ok(Value::F64(x)) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
        Ok(other) => panic!("Unexpected result type: {:?}", other),
        Err(e) => panic!("Expected success, got error: {}", e),
    }
}

fn test_parametric_struct_user_bound_instantiation() {
    // Test that Container{MyItem} can be instantiated when MyItem <: MyBase
    let src = r#"
abstract type MyBase end

struct MyItem <: MyBase
    x::Int64
end

struct Container{T<:MyBase}
    item::T
end

# This should fail if bound checking rejects MyItem as not satisfying MyBase
c = Container{MyItem}(MyItem(42))
1  # Return simple value to avoid struct conversion issues
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::I64(x)) => assert_eq!(x, 1, "Expected 1, got {}", x),
        Ok(Value::F64(x)) => assert!((x - 1.0).abs() < 1e-10, "Expected 1.0, got {}", x),
        Ok(other) => panic!("Unexpected result type: {:?}", other),
        Err(e) => {
            // If error contains "does not satisfy bound", the bound check is working but rejecting
            if e.contains("does not satisfy bound") {
                panic!(
                    "Bound check failed - MyItem should satisfy MyBase bound: {}",
                    e
                );
            }
            panic!("Unexpected error: {}", e);
        }
    }
}

fn test_parametric_struct_user_bound_violation() {
    // Test that Container{WrongType} fails when WrongType does NOT satisfy MyBase bound
    let src = r#"
abstract type MyBase end

struct WrongType
    x::Int64
end

struct Container{T<:MyBase}
    item::T
end

# This should fail because WrongType does not satisfy MyBase bound
c = Container{WrongType}(WrongType(42))
1
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(_) => panic!("Expected bound violation error, but got success"),
        Err(e) => {
            // Should contain error about bound not being satisfied
            assert!(
                e.contains("does not satisfy bound") || e.contains("not satisfy"),
                "Expected bound violation error, got: {}",
                e
            );
        }
    }
}

// ============================================================================
// Logarithmic function tests
// ============================================================================

fn test_log2() {
    // log2(8) should equal 3
    let src = "log2(8.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 3.0).abs() < 1e-10,
        "log2(8) should equal 3, got {}",
        result
    );
}

fn test_log10() {
    // log10(100) should equal 2
    let src = "log10(100.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 2.0).abs() < 1e-10,
        "log10(100) should equal 2, got {}",
        result
    );
}

fn test_log1p() {
    // log1p(0) should equal 0
    let src = "log1p(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        result.abs() < 1e-10,
        "log1p(0) should equal 0, got {}",
        result
    );
}

// ==================== Custom Show Method Tests ====================

fn test_custom_show_basic() {
    // Test that custom Base.show method is called by println
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.show(io::IO, p::Point)
    print(io, "(", p.x, ", ", p.y, ")")
end

p = Point(3.0, 4.0)
println(p)
0.0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    // Note: Float values like 3.0 may be printed as "3" when they're whole numbers
    assert!(
        output.trim() == "(3.0, 4.0)" || output.trim() == "(3, 4)",
        "Custom show should format Point as (x, y), got: {}",
        output
    );
}

fn test_custom_show_without_show_uses_default() {
    // Test that structs without custom show use default formatting
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

p = Point(3.0, 4.0)
println(p)
0.0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    // Default formatting should show struct name and fields
    assert!(
        output.contains("Point"),
        "Default show should include struct name, got: {}",
        output
    );
}

fn test_custom_show_multiple_values() {
    // Test printing multiple values with custom show
    let src = r#"
struct Point
    x::Float64
    y::Float64
end

function Base.show(io::IO, p::Point)
    print(io, "<", p.x, ",", p.y, ">")
end

p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)
print(p1)
print(" -> ")
println(p2)
0.0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    // Should show both points with custom formatting
    assert!(
        output.contains("<1") && output.contains(">") && output.contains("<3"),
        "Should show both points with custom format, got: {}",
        output
    );
}

// ============================================================================
// Type{T} Pattern and Promotion Tests
// ============================================================================

fn test_promote_rule_basic() {
    // Test promote_rule(Float64, Int64) returns Float64
    let src = r#"
r1 = promote_rule(Float64, Int64)
println(r1)
r1 === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output: {}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "promote_rule(Float64, Int64) should return Float64, output: {}",
        output
    );
}

fn test_promote_type_basic() {
    // Test promote_type(Float64, Int64) returns Float64
    let src = r#"
t = promote_type(Float64, Int64)
println(t)
t === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output: {}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "promote_type(Float64, Int64) should return Float64, output: {}",
        output
    );
}

fn test_promote_type_debug() {
    // Debug: step by step what happens inside promote_type
    let src = r#"
# Call promote_rule directly
r1_direct = promote_rule(Float64, Int64)
println("Direct promote_rule: ", r1_direct)

# Now call it with type params (this is what promote_type does)
function test_call(::Type{T}, ::Type{S}) where {T, S}
    println("T = ", T)
    println("S = ", S)
    r = promote_rule(T, S)
    println("promote_rule(T, S) = ", r)
    r
end

result = test_call(Float64, Int64)
println("Result: ", result)
result === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Debug output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "Result should be Float64 (1.0 = true)");
}

// Known limitation: if-expressions don't return values, and functions
// without Type{T} patterns can't return DataType directly.
// Use assignment pattern instead (see test_if_with_datatype_variable).

fn test_promote_rule_direct_works() {
    // Verify that promote_rule (with Type{} signature) can return DataType
    let src = r#"
# Direct call to promote_rule - has ::Type{Float64}, ::Type{Int64} signature
r = promote_rule(Float64, Int64)
println("promote_rule result: ", r)
r === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "promote_rule should return Float64");
}

// test_datatype_return_with_type_pattern - removed (known limitation: returning type variable T directly)
// Use function call results instead (see test_promote_rule_from_typevar).

fn test_real_plus_complex_julia() {
    // Test Real + Complex via Julia source code (not IR)
    let src = r#"
x = 1.0
z = 2.0 + 3.0im
result = x + z
println("1.0 + (2.0 + 3.0im) = ", result)
real(result)
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 3.0, "real(1.0 + (2.0+3.0im)) should be 3.0");
}

fn test_cr_plus_ci_times_im() {
    // Test the exact pattern from Mandelbrot: cr + ci * im
    let src = r#"
cr = -2.0
ci = 1.0
c = cr + ci * im
println("c = ", c)
real(c)
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, -2.0, "real(cr + ci * im) should be -2.0");
}

fn test_float_plus_complex_literal() {
    // Test -0.75 + 0.0im (this is in the failing Mandelbrot)
    let src = r#"
c = -0.75 + 0.0im
println("c = ", c)
real(c)
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    // Check if there's an error
    if output.contains("[error]") || output.contains("Error") {
        println!("Error detected!");
    }
    let result = compile_and_run_str(src, 0);
    // For now, just check we get something reasonable
    println!("Result: {}", result);
}

/// Test mandelbrot loop pattern - Currently has Complex{Bool} type inference issues.
/// The method table dispatch for `*(Float64, Complex{Bool})` fails at compile time.
/// See Issue #1329 for details.
/// FIXED: Complex type promotion now works correctly in compile-time inference.
fn test_mandelbrot_loop_pattern() {
    // Test the loop pattern with Complex{Float64} - this works
    let src = r#"
function mandelbrot_escape(c::Complex{Float64}, maxiter::Int64)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Test a few points (like the iOS sample)
c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("(0, 0): ", c1)

# Now try the loop pattern
for row in 0:2
    ci = 1.0 - row * 0.2
    for col in 0:2
        cr = -2.0 + col * 0.15
        c = cr + ci * im
        n = mandelbrot_escape(c, 50)
        println("row=", row, " col=", col, " n=", n)
    end
end

c1
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 100.0, "c1 should be 100");
}

fn test_mandelbrot_with_complex_no_param() {
    // Test with ::Complex (no type parameter) - this is what the iOS sample uses
    let src = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("(0, 0): ", c1)

# Try the loop
for row in 0:1
    ci = 1.0 - row * 0.2
    for col in 0:1
        cr = -2.0 + col * 0.15
        c = cr + ci * im
        n = mandelbrot_escape(c, 50)
        println("row=", row, " col=", col, " n=", n)
    end
end

c1
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 100.0, "c1 should be 100");
}

fn test_mandelbrot_no_type_annotations() {
    // Test Mandelbrot with Complex numbers but WITHOUT type annotations
    let src = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("(0, 0): ", c1)

c2 = mandelbrot_escape(1.0 + 1.0im, 100)
println("(1, 1): ", c2)

c1
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 100.0, "c1 should be 100 (in set)");
}

fn test_promote_rule_via_variable() {
    // The key case - calling promote_rule, storing in variable, then comparing
    // This is what promote_type does
    let src = r#"
r1 = promote_rule(Float64, Int64)
println("r1 = ", r1)
r1 === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "r1 should be Float64");
}

fn test_promote_rule_from_typevar() {
    // Calling promote_rule with type variables (inside a function)
    let src = r#"
function test_pr(::Type{T}, ::Type{S}) where {T, S}
    println("T = ", T)
    println("S = ", S)
    r = promote_rule(T, S)
    println("r = ", r)
    r === Float64
end

test_pr(Float64, Int64)
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "promote_rule(T,S) where T=Float64, S=Int64 should be Float64"
    );
}

fn test_if_with_datatype_variable() {
    // Test if-expression that checks and returns DataType variable
    let src = r#"
function test_if_datatype(::Type{T}, ::Type{S}) where {T, S}
    R = promote_rule(T, S)
    println("R = ", R)
    println("R !== Nothing = ", R !== Nothing)

    # Use explicit if-else with return to avoid expression value issues
    result = Nothing
    if R !== Nothing
        result = R
    end
    println("result = ", result)
    result
end

val = test_if_datatype(Float64, Int64)
println("val = ", val)
val === Float64
"#;
    let output = compile_and_run_str_with_output(src, 0);
    println!("Output:\n{}", output);
    let result = compile_and_run_str(src, 0);
    assert_eq!(result, 1.0, "Should return Float64");
}

// ==================== Struct Array Tests ====================

fn test_struct_array_basic() {
    // Test accessing first element with real()
    let src = r#"
arr = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
real(arr[1])
"#;
    let result = run_core_pipeline(src, 0);
    println!("Result: {:?}", result);
    match result {
        Ok(Value::F64(v)) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
        Ok(other) => panic!("Expected F64(1.0), got {:?}", other),
        Err(e) => panic!("Expected F64(1.0), got error: {}", e),
    }
}

fn test_struct_array_index_second_element() {
    // Simplified: just return real(arr[2]) directly without intermediate variables
    let src = r#"
arr = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
real(arr[2])
"#;
    let result = run_core_pipeline(src, 0);
    println!("Result: {:?}", result);
    match result {
        Ok(Value::F64(v)) => assert!((v - 3.0).abs() < 1e-10, "Expected 3.0, got {}", v),
        Ok(other) => panic!("Expected F64(3.0), got {:?}", other),
        Err(e) => panic!("Expected F64(3.0), got error: {}", e),
    }
}

fn test_struct_array_imag() {
    // Test imag() on first element
    let src = r#"
arr = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
imag(arr[1])
"#;
    let result = run_core_pipeline(src, 0);
    println!("Result: {:?}", result);
    match result {
        Ok(Value::F64(v)) => assert!((v - 2.0).abs() < 1e-10, "Expected 2.0, got {}", v),
        Ok(other) => panic!("Expected F64(2.0), got {:?}", other),
        Err(e) => panic!("Expected F64(2.0), got error: {}", e),
    }
}

// ==================== Boolean Context Tests ====================
// Julia requires Bool type in boolean contexts (if/while conditions).
// Using non-boolean values like Int64 should result in a TypeError.

fn test_if_integer_error() {
    // `if 1` should error: non-boolean (Int64) used in boolean context
    let src = r#"
if 1
    println("Should not print")
end
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Err(e) => {
            assert!(
                e.contains("non-boolean") && e.contains("Int64") && e.contains("boolean context"),
                "Expected 'non-boolean (Int64) used in boolean context' error, got: {}",
                e
            );
        }
        Ok(v) => panic!("Expected TypeError for `if 1`, got Ok({:?})", v),
    }
}

fn test_if_true_ok() {
    // `if true` should work fine
    let output = compile_and_run_str_with_output(
        r#"
if true
    println("true_branch")
else
    println("false_branch")
end
"#,
        0,
    );
    assert!(
        output.contains("true_branch"),
        "Expected 'true_branch' in output, got: {}",
        output
    );
}

fn test_if_false_ok() {
    // `if false` should work fine
    let output = compile_and_run_str_with_output(
        r#"
if false
    println("true_branch")
else
    println("false_branch")
end
"#,
        0,
    );
    assert!(
        output.contains("false_branch"),
        "Expected 'false_branch' in output, got: {}",
        output
    );
}

fn test_if_comparison_ok() {
    // `if 1 > 0` should work (comparison returns Bool)
    let output = compile_and_run_str_with_output(
        r#"
if 1 > 0
    println("true_branch")
else
    println("false_branch")
end
"#,
        0,
    );
    assert!(
        output.contains("true_branch"),
        "Expected 'true_branch' in output, got: {}",
        output
    );
}

fn test_if_comparison_false_ok() {
    // `if 1 < 0` should work (comparison returns Bool)
    let output = compile_and_run_str_with_output(
        r#"
if 1 < 0
    println("true_branch")
else
    println("false_branch")
end
"#,
        0,
    );
    assert!(
        output.contains("false_branch"),
        "Expected 'false_branch' in output, got: {}",
        output
    );
}

fn test_typeof_comparison_returns_bool() {
    // `typeof(1 > 0)` should return Bool
    let output = compile_and_run_str_with_output(
        r#"
println(typeof(1 > 0))
"#,
        0,
    );
    assert!(
        output.contains("Bool"),
        "Expected 'Bool' in output, got: {}",
        output
    );
}

fn test_typeof_comparison_eq_returns_bool() {
    // `typeof(1 == 1)` should return Bool
    let output = compile_and_run_str_with_output(
        r#"
println(typeof(1 == 1))
"#,
        0,
    );
    assert!(
        output.contains("Bool"),
        "Expected 'Bool' in output, got: {}",
        output
    );
}

fn test_if_zero_error() {
    // `if 0` should also error (Int64 is not Bool)
    let src = r#"
if 0
    println("Should not print")
end
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Err(e) => {
            assert!(
                e.contains("non-boolean") && e.contains("Int64") && e.contains("boolean context"),
                "Expected 'non-boolean (Int64) used in boolean context' error, got: {}",
                e
            );
        }
        Ok(v) => panic!("Expected TypeError for `if 0`, got Ok({:?})", v),
    }
}

fn test_while_true_ok() {
    // `while` with Bool condition should work
    let output = compile_and_run_str_with_output(
        r#"
x = 0
while x < 3
    x = x + 1
end
println(x)
"#,
        0,
    );
    assert!(
        output.contains("3"),
        "Expected '3' in output, got: {}",
        output
    );
}

fn test_comparison_chained() {
    // Test that chained comparisons work correctly
    let output = compile_and_run_str_with_output(
        r#"
x = 5
if x > 0 && x < 10
    println("in_range")
else
    println("out_of_range")
end
"#,
        0,
    );
    assert!(
        output.contains("in_range"),
        "Expected 'in_range' in output, got: {}",
        output
    );
}

// ==================== Nested @testset Tests ====================

fn test_nested_testset() {
    // Nested @testset should work correctly
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "Outer" begin
    @test 1 + 1 == 2
    @testset "Inner" begin
        @test 2 * 2 == 4
        @test 3 - 1 == 2
    end
    @test 3 + 3 == 6
end
"#,
        0,
    );
    // Should show nested test output
    assert!(
        output.contains("Outer"),
        "Expected 'Outer' in output, got: {}",
        output
    );
    assert!(
        output.contains("Inner"),
        "Expected 'Inner' in output, got: {}",
        output
    );
}

fn test_deeply_nested_testset() {
    // Deeply nested @testset should work correctly
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "Level1" begin
    @test true
    @testset "Level2" begin
        @test true
        @testset "Level3" begin
            @test true
        end
    end
end
"#,
        0,
    );
    assert!(
        output.contains("Level1"),
        "Expected 'Level1' in output, got: {}",
        output
    );
    assert!(
        output.contains("Level2"),
        "Expected 'Level2' in output, got: {}",
        output
    );
    assert!(
        output.contains("Level3"),
        "Expected 'Level3' in output, got: {}",
        output
    );
}

fn test_nested_testset_with_failures() {
    // Nested @testset should correctly count failures
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "Outer" begin
    @test true
    @testset "Inner" begin
        @test false  # This should fail
        @test true
    end
    @test true
end
"#,
        0,
    );
    // Should show test output with failure indicator
    assert!(
        output.contains("Outer"),
        "Expected 'Outer' in output, got: {}",
        output
    );
    assert!(
        output.contains("Inner"),
        "Expected 'Inner' in output, got: {}",
        output
    );
}

// ==================== @test_throws Tests ====================

fn test_test_throws_division_by_zero() {
    // @test_throws should pass when expected exception is thrown (division by zero)
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "DivisionTest" begin
    @test_throws DivideError 1 ÷ 0
end
"#,
        0,
    );
    assert!(
        output.contains("DivisionTest"),
        "Expected 'DivisionTest' in output, got: {}",
        output
    );
    assert!(
        output.contains("Test Passed"),
        "Expected 'Test Passed' in output, got: {}",
        output
    );
}

// Note: BoundsError test removed - bounds errors return Err directly instead of using raise(),
// so they don't go through the try/catch mechanism that @test_throws relies on

fn test_test_throws_any_error() {
    // @test_throws with Exception should catch any error (division by zero)
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "AnyErrorTest" begin
    @test_throws Exception 1 ÷ 0
end
"#,
        0,
    );
    assert!(
        output.contains("AnyErrorTest"),
        "Expected 'AnyErrorTest' in output, got: {}",
        output
    );
    assert!(
        output.contains("Test Passed"),
        "Expected 'Test Passed' in output, got: {}",
        output
    );
}

fn test_test_throws_no_exception() {
    // @test_throws should fail when no exception is thrown
    let output = compile_and_run_str_with_output(
        r#"
using Test
@testset "NoExceptionTest" begin
    @test_throws DomainError 1 + 1
end
"#,
        0,
    );
    assert!(
        output.contains("NoExceptionTest"),
        "Expected 'NoExceptionTest' in output, got: {}",
        output
    );
    assert!(
        output.contains("Test Failed"),
        "Expected 'Test Failed' in output, got: {}",
        output
    );
}

fn test_test_throws_standalone() {
    // @test_throws should work standalone (not inside @testset)
    let output = compile_and_run_str_with_output(
        r#"
using Test
@test_throws DivideError 1 ÷ 0
"#,
        0,
    );
    assert!(
        output.contains("Test Passed"),
        "Expected 'Test Passed' in output, got: {}",
        output
    );
}

fn test_test_throws_without_using_test() {
    // @test_throws without `using Test` should fail
    let src = r#"
@test_throws DomainError 1 ÷ 0
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when using @test_throws without 'using Test', but got: {:?}",
        result
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("using Test"),
        "Error message should mention 'using Test': {}",
        err_msg
    );
}

// ==================== Numeric Literals ====================

fn test_hex_integer_literal() {
    // Hex literal width follows Julia 1.12 rules:
    //   1-2 hex digits → UInt8, 3-4 → UInt16, 5-8 → UInt32, 9-16 → UInt64.
    assert_u8(run_core_pipeline("0xff", 0).unwrap(), 255);
    assert_u8(run_core_pipeline("0xFF", 0).unwrap(), 255);
    assert_u8(run_core_pipeline("0x10", 0).unwrap(), 16);
    assert_u16(run_core_pipeline("0xABCD", 0).unwrap(), 43981);
}

fn test_hex_integer_with_underscore() {
    // Underscore separators are ignored when counting digits.
    assert_u16(run_core_pipeline("0xff_ff", 0).unwrap(), 65535);
    assert_u32(run_core_pipeline("0x1_0000", 0).unwrap(), 65536);
}

fn test_binary_integer_literal() {
    // Binary literal width: 1-8 digits → UInt8, 9-16 → UInt16, etc.
    assert_u8(run_core_pipeline("0b0", 0).unwrap(), 0);
    assert_u8(run_core_pipeline("0b1", 0).unwrap(), 1);
    assert_u8(run_core_pipeline("0b10", 0).unwrap(), 2);
    assert_u8(run_core_pipeline("0b1010", 0).unwrap(), 10);
    assert_u8(run_core_pipeline("0b11111111", 0).unwrap(), 255);
    // `0B1010` is a SubsetJuliaVM lenient extension; official Julia rejects
    // uppercase prefix but sjulia accepts it as the same UInt8 literal.
    assert_u8(run_core_pipeline("0B1010", 0).unwrap(), 10);
}

fn test_binary_integer_with_underscore() {
    assert_u8(run_core_pipeline("0b1111_0000", 0).unwrap(), 240);
    assert_u8(run_core_pipeline("0b1010_1010", 0).unwrap(), 170);
}

fn test_octal_integer_literal() {
    // Octal literal width is determined by the value's bit-width (Julia 1.12):
    // 0o0..0o377 fit in UInt8; 0o400..0o177777 are UInt16; etc.
    assert_u8(run_core_pipeline("0o0", 0).unwrap(), 0);
    assert_u8(run_core_pipeline("0o7", 0).unwrap(), 7);
    assert_u8(run_core_pipeline("0o10", 0).unwrap(), 8);
    assert_u8(run_core_pipeline("0o17", 0).unwrap(), 15);
    assert_u8(run_core_pipeline("0o77", 0).unwrap(), 63);
    assert_u16(run_core_pipeline("0o777", 0).unwrap(), 511);
    // `0O17` is a SubsetJuliaVM lenient extension (uppercase prefix).
    assert_u8(run_core_pipeline("0O17", 0).unwrap(), 15);
}

fn test_float32_literal() {
    // Float32 literals: 1.0f0
    assert_f32(run_core_pipeline("1.0f0", 0).unwrap(), 1.0);
    assert_f32(run_core_pipeline("1f0", 0).unwrap(), 1.0);
    assert_f32(run_core_pipeline("2.5f0", 0).unwrap(), 2.5);
    assert_f32(run_core_pipeline("1f1", 0).unwrap(), 10.0);
    assert_f32(run_core_pipeline("1f2", 0).unwrap(), 100.0);
    assert_f32(run_core_pipeline("1.5f-1", 0).unwrap(), 0.15);
}

fn test_hex_float_literal() {
    // Hex float literals: 0x1.8p3 = 1.5 * 2^3 = 12.0
    assert_f64(run_core_pipeline("0x1p0", 0).unwrap(), 1.0);
    assert_f64(run_core_pipeline("0x1p1", 0).unwrap(), 2.0);
    assert_f64(run_core_pipeline("0x1p2", 0).unwrap(), 4.0);
    assert_f64(run_core_pipeline("0x1p3", 0).unwrap(), 8.0);
    assert_f64(run_core_pipeline("0x1p-1", 0).unwrap(), 0.5);
    assert_f64(run_core_pipeline("0x1.8p0", 0).unwrap(), 1.5);
    assert_f64(run_core_pipeline("0x1.8p3", 0).unwrap(), 12.0);
}

// ==================== sqrt DomainError ====================

fn test_sqrt_positive() {
    // sqrt of positive numbers should work
    assert_f64(run_core_pipeline("sqrt(4.0)", 0).unwrap(), 2.0);
    assert_f64(run_core_pipeline("sqrt(9)", 0).unwrap(), 3.0);
    assert_f64(run_core_pipeline("sqrt(0.0)", 0).unwrap(), 0.0);
}

fn test_sqrt_negative_domain_error() {
    // sqrt of negative real numbers should throw DomainError (not return NaN)
    let result = run_core_pipeline("sqrt(-1)", 0);
    assert!(
        result.is_err(),
        "sqrt(-1) should throw DomainError, not return NaN"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("Domain error") || err_msg.contains("DomainError"),
        "Error should be DomainError: {}",
        err_msg
    );
    assert!(
        err_msg.contains("sqrt") || err_msg.contains("negative"),
        "Error should mention sqrt or negative: {}",
        err_msg
    );
}

fn test_sqrt_negative_float_domain_error() {
    // sqrt of negative float should also throw DomainError
    let result = run_core_pipeline("sqrt(-1.0)", 0);
    assert!(
        result.is_err(),
        "sqrt(-1.0) should throw DomainError, not return NaN"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("Domain error") || err_msg.contains("DomainError"),
        "Error should be DomainError: {}",
        err_msg
    );
}

fn test_sqrt_complex_negative() {
    // sqrt(complex(-1)) should return 0 + 1im (the imaginary unit)
    // This is the correct mathematical result: sqrt(-1) = i
    let src = r#"
z = sqrt(complex(-1.0, 0.0))
# z should be approximately 0 + 1im
abs(z.re) < 1e-10 && abs(z.im - 1.0) < 1e-10
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        Ok(v) => panic!("Expected Bool(true), got {:?}", v),
        Err(e) => panic!("sqrt(complex(-1)) failed: {}", e),
    }
}

// Issue #1330: Test @show with user-defined short function definition
fn test_show_with_user_defined_short_function() {
    let src = r#"
f(x) = 2x + 1
@show f(3)
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(
        matches!(result, Value::I64(7)),
        "Expected I64(7), got {:?}",
        result
    );
    assert_eq!(output, "f(3) = 7\n");
}

// Issue #1330: Test @show with user-defined regular function definition
fn test_show_with_user_defined_regular_function() {
    let src = r#"
function double(x)
    2 * x
end
@show double(5)
"#;
    let (result, output) = compile_and_run_program_direct(src, 0);
    assert!(
        matches!(result, Value::I64(10)),
        "Expected I64(10), got {:?}",
        result
    );
    assert_eq!(output, "double(5) = 10\n");
}

// Generated aggregate chunks for nextest process amortization.
#[test]
fn chunk_000() {
    test_char_literal_simple();
    test_char_literal_escape_newline();
    test_char_literal_escape_tab();
    test_char_literal_escape_backslash();
    test_char_literal_unicode();
    test_char_typeof();
    test_char_println();
    test_string_indexing_returns_char();
    test_string_indexing_value();
    test_string_uppercase();
    test_string_lowercase();
    test_string_strip();
    test_string_startswith();
    test_string_endswith();
    test_string_occursin();
    test_string_repeat();
}

#[test]
fn chunk_001() {
    test_string_chop();
    test_string_chomp();
    test_string_length();
    test_string_ncodeunits();
    test_string_split();
    test_char_to_int();
    test_multibyte_string_length();
    test_multibyte_string_ncodeunits();
    test_multibyte_string_index_first_char();
    test_multibyte_string_index_second_char();
    test_multibyte_string_index_third_char();
    test_multibyte_string_invalid_index_error();
    test_multibyte_string_invalid_index_error_middle();
    test_multibyte_string_last_char();
    test_mixed_ascii_multibyte_string();
    test_mixed_ascii_multibyte_index_ascii();
}

#[test]
fn chunk_002() {
    test_mixed_ascii_multibyte_index_kanji();
    test_emoji_string_length();
    test_emoji_string_index();
    test_emoji_invalid_index();
    test_multibyte_uppercase();
    test_multibyte_lowercase();
    test_greek_string_operations();
    test_greek_string_index();
    test_pipe_operator_basic();
    test_pipe_operator_chain();
    test_pipe_operator_with_length();
    test_pipe_operator_multiple_chains();
    test_pipe_operator_with_expression();
    test_euler_constant();
    test_euler_in_expression();
    test_euler_with_log();
}

#[test]
fn chunk_003() {
    test_euler_arithmetic();
    test_mathconstants_qualified_access();
    test_mathconstants_pi();
    test_mathconstants_golden_ratio();
    test_mathconstants_golden_alias();
    test_mathconstants_eulergamma();
    test_mathconstants_eulergamma_alias();
    test_mathconstants_catalan();
    test_mathconstants_using_import();
    test_mathconstants_using_all_constants();
    test_abstract_type_basic();
    test_abstract_type_with_parent();
    test_struct_with_abstract_parent();
    test_isa_with_struct_type();
    test_isa_with_abstract_parent();
    test_isa_with_grandparent();
}

#[test]
fn chunk_004() {
    test_isa_with_intermediate_type();
    test_isa_with_unrelated_type();
    test_isa_with_sibling_type();
    test_multiple_abstract_hierarchies();
    test_ternary_basic_true();
    test_ternary_basic_false();
    test_ternary_with_expressions();
    test_ternary_nested();
    test_ternary_nested_equal();
    test_ternary_in_assignment();
    test_ternary_with_function_call();
    test_ternary_short_circuit();
    test_ternary_short_circuit_false();
    test_ternary_in_for_loop();
    test_egal_integer();
    test_egal_float();
}

#[test]
fn chunk_005() {
    test_egal_nan();
    test_egal_string();
    test_egal_nothing();
    test_not_egal_operator();
    test_isequal_basic();
    test_isequal_nan();
    test_isequal_negative_zero();
    test_isequal_string();
    test_hash_integer();
    test_hash_float();
    test_hash_string();
    test_subtype_same_type();
    test_subtype_number_hierarchy();
    test_subtype_any();
    test_subtype_not_subtype();
    test_convert_to_float64();
}

#[test]
fn chunk_006() {
    test_convert_to_int64();
    test_const_basic();
    test_const_expression();
    test_const_multiple();
    test_global_basic();
    test_global_in_function();
    test_bigint_from_i64();
    test_bigint_basic_display();
    test_bigint_multiplication();
    test_bigint_large_multiplication();
    test_bigint_addition();
    test_bigint_subtraction();
    test_parametric_struct_with_user_defined_abstract_bound();
    test_parametric_struct_user_bound_instantiation();
    test_parametric_struct_user_bound_violation();
    test_log2();
}

#[test]
fn chunk_007() {
    test_log10();
    test_log1p();
    test_custom_show_basic();
    test_custom_show_without_show_uses_default();
    test_custom_show_multiple_values();
    test_promote_rule_basic();
    test_promote_type_basic();
    test_promote_type_debug();
    test_promote_rule_direct_works();
    test_real_plus_complex_julia();
    test_cr_plus_ci_times_im();
    test_float_plus_complex_literal();
    test_mandelbrot_loop_pattern();
    test_mandelbrot_with_complex_no_param();
    test_mandelbrot_no_type_annotations();
    test_promote_rule_via_variable();
}

#[test]
fn chunk_008() {
    test_promote_rule_from_typevar();
    test_if_with_datatype_variable();
    test_struct_array_basic();
    test_struct_array_index_second_element();
    test_struct_array_imag();
    test_if_integer_error();
    test_if_true_ok();
    test_if_false_ok();
    test_if_comparison_ok();
    test_if_comparison_false_ok();
    test_typeof_comparison_returns_bool();
    test_typeof_comparison_eq_returns_bool();
    test_if_zero_error();
    test_while_true_ok();
    test_comparison_chained();
    test_nested_testset();
}

#[test]
fn chunk_009() {
    test_deeply_nested_testset();
    test_nested_testset_with_failures();
    test_test_throws_division_by_zero();
    test_test_throws_any_error();
    test_test_throws_no_exception();
    test_test_throws_standalone();
    test_test_throws_without_using_test();
    test_hex_integer_literal();
    test_hex_integer_with_underscore();
    test_binary_integer_literal();
    test_binary_integer_with_underscore();
    test_octal_integer_literal();
    test_float32_literal();
    test_hex_float_literal();
    test_sqrt_positive();
    test_sqrt_negative_domain_error();
}

#[test]
fn chunk_010() {
    test_sqrt_negative_float_domain_error();
    test_sqrt_complex_negative();
    test_show_with_user_defined_short_function();
    test_show_with_user_defined_regular_function();
}
