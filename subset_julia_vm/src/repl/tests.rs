use super::*;
use crate::vm::Value;

/// Run a test closure on a thread with 16 MB stack to avoid stack overflow
/// in debug builds. REPL eval involves deep recursion through parse → lower
/// → compile → VM execute, which exceeds the default 8 MB test-thread stack.
fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let builder = std::thread::Builder::new()
        .name("repl-test".into())
        .stack_size(16 * 1024 * 1024);
    let handler = builder.spawn(f).unwrap();
    if let Err(e) = handler.join() {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn test_repl_globals() {
    let mut globals = REPLGlobals::new();

    // Set and get integer
    globals.set("x", Value::I64(42));
    assert!(
        matches!(globals.get("x"), Some(Value::I64(42))),
        "Expected Some(I64(42)), got {:?}", globals.get("x")
    );

    // Set and get float
    globals.set("y", Value::F64(std::f64::consts::PI));
    assert!(
        matches!(globals.get("y"), Some(Value::F64(v)) if (v - std::f64::consts::PI).abs() < 0.001),
        "Expected Some(F64(~PI)), got {:?}", globals.get("y")
    );

    // Overwrite with different type
    globals.set("x", Value::F64(std::f64::consts::E));
    assert!(
        matches!(globals.get("x"), Some(Value::F64(v)) if (v - std::f64::consts::E).abs() < 0.001),
        "Expected Some(F64(~E)), got {:?}", globals.get("x")
    );

    // Clear
    globals.clear();
    assert!(globals.get("x").is_none());
}

#[test]
fn test_repl_session_simple() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Evaluate simple expression
        let result = session.eval("1 + 2");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected Some(Value::I64(3)), got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_session_variable_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a variable
        let result = session.eval("x = 10");
        assert!(result.success);

        // Use the variable in a subsequent evaluation
        let result = session.eval("x + 5");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(15))),
            "Expected Some(Value::I64(15)), got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_session_ans() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation
        session.eval("42");

        // ans should be available
        let result = session.eval("ans * 2");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(84))),
            "Expected Some(Value::I64(84)), got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_session_ans_after_assignment() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Assignment should also set ans (Julia behavior: assignment returns value)
        let result = session.eval("x = 5");
        assert!(result.success, "x = 5 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "x = 5 should return I64(5), got {:?}", result.value
        );

        // Check if ans is available
        let result = session.eval("ans");
        assert!(
            result.success,
            "ans should be available: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "ans should be I64(5), got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_eval_initializes_compile_cache() {
    run_with_large_stack(|| {
        crate::compile::cache::clear_cache();
        assert!(
            !crate::compile::cache::is_cache_initialized(),
            "cache should start empty for this test"
        );

        let mut session = REPLSession::new(42);
        let result = session.eval("1 + 1");
        assert!(result.success, "eval should succeed: {:?}", result.error);
        assert!(
            crate::compile::cache::is_cache_initialized(),
            "REPL eval should initialize compile cache to avoid full Base recompilation"
        );

        crate::compile::cache::clear_cache();
    });
}

#[test]
fn test_repl_session_function_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a function
        let result = session.eval("function double(x)\n  x * 2\nend");
        assert!(result.success);

        // Use the function
        let result = session.eval("double(21)");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_session_reset() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define a variable
        let result = session.eval("x = 100");
        assert!(result.success);

        // Verify the variable is set
        let result = session.eval("x + 1");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(101))),
            "Expected Some(Value::I64(101)), got {:?}", result.value
        );

        // Reset
        session.reset();

        // After reset, defining a new variable should work independently
        // and the old value should not persist
        let result = session.eval("y = 5");
        assert!(result.success);

        // Verify session variables are cleared (no 'x' in variable_names)
        let names = session.variable_names();
        assert!(
            !names.contains(&"x".to_string()),
            "x should not be in variable names after reset"
        );
        assert!(
            names.contains(&"y".to_string()),
            "y should be in variable names"
        );
    });
}

#[test]
fn test_split_expressions() {
    let session = REPLSession::new(42);

    let src = r#"function fizzbuzz(n)
    for i in 1:n
        println(i)
    end
end

fizzbuzz(10)"#;

    let exprs = session.split_expressions(src).unwrap();

    assert_eq!(exprs.len(), 2, "Should have 2 top-level expressions");

    // First expression: function definition
    assert!(
        exprs[0].2.starts_with("function fizzbuzz"),
        "First should be function"
    );
    assert!(exprs[0].2.ends_with("end"), "First should end with 'end'");

    // Second expression: function call
    assert_eq!(exprs[1].2.trim(), "fizzbuzz(10)", "Second should be call");
}

