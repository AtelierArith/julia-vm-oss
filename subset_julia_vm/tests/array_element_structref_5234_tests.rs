//! Issue #5234: bare stdout `print` / `println` of an array whose ELEMENTS are
//! heap-allocated structs (Pair, Complex, user struct) must resolve each
//! `Value::StructRef(idx)` through the element's show form, not leak the Rust
//! debug `StructRef(heap_idx=N)` repr.
//!
//! These tests capture the VM's stdout (`get_output()`) so they exercise the
//! exact entry points the bug reproduced on — `BuiltinId::Print` /
//! `BuiltinId::Println` and the `PrintAny` / `PrintAnyNoNewline` instructions —
//! which the `string` / `repr` fixture path does NOT reach (those go through
//! `ToString` / `Repr`, which already deep-resolved). The fixture
//! `io::array_element_structref_5234` covers the string/repr/sprint side; this
//! Rust test closes the remaining bare-stdout gap that the #4766 matrix
//! fixtures (which only used `print(buf, x)` / `sprint`) never hit.

mod common;
use common::compile_and_run_str_with_output;

/// No display path may leak the Rust debug `StructRef(...)` / `heap_idx=`
/// tokens into user-visible output.
fn assert_no_structref_leak(output: &str, ctx: &str) {
    assert!(
        !output.contains("StructRef") && !output.contains("heap_idx"),
        "{ctx}: leaked StructRef debug repr into stdout output: {output:?}"
    );
}

#[test]
fn println_array_of_pair_literal_no_structref_leak() {
    let output = compile_and_run_str_with_output("println([1 => 1, 2 => 4])\n0\n", 0);
    assert_no_structref_leak(&output, "println([Pair...])");
    assert!(
        output.contains("[1 => 1, 2 => 4]"),
        "expected `[1 => 1, 2 => 4]`, got: {output:?}"
    );
}

#[test]
fn print_array_of_pair_literal_no_structref_leak() {
    let output = compile_and_run_str_with_output("print([1 => 1, 2 => 4])\n0\n", 0);
    assert_no_structref_leak(&output, "print([Pair...])");
    assert!(
        output.contains("[1 => 1, 2 => 4]"),
        "expected `[1 => 1, 2 => 4]`, got: {output:?}"
    );
}

#[test]
fn println_array_of_complex_literal_no_structref_leak() {
    let output = compile_and_run_str_with_output("println([complex(1, 1), complex(2, 2)])\n0\n", 0);
    assert_no_structref_leak(&output, "println([Complex...])");
    assert!(
        output.contains("1 + 1im") && output.contains("2 + 2im"),
        "expected complex elements `1 + 1im` / `2 + 2im`, got: {output:?}"
    );
}

#[test]
fn println_comprehension_of_complex_no_structref_leak() {
    let output =
        compile_and_run_str_with_output("println([complex(x, x) for x in [1, 2]])\n0\n", 0);
    assert_no_structref_leak(&output, "println([complex comprehension])");
    assert!(
        output.contains("1 + 1im") && output.contains("2 + 2im"),
        "expected complex elements `1 + 1im` / `2 + 2im`, got: {output:?}"
    );
}

#[test]
fn println_array_of_user_struct_no_structref_leak() {
    let src = "\
struct Foo5234
    x::Int
end
println([Foo5234(1), Foo5234(2)])
0
";
    let output = compile_and_run_str_with_output(src, 0);
    assert_no_structref_leak(&output, "println([Foo5234...])");
    assert!(
        output.contains("Foo5234(1)") && output.contains("Foo5234(2)"),
        "expected struct elements `Foo5234(1)` / `Foo5234(2)`, got: {output:?}"
    );
}

#[test]
fn println_comprehension_of_user_struct_no_structref_leak() {
    let src = "\
struct Bar5234
    x::Int
end
println([Bar5234(i) for i in 1:3])
0
";
    let output = compile_and_run_str_with_output(src, 0);
    assert_no_structref_leak(&output, "println([Bar5234 comprehension])");
    assert!(
        output.contains("Bar5234(1)") && output.contains("Bar5234(3)"),
        "expected struct elements `Bar5234(1)` / `Bar5234(3)`, got: {output:?}"
    );
}

#[test]
fn println_matrix_of_pair_no_structref_leak() {
    let output = compile_and_run_str_with_output("println([1 => 2 3 => 4; 5 => 6 7 => 8])\n0\n", 0);
    assert_no_structref_leak(&output, "println(matrix of Pair)");
    assert!(
        output.contains("1 => 2") && output.contains("7 => 8"),
        "expected matrix Pair elements, got: {output:?}"
    );
}
