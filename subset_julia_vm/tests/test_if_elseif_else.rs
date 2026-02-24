//! Test to verify if/elseif/else parsing and execution

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

fn run_and_get_output(src: &str) -> String {
    let mut parser = Parser::new().expect("Parser initialization failed");
    let parsed = parser.parse(src).expect("Parse failed");
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).expect("Lowering failed");
    let compiled = compile_core_program(&program).expect("Compilation failed");

    let rng = StableRng::new(12345);
    let mut vm = Vm::new_program(compiled, rng);
    let _result = vm.run();
    vm.get_output().to_string()
}

#[test]
fn test_if_elseif_else_basic() {
    println!("\n=== Test if/elseif/else basic ===");

    let src = r#"
n = 50
if n == 50
    print("*")
elseif n > 10
    print("+")
else
    print(" ")
end
"#;

    let output = run_and_get_output(src);
    println!("Output: '{}'", output);
    assert_eq!(output.trim(), "*", "Expected '*' for n=50");
}

#[test]
fn test_if_elseif_else_variations() {
    println!("\n=== Test if/elseif/else variations ===");

    // Test case 1: n == 50 (should print '*')
    let src1 = r#"
n = 50
if n == 50
    print("*")
elseif n > 10
    print("+")
else
    print(" ")
end
"#;
    let output1 = run_and_get_output(src1);
    println!("n=50: output='{}'", output1);
    assert_eq!(output1.trim(), "*", "Expected '*' for n=50");

    // Test case 2: n > 10 but != 50 (should print '+')
    let src2 = r#"
n = 20
if n == 50
    print("*")
elseif n > 10
    print("+")
else
    print(" ")
end
"#;
    let output2 = run_and_get_output(src2);
    println!("n=20: output='{}'", output2);
    assert_eq!(output2.trim(), "+", "Expected '+' for n=20");

    // Test case 3: n <= 10 (should print ' ')
    let src3 = r#"
n = 5
if n == 50
    print("*")
elseif n > 10
    print("+")
else
    print(" ")
end
"#;
    let output3 = run_and_get_output(src3);
    println!("n=5: output='{}'", output3);
    // Don't trim - we're comparing against a space which would be trimmed away
    assert_eq!(output3, " ", "Expected ' ' for n=5");
}

#[test]
fn test_mandelbrot_style_condition() {
    println!("\n=== Test Mandelbrot-style condition ===");

    // This matches the exact structure from Mandelbrot code
    // n == 50 -> "*", n > 10 -> "+", else -> " "
    let test_cases = vec![
        (50, "*"),  // n == 50
        (20, "+"),  // n > 10
        (5, " "),   // else
        (100, "+"), // n > 10 (not n == 50!)
        (11, "+"),  // n > 10
        (10, " "),  // else (10 is not > 10)
    ];

    for (n, expected) in test_cases {
        let src = format!(
            r#"
n = {}
if n == 50
    print("*")
elseif n > 10
    print("+")
else
    print(" ")
end
"#,
            n
        );

        let output = run_and_get_output(&src);
        println!("n={}: output='{}', expected='{}'", n, output, expected);
        // Don't trim - we're comparing against a space which would be trimmed away
        assert_eq!(output, expected, "Mismatch for n={}", n);
    }
}

#[test]
fn test_multiple_elseif() {
    println!("\n=== Test multiple elseif clauses ===");

    let src = r#"
x = 3
if x == 1
    print("one")
elseif x == 2
    print("two")
elseif x == 3
    print("three")
else
    print("other")
end
"#;

    let output = run_and_get_output(src);
    println!("Output: '{}'", output);
    assert_eq!(output.trim(), "three", "Expected 'three' for x=3");
}