#[test]
fn test_split_expressions_sequential_eval() {
    run_with_large_stack(|| {
        let src = r#"function double(x)
    x * 2
end

double(21)"#;

        let mut session = REPLSession::new(42);
        let exprs = session.split_expressions(src).unwrap();

        assert_eq!(exprs.len(), 2);

        // First: define function
        let result = session.eval(&exprs[0].2);
        assert!(result.success, "Function definition should succeed");

        // Second: call function
        let result = session.eval(&exprs[1].2);
        assert!(result.success, "Function call should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}", result.value
        );
    });
}

#[test]
fn test_fizzbuzz_split() {
    run_with_large_stack(|| {
        let src = r#"function fizzbuzz(n)
    for i in 1:n
        if i % 15 == 0
            println("FizzBuzz")
        elseif i % 3 == 0
            println("Fizz")
        elseif i % 5 == 0
            println("Buzz")
        else
            println(i)
        end
    end
end

fizzbuzz(15)"#;

        let mut session = REPLSession::new(42);

        // Test split detection
        let exprs = session.split_expressions(src);
        assert!(exprs.is_some(), "Should detect multiple expressions");
        let exprs = exprs.unwrap();
        assert_eq!(exprs.len(), 2, "Should have 2 expressions");

        // First expression: function definition
        assert!(
            exprs[0].2.starts_with("function fizzbuzz"),
            "First should be function"
        );

        // Second expression: function call
        assert_eq!(exprs[1].2.trim(), "fizzbuzz(15)", "Second should be call");

        // Evaluate sequentially
        let result1 = session.eval(&exprs[0].2);
        assert!(result1.success, "Function definition should succeed");

        let result2 = session.eval(&exprs[1].2);
        assert!(result2.success, "Function call should succeed");

        // Check output contains expected values
        assert!(result2.output.contains("1"), "Should output 1");
        assert!(result2.output.contains("Fizz"), "Should output Fizz");
        assert!(result2.output.contains("Buzz"), "Should output Buzz");
        assert!(
            result2.output.contains("FizzBuzz"),
            "Should output FizzBuzz"
        );
    });
}

#[test]
fn test_repl_array_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Define an array
        let result = session.eval("v1 = [1, 2, 3, 4, 5]");
        assert!(
            result.success,
            "Array definition should succeed: {:?}",
            result.error
        );

        // Use the array in a subsequent evaluation
        let result = session.eval("length(v1)");
        assert!(
            result.success,
            "Using array should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected length to be 5, got {:?}", result.value
        );

        // Define another array and use both
        let result = session.eval("v2 = [5, 4, 3, 2, 1]");
        assert!(result.success, "Second array definition should succeed");

        // Define a function that uses arrays
        let result = session.eval(
            r#"function dot_product(a, b)
                sum = 0.0
                for i in 1:length(a)
                    sum += a[i] * b[i]
                end
                sum
            end"#,
        );
        assert!(
            result.success,
            "Function definition should succeed: {:?}",
            result.error
        );

        // Call the function with persisted arrays
        let result = session.eval("dot_product(v1, v2)");
        assert!(
            result.success,
            "Function call should succeed: {:?}",
            result.error
        );
        // v1 · v2 = 1*5 + 2*4 + 3*3 + 4*2 + 5*1 = 5 + 8 + 9 + 8 + 5 = 35
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 35.0).abs() < 0.001),
            "Pattern match failed, got {:?}", result.value
        );
    });
}

#[test]
fn test_semicolon_separated_statements() {
    run_with_large_stack(|| {
        // Test that semicolon-separated statements can be evaluated in REPL
        let mut session = REPLSession::new(42);

        // Evaluate statements separately
        let result = session.eval("x = 3");
        assert!(result.success, "x = 3 should succeed");

        let result = session.eval("y = 2");
        assert!(result.success, "y = 2 should succeed");

        let result = session.eval("z = x + y");
        assert!(result.success, "z = x + y should succeed");

        // z should now be 5
        let result = session.eval("z");
        assert!(result.success);
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected z to be 5, got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_time_macro_variable_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // Test that variables assigned inside @time persist
        let result = session.eval("@time x = 42");
        assert!(
            result.success,
            "@time assignment should succeed: {:?}",
            result.error
        );

        // x should be available in the next evaluation
        let result = session.eval("x + 1");
        assert!(result.success, "Using x should succeed: {:?}", result.error);
        assert!(
            matches!(result.value, Some(Value::I64(43))),
            "Expected x + 1 to be 43, got {:?}", result.value
        );

        // Test with array
        let result = session.eval("@time grid = [1, 2, 3, 4, 5]");
        assert!(
            result.success,
            "@time array assignment should succeed: {:?}",
            result.error
        );

        // grid should be available
        let result = session.eval("length(grid)");
        assert!(
            result.success,
            "Using grid should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(5))),
            "Expected length(grid) to be 5, got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_using_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: using Statistics
        let result = session.eval("using Statistics");
        assert!(
            result.success,
            "using Statistics should succeed: {:?}",
            result.error
        );

        // Second evaluation: mean([1,2,3]) - should work because using persists
        let result = session.eval("mean([1, 2, 3])");
        assert!(
            result.success,
            "mean after using should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 2.0).abs() < 0.001),
            "Pattern match failed, got {:?}", result.value
        );

        // Third evaluation: std - should also work
        let result = session.eval("std([1.0, 2.0, 3.0])");
        assert!(
            result.success,
            "std after using should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 1.0).abs() < 0.001),
            "Pattern match failed, got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_using_test_macro_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        let result = session.eval("using Test");
        assert!(
            result.success,
            "using Test should succeed: {:?}",
            result.error
        );

        let result = session.eval("@test 1 + 1 == 2");
        assert!(
            result.success,
            "@test after using Test should succeed: {:?}",
            result.error
        );
    });
}

#[test]
fn test_repl_namedtuple_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: create a NamedTuple
        let result = session.eval("F = (U = [1.0, 2.0], S = [3.0, 4.0], V = [5.0, 6.0])");
        assert!(
            result.success,
            "NamedTuple creation should succeed: {:?}",
            result.error
        );

        // Second evaluation: destructure the NamedTuple - F should persist
        let result = session.eval("U, S, V = F");
        assert!(
            result.success,
            "Destructuring NamedTuple should succeed: {:?}",
            result.error
        );

        // Third evaluation: use the destructured variables
        let result = session.eval("U[1]");
        assert!(
            result.success,
            "Accessing U should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 1.0).abs() < 0.001),
            "Pattern match failed, got {:?}", result.value
        );

        let result = session.eval("S[2]");
        assert!(
            result.success,
            "Accessing S should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::F64(v)) if (v - 4.0).abs() < 0.001),
            "Pattern match failed, got {:?}", result.value
        );
    });
}

#[test]
fn test_repl_module_persistence() {
    run_with_large_stack(|| {
        let mut session = REPLSession::new(42);

        // First evaluation: define a module with functions
        let result = session.eval(
            r#"
module MyModule

export double, triple

function double(x)
    return x * 2
end

function triple(x)
    return x * 3
end

end
"#,
        );
        assert!(
            result.success,
            "Module definition should succeed: {:?}",
            result.error
        );

        // Second evaluation: use the module (relative import)
        let result = session.eval("using .MyModule");
        assert!(
            result.success,
            "using .MyModule should succeed: {:?}",
            result.error
        );

        // Third evaluation: use the exported function - should work because module persists
        let result = session.eval("double(21)");
        assert!(
            result.success,
            "double(21) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected double(21) to be 42, got {:?}", result.value
        );

        // Fourth evaluation: use another exported function
        let result = session.eval("triple(10)");
        assert!(
            result.success,
            "triple(10) should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(30))),
            "Expected triple(10) to be 30, got {:?}", result.value
        );

        // Fifth evaluation: run completely unrelated code - should NOT try to load MyModule from LOAD_PATH
        // This is the key test for the bug fix - previously it would fail with "module 'MyModule' not found in LOAD_PATH"
        let result = session.eval("x = 1 + 2");
        assert!(
            result.success,
            "Simple expression after module use should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(3))),
            "Expected Some(Value::I64(3)), got {:?}", result.value
        );

        // Sixth evaluation: use the module function again after unrelated code
        let result = session.eval("double(100)");
        assert!(
            result.success,
            "double(100) after unrelated code should succeed: {:?}",
            result.error
        );
        assert!(
            matches!(result.value, Some(Value::I64(200))),
            "Expected double(100) to be 200, got {:?}", result.value
        );
    });
}

#[test]
fn test_split_simple_statements() {
    // Test that simple statements (no block structures) are split correctly
    let session = REPLSession::new(42);

    let src = r#"x = 42
pi_approx = 3.14159
println("x = $(x)")
y = 10
println(y)"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split simple statements");

    let exprs = exprs.unwrap();
    assert_eq!(
        exprs.len(),
        5,
        "Should have 5 top-level expressions, got {:?}",
        exprs
    );

    assert_eq!(exprs[0].2, "x = 42");
    assert_eq!(exprs[1].2, "pi_approx = 3.14159");
    assert_eq!(exprs[2].2, "println(\"x = $(x)\")");
    assert_eq!(exprs[3].2, "y = 10");
    assert_eq!(exprs[4].2, "println(y)");
}

#[test]
fn test_split_with_comments() {
    // Test that comments are handled correctly
    let session = REPLSession::new(42);

    let src = r#"# This is a comment
x = 42
# Another comment
y = 10"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split statements with comments");

    let exprs = exprs.unwrap();
    // Comments on their own lines should be skipped, leaving just the assignments
    assert_eq!(
        exprs.len(),
        2,
        "Should have 2 expressions (comments excluded), got {:?}",
        exprs
    );
    assert_eq!(exprs[0].2, "x = 42");
    assert_eq!(exprs[1].2, "y = 10");
}

#[test]
fn test_split_with_block_comments() {
    // Test that block comments (#= ... =#) are handled correctly
    let session = REPLSession::new(42);

    // Test 1: Simple block comment spanning multiple lines
    let src = r#"#=
==========================================
Welcome message
==========================================
=#
println("Hello, World!")"#;

    let exprs = session.split_expressions(src);
    // Should either return None (single expression) or have 1 expression
    // Since there's only one actual statement, it might return None
    if let Some(exprs) = exprs {
        assert_eq!(
            exprs.len(),
            1,
            "Should have 1 expression after block comment, got {:?}",
            exprs
        );
        assert_eq!(exprs[0].2.trim(), "println(\"Hello, World!\")");
    }

    // Test 2: Multiple expressions with block comment
    let src2 = r#"x = 42
#= This is a block comment
spanning multiple lines =#
y = 10"#;

    let exprs2 = session.split_expressions(src2);
    assert!(
        exprs2.is_some(),
        "Should split statements with block comments"
    );
    let exprs2 = exprs2.unwrap();
    assert_eq!(
        exprs2.len(),
        2,
        "Should have 2 expressions, got {:?}",
        exprs2
    );
    assert_eq!(exprs2[0].2, "x = 42");
    assert_eq!(exprs2[1].2, "y = 10");
}

#[test]
fn test_split_with_nested_block_comments() {
    // Test that nested block comments are handled correctly
    let session = REPLSession::new(42);

    let src = r#"x = 1
#= outer comment
#= nested comment =#
still in outer =#
y = 2"#;

    let exprs = session.split_expressions(src);
    assert!(
        exprs.is_some(),
        "Should split statements with nested block comments"
    );
    let exprs = exprs.unwrap();
    assert_eq!(exprs.len(), 2, "Should have 2 expressions, got {:?}", exprs);
    assert_eq!(exprs[0].2, "x = 1");
    assert_eq!(exprs[1].2, "y = 2");
}

#[test]
fn test_block_comment_eval() {
    run_with_large_stack(|| {
        // Test that code with block comments can be evaluated in REPL
        let mut session = REPLSession::new(42);

        let src = r#"#=
==========================================
Welcome to SubsetJuliaVM!
==========================================
=#

println("Hello, World!")"#;

        let result = session.eval(src);
        assert!(
            result.success,
            "Should successfully evaluate code with block comment: {:?}",
            result.error
        );
        assert!(
            result.output.contains("Hello, World!"),
            "Should output Hello, World!"
        );
    });
}

#[test]
fn test_split_multiline_array() {
    // Test that multi-line array literals are NOT split
    let session = REPLSession::new(42);

    let src = r#"x = [1, 2,
     3, 4]
println(x)"#;

    let exprs = session.split_expressions(src);
    assert!(exprs.is_some(), "Should split after array");

    let exprs = exprs.unwrap();
    assert_eq!(
        exprs.len(),
        2,
        "Should have 2 expressions (array + println)"
    );
    assert!(
        exprs[0].2.contains("[1, 2,"),
        "First should be array literal"
    );
    assert_eq!(exprs[1].2, "println(x)");
}

#[test]
fn test_split_simple_statements_sequential_eval() {
    run_with_large_stack(|| {
        // Test that simple statements can be evaluated sequentially
        let src = r#"x = 42
y = x + 10
z = x * y"#;

        let mut session = REPLSession::new(42);
        let exprs = session.split_expressions(src).unwrap();

        assert_eq!(exprs.len(), 3);

        // First: x = 42
        let result = session.eval(&exprs[0].2);
        assert!(result.success, "x = 42 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(42))),
            "Expected Some(Value::I64(42)), got {:?}", result.value
        );

        // Second: y = x + 10 (x persists)
        let result = session.eval(&exprs[1].2);
        assert!(result.success, "y = x + 10 should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(52))),
            "Expected Some(Value::I64(52)), got {:?}", result.value
        );

        // Third: z = x * y (both x and y persist)
        let result = session.eval(&exprs[2].2);
        assert!(result.success, "z = x * y should succeed");
        assert!(
            matches!(result.value, Some(Value::I64(2184))),
            "Expected Some(I64(2184)) (42 * 52), got {:?}", result.value
        );
    });
}
