//! Integration tests: Module support, Base module, basic arithmetic, random numbers
#![allow(dead_code)]

mod common;
use common::*;

use subset_julia_vm::vm::Value;
use subset_julia_vm::*;

// ==================== Module Support ====================

fn test_simple_module() {
    let src = r#"
module MyModule
    x = 42
    println(x)
end
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok());
    // Module execution should complete without error
}

fn test_module_with_function() {
    let src = r#"
module MyModule
    function add(a, b)
        return a + b
    end
    result = add(10, 20)
    println(result)
end
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok());
}

fn test_module_with_main() {
    // Test that main block runs after module definition
    // Uses function call since Module.constant access isn't supported yet
    let src = r#"
module MyModule
    get_value() = 100
end
println("After module")
MyModule.get_value()
"#;
    let result = run_core_pipeline(src, 0);
    // Main should return the value from MyModule.get_value() (100)
    assert_ok_numeric(result, 100.0);
}

fn test_module_qualified_call() {
    // Test Module.func() qualified call syntax
    let src = r#"
module Math
    function square(x)
        return x * x
    end
    function add(a, b)
        return a + b
    end
end

result = Math.square(5)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 25.0);
}

fn test_module_qualified_call_with_args() {
    // Test Module.func() with multiple arguments
    let src = r#"
module Calculator
    function multiply(a, b)
        return a * b
    end
end

result = Calculator.multiply(7, 8)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 56.0);
}

fn test_module_qualified_call_multiple_functions() {
    // Test calling multiple functions from the same module
    let src = r#"
module Utils
    function double(x)
        return x * 2
    end
    function triple(x)
        return x * 3
    end
end

a = Utils.double(10)
b = Utils.triple(10)
a + b
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 50.0); // 20 + 30
}

fn test_module_qualified_call_unknown_module() {
    // Test error when calling function from unknown module
    let src = r#"
module MyModule
    function foo()
        return 1
    end
end

result = UnknownModule.foo()
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err());
}

fn test_module_qualified_call_unknown_function() {
    // Test error when calling unknown function from module
    let src = r#"
module MyModule
    function foo()
        return 1
    end
end

result = MyModule.bar()
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err());
}

fn test_module_qualified_alias_does_not_fall_back_to_unrelated_bare_alias_7955() {
    let src = r#"
module AliasOwner7955
    const T = Int64
end

module AliasOther7955
end

AliasOther7955.T
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err());
    let err = format!("{:?}", result.err().unwrap());
    assert!(
        err.contains("AliasOther7955") && err.contains("T"),
        "unexpected error: {}",
        err
    );
}

fn test_using_module() {
    // Test using statement to import module functions
    let src = r#"
module Math
    function square(x)
        return x * x
    end
end

using Math

result = square(6)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 36.0);
}

fn test_import_module() {
    // Test import statement (treated same as using in MVP)
    let src = r#"
module Utils
    function double(x)
        return x * 2
    end
end

import Utils

result = double(7)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 14.0);
}

fn test_using_with_qualified_call() {
    // Test using combined with qualified call
    let src = r#"
module Calculator
    function add(a, b)
        return a + b
    end
    function sub(a, b)
        return a - b
    end
end

using Calculator

# Can use both direct call and qualified call
a = add(10, 5)
b = Calculator.sub(10, 5)
a + b
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 20.0); // 15 + 5
}

fn test_export_statement() {
    // Test export statement in module
    let src = r#"
module Geometry
    export area

    function area(r)
        return 3.14159 * r * r
    end

    function circumference(r)
        return 2 * 3.14159 * r
    end
end

using Geometry

# area is exported, so it should be available
result = area(2.0)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_ok());
    if let Ok(Value::F64(v)) = result {
        assert!((v - 12.56636).abs() < 1e-4);
    } else {
        panic!("Expected ~12.56636, got {:?}", result);
    }
}

fn test_export_multiple_functions() {
    // Test exporting multiple functions
    let src = r#"
module Math
    export add, mul

    function add(a, b)
        return a + b
    end

    function mul(a, b)
        return a * b
    end

    function sub(a, b)
        return a - b
    end
end

using Math

# Both add and mul are exported
result = add(3, 4) + mul(2, 5)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 17.0); // 7 + 10
}

fn test_qualified_call_bypasses_export() {
    // Test that Module.func() works even for non-exported functions
    let src = r#"
module Utils
    export public_func

    function public_func()
        return 1
    end

    function private_func()
        return 2
    end
end

# Qualified call should work for any function
result = Utils.private_func()
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 2.0);
}

fn test_selective_import() {
    // Test using Module: func selective import
    let src = r#"
module Math
    export add, sub

    function add(a, b)
        return a + b
    end

    function sub(a, b)
        return a - b
    end

    function mul(a, b)
        return a * b
    end
end

# Only import add, not sub or mul
using Math: add

result = add(3, 4)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 7.0);
}

fn test_selective_import_multiple() {
    // Test using Module: func1, func2 with multiple functions
    // Note: Use unique function names to avoid collision with Base functions
    let src = r#"
module Utils
    export get_one, get_two, get_three

    function get_one()
        return 1
    end

    function get_two()
        return 2
    end

    function get_three()
        return 3
    end
end

# Import only get_one and get_three
using Utils: get_one, get_three

result = get_one() + get_three()
result
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 4.0); // 1 + 3
}

fn test_non_exported_function_blocked() {
    // Test that non-exported functions cannot be called via using
    let src = r#"
module Secret
    export public_func

    function public_func()
        return 1
    end

    function private_func()
        return 2
    end
end

using Secret

# This should fail - private_func is not exported
result = private_func()
result
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when calling non-exported function, but got: {:?}",
        result
    );
}

fn test_non_imported_function_blocked() {
    // Test that functions not in selective import cannot be called
    let src = r#"
module Math
    export add, sub

    function add(a, b)
        return a + b
    end

    function sub(a, b)
        return a - b
    end
end

# Only import add
using Math: add

# This should fail - sub was not imported
result = sub(5, 3)
result
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when calling non-imported function, but got: {:?}",
        result
    );
}

fn test_module_function_without_using() {
    // Test that module functions cannot be called without using
    let src = r#"
module Util
    export helper

    function helper()
        return 42
    end
end

# No using statement - should fail
result = helper()
result
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error when calling module function without using, but got: {:?}",
        result
    );
}

// ==================== Relative Imports (using .Module) ====================

fn test_relative_import_basic() {
    // Test using .Module syntax for user-defined modules
    let src = r#"
module MyModule
    export greet

    function greet()
        return 42
    end
end

using .MyModule

greet()
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 42.0);
}

fn test_relative_import_qualified_call() {
    // Test qualified call with relative import
    let src = r#"
module Math
    function add(a, b)
        return a + b
    end
end

using .Math

# Qualified call should work
Math.add(10, 20)
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 30.0);
}

fn test_relative_import_with_export() {
    // Test relative import respects export statement
    let src = r#"
module Utils
    export public_func

    function public_func()
        return 100
    end

    function private_func()
        return 200
    end
end

using .Utils

# Direct call to exported function should work
public_func()
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 100.0);
}

fn test_relative_import_selective() {
    // Test selective relative import: using .Module: func
    let src = r#"
module Tools
    function foo()
        return 1
    end

    function bar()
        return 2
    end
end

using .Tools: foo

# foo should work
foo()
"#;
    let result = run_core_pipeline(src, 0);
    assert_ok_numeric(result, 1.0);
}

// ==================== Nested Modules ====================

fn test_nested_module_basic() {
    // Test basic nested module definition
    let src = r#"
module Outer
    module Inner
        function greet()
            return 42
        end
    end
end

Outer.Inner.greet()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

fn test_nested_module_multiple_levels() {
    // Test three-level nested module (A.B.C.func)
    let src = r#"
module A
    module B
        module C
            function compute()
                return 123
            end
        end
    end
end

A.B.C.compute()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 123.0).abs() < 1e-10);
}

fn test_nested_module_with_parent_function() {
    // Test nested module where parent also has functions
    let src = r#"
module Parent
    function parent_func()
        return 10
    end

    module Child
        function child_func()
            return 20
        end
    end
end

result = Parent.parent_func() + Parent.Child.child_func()
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 30.0).abs() < 1e-10);
}

fn test_nested_module_with_arguments() {
    // Test nested module function with arguments
    let src = r#"
module Math
    module Ops
        function add(a, b)
            return a + b
        end

        function mul(a, b)
            return a * b
        end
    end
end

result = Math.Ops.add(3, 4) + Math.Ops.mul(2, 5)
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 17.0).abs() < 1e-10); // 7 + 10 = 17
}

fn test_nested_module_sibling_submodules() {
    // Test multiple sibling submodules
    let src = r#"
module Utils
    module StringOps
        function get_length()
            return 5
        end
    end

    module MathOps
        function square(x)
            return x * x
        end
    end
end

result = Utils.MathOps.square(4) + Utils.StringOps.get_length()
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 21.0).abs() < 1e-10); // 16 + 5 = 21
}

fn test_nested_module_unknown_path() {
    // Test error when accessing unknown nested module path
    let src = r#"
module A
    module B
        function f()
            return 1
        end
    end
end

A.C.f()
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for unknown module path, but got: {:?}",
        result
    );
}

// ==================== Base Module ====================

fn test_base_sqrt() {
    // Test Base.sqrt() qualified call
    let src = r#"
Base.sqrt(16)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10);
}

fn test_base_math_functions() {
    // Test various Base math functions
    // Note: abs is now Pure Julia, so use it directly instead of Base.abs
    let src = r#"
result = Base.sin(0) + Base.cos(0) + abs(-5)
result
"#;
    let result = compile_and_run_str(src, 0);
    // sin(0) = 0, cos(0) = 1, abs(-5) = 5 => 0 + 1 + 5 = 6
    assert!((result - 6.0).abs() < 1e-10);
}

fn test_base_array_functions() {
    // Test Base array creation functions
    let src = r#"
arr = Base.zeros(3)
Base.length(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10);
}

fn test_base_println() {
    // Test Base.println()
    let src = r#"
Base.println("Hello from Base")
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

fn test_base_unknown_function() {
    // Test error when calling unknown function from Base
    let src = r#"
Base.unknown_function()
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for unknown Base function, but got: {:?}",
        result
    );
}

fn test_base_no_implicit_shadowing() {
    // Test that user-defined function does NOT shadow Base function
    // Base functions are always called, even if a user-defined function has the same name
    let src = r#"
function sqrt(x)
    return -1.0  # Custom implementation (never called)
end

# sqrt() still calls Base.sqrt, not the user-defined one
result = sqrt(16)
result
"#;
    let result = compile_and_run_str(src, 0);
    // Base.sqrt(16) = 4.0, not -1.0
    assert!((result - 4.0).abs() < 1e-10);
}

fn test_base_explicit_qualified() {
    // Test that Base.func() works for explicit qualification
    let src = r#"
# Explicit Base.sqrt() call
result = Base.sqrt(16)
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10);
}

fn test_base_both_unqualified_and_qualified() {
    // Test that both unqualified and qualified calls work identically
    // (since user-defined functions don't shadow Base)
    let src = r#"
function sqrt(x)
    return x * 2  # Never called - Base.sqrt takes precedence
end

# Both call Base.sqrt
result = sqrt(16) + Base.sqrt(9)  # 4 + 3 = 7
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 7.0).abs() < 1e-10);
}

fn test_base_higher_order_functions() {
    // Test Base higher-order functions
    let src = r#"
function double(x)
    return 2x
end

arr = [1, 2, 3]
result = Base.sum(Base.map(double, arr))  # 2 + 4 + 6 = 12
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 12.0).abs() < 1e-10);
}

fn test_base_in_function() {
    // Test calling Base functions from within a top-level function
    // Using sqrt directly (not Base.sqrt) to isolate the issue
    let src = r#"
function compute(x)
    return sqrt(x)
end

compute(16)  # sqrt(16) = 4
"#;
    let result = run_core_pipeline(src, 0);
    eprintln!("Result: {:?}", result);
    match result {
        Ok(Value::F64(v)) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
        Ok(v) => panic!("Expected F64, got {:?}", v),
        Err(e) => panic!("Expected Ok, got Err: {}", e),
    }
}

// ==================== Base Submodules (Phase B3) ====================

fn test_base_math_submodule() {
    // Test Base.Math.sqrt, Base.Math.sin, etc.
    let src = r#"
result = Base.Math.sqrt(16) + Base.Math.sin(0)
result
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10); // sqrt(16) = 4, sin(0) = 0
}

fn test_base_math_multiple_functions() {
    // Test multiple Math functions
    // Note: abs is now Pure Julia, so use it directly instead of Base.Math.abs
    let src = r#"
a = abs(-5)
b = Base.Math.floor(3.7)
c = Base.Math.ceil(2.1)
a + b + c
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 11.0).abs() < 1e-10); // 5 + 3 + 3 = 11
}

fn test_base_io_submodule() {
    // Test Base.IO.println (just verify it compiles and runs)
    let src = r#"
Base.IO.println("Hello from Base.IO")
42
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

fn test_base_collections_submodule() {
    // Test Base.Collections functions
    let src = r#"
arr = [1, 2, 3]
len = Base.Collections.length(arr)
len
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10);
}

fn test_base_collections_zeros_ones() {
    // Test Base.Collections.zeros and ones
    let src = r#"
arr = Base.Collections.zeros(3)
arr2 = Base.Collections.ones(2)
Base.Collections.length(arr) + Base.Collections.length(arr2)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10);
}

fn test_base_random_submodule() {
    let result = run_core_pipeline("Base.Random.rand()\n", 42);
    assert!(
        result.is_err(),
        "Base.Random.rand() should not bypass the Random stdlib root"
    );
}

fn test_base_complex_submodule() {
    // Test Complex functions
    // Note: complex and abs are now Pure Julia functions
    let src = r#"
z = complex(3, 4)
abs(z)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10); // |3+4i| = 5
}

fn test_base_iterators_submodule() {
    // Test Base.Iterators.map and sum
    let src = r#"
arr = [1, 2, 3, 4]
Base.Iterators.sum(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 10.0).abs() < 1e-10);
}

fn test_base_linearalgebra_is_not_a_submodule() {
    // LinearAlgebra is a stdlib root module loaded by `using LinearAlgebra`, not
    // a public Base submodule. This matches upstream Julia's `Base.LinearAlgebra`
    // UndefVarError behavior.
    let src = r#"
Base.LinearAlgebra
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Base.LinearAlgebra should be undefined");

    let src = r#"
using Base.LinearAlgebra
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "using Base.LinearAlgebra should be undefined"
    );
}

fn test_stdlib_roots_are_not_public_base_submodules_8278() {
    for module in [
        "Base64",
        "Dates",
        "InteractiveUtils",
        "LinearAlgebra",
        "Printf",
        "Random",
        "Statistics",
        "Test",
    ] {
        let property_src = format!("Base.{module}\n");
        let property_result = run_core_pipeline(&property_src, 0);
        assert!(
            property_result.is_err(),
            "Base.{module} should be undefined"
        );

        let using_src = format!("using Base.{module}\ntrue\n");
        let using_result = run_core_pipeline(&using_src, 0);
        assert!(
            using_result.is_err(),
            "using Base.{module} should be undefined"
        );
    }
}

fn test_linearalgebra_det_smoke_8276() {
    let src = r#"
using LinearAlgebra
det([1.0 2.0; 3.0 4.0])
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result + 2.0).abs() < 1e-10);
}

fn test_linearalgebra_inv_smoke_8276() {
    let src = r#"
using LinearAlgebra
A = [4.0 7.0; 2.0 6.0]
B = inv(A)
abs(B[1, 1] - 0.6) < 1e-10 &&
    abs(B[1, 2] + 0.7) < 1e-10 &&
    abs(B[2, 1] + 0.2) < 1e-10 &&
    abs(B[2, 2] - 0.4) < 1e-10
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        other => panic!("Expected true, got {:?}", other),
    }
}

fn test_linearalgebra_svd_smoke_8276() {
    let src = r#"
using LinearAlgebra
A = [1.0 2.0; 3.0 4.0; 5.0 6.0]
F = svd(A)
size(F.U, 1) == 3 &&
    size(F.U, 2) == 2 &&
    length(F.S) == 2 &&
    size(F.V, 1) == 2 &&
    size(F.V, 2) == 2 &&
    F.S[1] >= F.S[2]
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        other => panic!("Expected true, got {:?}", other),
    }
}

fn test_linearalgebra_eigen_smoke_8276() {
    let src = r#"
using LinearAlgebra
A = [2.0 1.0; 1.0 2.0]
F = eigen(A)
vals = F.values
vecs = F.vectors
v1 = vecs[1, 1]
v2 = vecs[2, 1]
lhs1 = A[1, 1] * v1 + A[1, 2] * v2
lhs2 = A[2, 1] * v1 + A[2, 2] * v2
rhs1 = vals[1] * v1
rhs2 = vals[1] * v2
length(vals) == 2 &&
    size(vecs, 1) == 2 &&
    size(vecs, 2) == 2 &&
    abs(lhs1 - rhs1) < 1e-8 &&
    abs(lhs2 - rhs2) < 1e-8
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        other => panic!("Expected true, got {:?}", other),
    }
}

fn test_base_submodule_unknown_function() {
    // Test error for unknown function in submodule
    let src = r#"
Base.Math.unknown_function(1)
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for unknown function in submodule"
    );
}

fn test_base_unknown_submodule() {
    // Test error for unknown submodule
    let src = r#"
Base.Unknown.sqrt(4)
"#;
    let result = run_core_pipeline(src, 0);
    assert!(result.is_err(), "Expected error for unknown submodule");
}

// ==================== Base Functions (Phase B4) ====================

fn test_base_parses() {
    // Test that Base source parses correctly
    use subset_julia_vm::base;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    let base_src = base::get_base();
    eprintln!("Base source length: {}", base_src.len());

    let mut parser = Parser::new().expect("Parser init failed");
    let outcome = parser.parse(&base_src).expect("Base parse failed");

    let mut lowering = Lowering::new(&base_src);
    let program = lowering.lower(outcome).expect("Base lowering failed");

    eprintln!("Base functions count: {}", program.functions.len());
    for f in &program.functions {
        eprintln!("  - {}", f.name);
    }

    assert!(!program.functions.is_empty(), "Base should have functions");
}

fn test_prelude_prod() {
    // Test prod function from prelude
    let src = r#"
arr = [2, 3, 4]
prod(arr)
"#;
    let result = run_core_pipeline(src, 0);
    eprintln!("test_prelude_prod result: {:?}", result);
    match result {
        Ok(Value::F64(v)) => assert!((v - 24.0).abs() < 1e-10, "Expected 24.0, got {}", v),
        Ok(Value::I64(v)) => assert_eq!(v, 24, "Expected 24, got {}", v),
        other => panic!("Expected numeric value, got {:?}", other),
    }
}

fn test_prelude_minimum_maximum() {
    // Test minimum and maximum functions
    let src = r#"
arr = [5, 2, 8, 1, 9]
minimum(arr) + maximum(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 10.0).abs() < 1e-10); // 1 + 9 = 10
}

fn test_prelude_sign() {
    // Test sign function
    let src = r#"
sign(-5) + sign(0) + sign(3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10); // -1 + 0 + 1 = 0
}

fn test_prelude_clamp() {
    // Test clamp function
    let src = r#"
clamp(5, 0, 10) + clamp(-5, 0, 10) + clamp(15, 0, 10)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 15.0).abs() < 1e-10); // 5 + 0 + 10 = 15
}

// Note: any, all, and count with function parameters are not yet supported.
// HOF functions any/all/count with lambda arguments are supported via builtin instructions.

fn test_prelude_any_all() {
    // Test any and all higher-order functions
    let src = r#"
arr = [1, 2, 3, 4, 5]
has_even = any(x -> x % 2 == 0, arr)
all_positive = all(x -> x > 0, arr)
has_even && all_positive
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        other => panic!("Expected true, got {:?}", other),
    }
}

fn test_prelude_count() {
    // Test count higher-order function
    let src = r#"
arr = [1, 2, 3, 4, 5, 6]
count(x -> x % 2 == 0, arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10); // 3 even numbers
}

fn test_prelude_argmin_argmax() {
    // Test argmin and argmax functions
    let src = r#"
arr = [5, 2, 8, 1, 9]
argmin(arr) + argmax(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 9.0).abs() < 1e-10); // 4 + 5 = 9 (1-indexed)
}

fn test_prelude_cumsum() {
    // Test cumsum function
    let src = r#"
arr = [1, 2, 3, 4]
cs = cumsum(arr)
cs[4]
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 10.0).abs() < 1e-10); // 1+2+3+4 = 10
}

fn test_statistics_mean() {
    // Test mean function via Statistics stdlib
    let src = r#"
using Statistics
arr = [2, 4, 6, 8]
mean(arr)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10); // (2+4+6+8)/4 = 5
}

fn test_prelude_hypot() {
    // Test hypot function (3-4-5 triangle)
    let src = r#"
hypot(3, 4)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10);
}

fn test_prelude_iseven_isodd() {
    // Test iseven and isodd functions
    let src = r#"
iseven(4) && isodd(5) && !iseven(3) && !isodd(6)
"#;
    let result = run_core_pipeline(src, 0);
    match result {
        Ok(Value::Bool(true)) => {}
        other => panic!("Expected true, got {:?}", other),
    }
}

// ==================== Include Tests ====================

fn test_include_lowers_to_program_body() {
    // include("file.jl") is now a lowered runtime operation; file existence is
    // checked by include execution, not by this direct lowering pass.
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;

    let src = r#"include("utils.jl")"#;
    let mut parser = Parser::new().expect("Parser init failed");
    let parsed = parser.parse(src).expect("Parse failed");
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(parsed).expect("include should lower");

    assert_eq!(program.main.stmts.len(), 1);
}

// ==================== Basic Arithmetic ====================

fn test_return_constant() {
    let src = r#"
function f(N)
    return 42
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

fn test_simple_multiplication() {
    let src = r#"
function f(N)
    return 2N
end
f(100)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 200.0).abs() < 1e-10);
}

fn test_addition() {
    let src = r#"
function f(N)
    return N + 10
end
f(32)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 42.0).abs() < 1e-10);
}

fn test_division() {
    let src = r#"
function f(N)
    return N / 4
end
f(100)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 25.0).abs() < 1e-10);
}

fn test_power_of_2() {
    let src = r#"
function f(N)
    return N^2
end
f(7)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 49.0).abs() < 1e-10);
}

fn test_sqrt() {
    let src = r#"
function f(N)
    return sqrt(N)
end
f(16)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10);
}

fn test_elementary_functions() {
    // Test sin
    let src = "function f(x) return sin(x) end\nf(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "sin(0) should be 0");

    // Test cos
    let src = "function f(x) return cos(x) end\nf(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "cos(0) should be 1");

    // Test exp
    let src = "function f(x) return exp(x) end\nf(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "exp(0) should be 1");

    // Test log
    let src = "function f(x) return log(x) end\nf(1.0)";
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "log(1) should be 0");

    // Test tan
    let src = "function f(x) return tan(x) end\nf(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "tan(0) should be 0");

    // Test exp(1) ≈ e
    let src = "function f(x) return exp(x) end\nf(1.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::E).abs() < 1e-10,
        "exp(1) should be e"
    );

    // Test cos(π) ≈ -1 (using script mode, not function)
    let src = "cos(3.141592653589793)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - (-1.0)).abs() < 1e-9,
        "cos(π) should be -1, got {}",
        result
    );
}

fn test_elementary_functions_broadcast() {
    // Test sin.(array)
    let src = r#"
x = [0.0, 1.5707963267948966]
y = sin.(x)
y[1] + y[2]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "sin([0, π/2]) should be [0, 1]"
    );

    // Test exp.(array)
    let src = r#"
x = [0.0, 1.0]
y = exp.(x)
y[1]
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "exp(0) should be 1");
}

fn test_inverse_trig_functions() {
    // Test asin(0) = 0
    let src = "asin(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.0).abs() < 1e-10,
        "asin(0) should be 0, got {}",
        result
    );

    // Test asin(1) = π/2
    let src = "asin(1.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::FRAC_PI_2).abs() < 1e-10,
        "asin(1) should be π/2, got {}",
        result
    );

    // Test acos(1) = 0
    let src = "acos(1.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.0).abs() < 1e-10,
        "acos(1) should be 0, got {}",
        result
    );

    // Test acos(0) = π/2
    let src = "acos(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::FRAC_PI_2).abs() < 1e-10,
        "acos(0) should be π/2, got {}",
        result
    );

    // Test atan(0) = 0
    let src = "atan(0.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.0).abs() < 1e-10,
        "atan(0) should be 0, got {}",
        result
    );

    // Test atan(1) = π/4
    let src = "atan(1.0)";
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - std::f64::consts::FRAC_PI_4).abs() < 1e-10,
        "atan(1) should be π/4, got {}",
        result
    );
}

fn test_user_defined_function_broadcast() {
    // Test user-defined function broadcast: square.(arr)
    // First, test that the basic function works
    let src = r#"
function square(x)
    return x * x
end
square(3.0)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 9.0).abs() < 1e-10,
        "square(3) should be 9, got {}",
        result
    );

    // Now test broadcast on single-element array
    let src = r#"
function square(x)
    return x * x
end

arr = [2.0]
result = square.(arr)
result[1]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 4.0).abs() < 1e-10,
        "square.([2]) should give [4], got {}",
        result
    );

    // Test with multiple elements
    let src = r#"
function square(x)
    return x * x
end

arr = [1.0, 2.0, 3.0]
result = square.(arr)
result[1] + result[2] + result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 1^2 + 2^2 + 3^2 = 1 + 4 + 9 = 14
    assert!(
        (result - 14.0).abs() < 1e-10,
        "square.([1,2,3]) should give sum 14, got {}",
        result
    );
}

fn test_complex_expression() {
    // sqrt(3^2 + 4^2) = sqrt(9+16) = sqrt(25) = 5
    let src = r#"
function f(N)
    return sqrt(3^2 + 4^2)
end
f(1)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10);
}

// ==================== Variables and Assignment ====================

fn test_variable_assignment() {
    // Note: Use for loop to avoid implicit multiplication issue
    // where "10\ny" becomes "10*y"
    let src = r#"
function f(N)
    sum = N
    for i in 1:N
        sum += i
    end
    return sum
end
f(10)
"#;
    let result = compile_and_run_str(src, 0);
    // N=10, sum = 10 + (1+2+...+10) = 10 + 55 = 65
    assert!((result - 65.0).abs() < 1e-10);
}

fn test_add_assign() {
    // Use for loop to test += without implicit mult issues
    let src = r#"
function f(N)
    cnt = N
    for i in 1:N
        cnt += 1
    end
    return cnt
end
f(5)
"#;
    let result = compile_and_run_str(src, 0);
    // N=5, cnt = 5 + 5 = 10
    assert!((result - 10.0).abs() < 1e-10);
}

// ==================== Control Flow ====================

fn test_ifelse_true() {
    // Use < comparison (N < 5 is false when N=10)
    let src = r#"
function f(N)
    return ifelse(5 < N, 100, N)
end
f(10)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 100.0).abs() < 1e-10);
}

fn test_ifelse_false() {
    // Use < comparison (5 < N is false when N=3)
    let src = r#"
function f(N)
    return ifelse(5 < N, 100, N)
end
f(3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10);
}

// ==================== Logical Operators ====================

fn test_logical_and_true() {
    // Both conditions true: 5 > 3 && 10 > 5
    let src = r#"
function f()
    if 5 > 3 && 10 > 5
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1 for true && true");
}

fn test_logical_and_false_left() {
    // Left condition false: 3 > 5 && 10 > 5 (short-circuits)
    let src = r#"
function f()
    if 3 > 5 && 10 > 5
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "Expected 0 for false && true");
}

fn test_logical_and_false_right() {
    // Right condition false: 5 > 3 && 5 > 10
    let src = r#"
function f()
    if 5 > 3 && 5 > 10
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "Expected 0 for true && false");
}

fn test_logical_or_true_left() {
    // Left condition true: 5 > 3 || 5 > 10 (short-circuits)
    let src = r#"
function f()
    if 5 > 3 || 5 > 10
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1 for true || false");
}

fn test_logical_or_true_right() {
    // Right condition true: 3 > 5 || 10 > 5
    let src = r#"
function f()
    if 3 > 5 || 10 > 5
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1 for false || true");
}

fn test_logical_or_false() {
    // Both conditions false: 3 > 5 || 5 > 10
    let src = r#"
function f()
    if 3 > 5 || 5 > 10
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.0).abs() < 1e-10,
        "Expected 0 for false || false"
    );
}

fn test_logical_operators_with_equality() {
    // Test && with == operator (the original bug case)
    let src = r#"
function f()
    a = 1.0
    b = 0.0
    if a > 0.0 && b == 0.0
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1.0).abs() < 1e-10,
        "Expected 1 for a > 0.0 && b == 0.0"
    );
}

fn test_logical_and_short_circuit_no_eval() {
    let src = r#"
function f()
    if false && (1 / 0 == 0)
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 0.0).abs() < 1e-10,
        "Expected 0 for false && (1/0)"
    );
}

fn test_logical_or_short_circuit_no_eval() {
    let src = r#"
function f()
    if true || (1 / 0 == 0)
        return 1
    end
    return 0
end
f()
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1 for true || (1/0)");
}

// ==================== tmp_repros Translations ====================

fn test_tmp_repro_implicit_mult_newline() {
    let src = r#"
y = 3
result = 10
y
println(result)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "10");
}

fn test_tmp_repro_addassign_ifelse() {
    let src = r#"
cnt = 0
cnt += ifelse(1 < 2, 1, 0)
println(cnt)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "1");
}

fn test_tmp_repro_inplace_mutation_persists() {
    let src = r#"
arr = [1, 2, 3]
function f!(a)
    a[1] = 9
end
f!(arr)
println(arr[1])
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "9");
}

fn test_tmp_repro_short_circuit_and_print() {
    let src = r#"
x = 0
if false && (1 / 0 == 0)
    x = 1
end
println(x)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "0");
}

fn test_tmp_repro_while_if_assignment() {
    let src = r#"
result = 0
i = 1
while i <= 3
    if i == 2
        result = i
    end
    i += 1
end
println(result)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "2");
}

fn test_tmp_repro_test_isa_macro() {
    let src = r#"
using Test
@test isa(1, "Int64")
println("done")
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_ok(),
        "Expected @test isa(...) to succeed, got: {:?}",
        result
    );
}

fn test_tmp_repro_try_finally_no_error() {
    let src = r#"
result = 0
try
    result = 5
finally
    x = 1
end
println(result)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "5");
}

fn test_tmp_repro_if_elseif_else_prints() {
    let src = r#"
for i in 1:5
    if i == 3
        println("Fizz")
    elseif i == 5
        println("Buzz")
    else
        println(i)
    end
end
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines, vec!["1", "2", "Fizz", "4", "Buzz"]);
}

fn test_tmp_repro_addassign_ifelse_loop() {
    let src = r#"
cnt = 0
for i in 1:3
    cnt += ifelse(i > 1, 1, 0)
end
println(cnt)
"#;
    let (_, output) = compile_and_run_program_direct(src, 0);
    assert_eq!(output.trim(), "2");
}

// ==================== For Loops ====================

fn test_for_loop_sum() {
    // Sum from 1 to N
    let src = r#"
function f(N)
    sum = 0
    for i in 1:N
        sum += i
    end
    return sum
end
f(10)
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 2 + ... + 10 = 55
    assert!((result - 55.0).abs() < 1e-10);
}

fn test_for_loop_count() {
    let src = r#"
function f(N)
    cnt = 0
    for _ in 1:N
        cnt += 1
    end
    return cnt
end
f(100)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 100.0).abs() < 1e-10);
}

// ==================== Random Number Generation ====================

fn test_rand_deterministic() {
    let src = r#"
function f(N)
    return rand()
end
f(1)
"#;

    // Same seed should produce same result
    let r1 = compile_and_run_str(src, 42);
    let r2 = compile_and_run_str(src, 42);
    assert_eq!(r1, r2);

    // Different seed should (almost certainly) produce different result
    let r3 = compile_and_run_str(src, 123);
    assert_ne!(r1, r3);
}

fn test_random_seed_function() {
    // Test Random.seed!() function resets RNG
    let src = r#"
function test_seed()
    Random.seed!(42)
    a = rand()
    Random.seed!(42)
    b = rand()
    return a == b
end
ifelse(test_seed(), 1.0, 0.0)
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "Random.seed! should reset RNG to produce same sequence"
    );
}

fn test_random_seed_different_seeds() {
    // Different seeds should produce different results
    let src = r#"
function test_seed()
    Random.seed!(1)
    x = rand()
    Random.seed!(2)
    y = rand()
    return x != y
end
ifelse(test_seed(), 1.0, 0.0)
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "Different seeds should produce different values"
    );
}

fn test_random_seed_with_randn() {
    // Test Random.seed!() works with randn as well
    let src = r#"
function test_seed()
    Random.seed!(100)
    a = randn()
    Random.seed!(100)
    b = randn()
    return a == b
end
ifelse(test_seed(), 1.0, 0.0)
"#;
    let result = compile_and_run_str(src, 0);
    assert_eq!(
        result, 1.0,
        "Random.seed! should reset RNG for randn as well"
    );
}

fn test_rand_range() {
    let src = r#"
function f(N)
    return rand()
end
f(1)
"#;

    // Test multiple seeds to verify range
    for seed in 0..100 {
        let r = compile_and_run_str(src, seed);
        assert!(r >= 0.0, "rand() returned {} which is < 0", r);
        assert!(r < 1.0, "rand() returned {} which is >= 1", r);
    }
}

fn test_rand_array_1d() {
    let src = r#"
# rand(n) creates 1D array of random Float64 values
arr = rand(5)
@assert length(arr) == 5
# All values should be in [0, 1)
sum = 0.0
for i in 1:5
    @assert arr[i] >= 0.0
    @assert arr[i] < 1.0
    sum += arr[i]
end
sum
"#;
    let result = compile_and_run_str(src, 42);
    // Sum of 5 random numbers in [0,1) should be between 0 and 5
    assert!((0.0..5.0).contains(&result), "Unexpected sum: {}", result);
}

fn test_rand_array_2d() {
    let src = r#"
# rand(m, n) creates 2D array of random Float64 values
mat = rand(3, 4)
@assert length(mat) == 12

# Check all values are in [0, 1)
sum = 0.0
for i in 1:3
    for j in 1:4
        @assert mat[i, j] >= 0.0
        @assert mat[i, j] < 1.0
        sum += mat[i, j]
    end
end
sum
"#;
    let result = compile_and_run_str(src, 42);
    // Sum of 12 random numbers in [0,1) should be between 0 and 12
    assert!((0.0..12.0).contains(&result), "Unexpected sum: {}", result);
}

fn test_rand_array_3d() {
    let src = r#"
# rand(k, m, n) creates 3D array
arr = rand(2, 3, 4)
@assert length(arr) == 24
arr[1, 1, 1]
"#;
    let result = compile_and_run_str(src, 42);
    assert!(
        (0.0..1.0).contains(&result),
        "Value not in [0,1): {}",
        result
    );
}

fn test_rand_int_array() {
    let src = r#"
# rand(Int, n) creates array of random integers
arr = rand(Int, 5)
@assert length(arr) == 5
# All values should be non-negative integers
for i in 1:5
    @assert arr[i] >= 0
end
arr[1]
"#;
    let result = compile_and_run_str(src, 42);
    // Should be a non-negative integer
    assert!(
        result >= 0.0,
        "Expected non-negative integer, got: {}",
        result
    );
    assert!(
        (result - result.floor()).abs() < 1e-10,
        "Expected integer, got: {}",
        result
    );
}

fn test_rand_int64_array() {
    let src = r#"
# rand(Int64, m, n) creates 2D array of random integers
mat = rand(Int64, 2, 3)
@assert length(mat) == 6
mat[1, 1]
"#;
    let result = compile_and_run_str(src, 42);
    assert!(result >= 0.0, "Expected non-negative integer");
}

fn test_rand_float64_array() {
    let src = r#"
# rand(Float64, n) is equivalent to rand(n)
arr = rand(Float64, 4)
@assert length(arr) == 4
# All values should be in [0, 1)
for i in 1:4
    @assert arr[i] >= 0.0
    @assert arr[i] < 1.0
end
arr[1]
"#;
    let result = compile_and_run_str(src, 42);
    assert!(
        (0.0..1.0).contains(&result),
        "Value not in [0,1): {}",
        result
    );
}

fn test_rand_array_deterministic() {
    let src = r#"
arr = rand(3)
arr[1] + arr[2] + arr[3]
"#;
    // Same seed should produce same result
    let r1 = compile_and_run_str(src, 42);
    let r2 = compile_and_run_str(src, 42);
    assert_eq!(r1, r2, "rand arrays should be deterministic");

    // Different seed should produce different result
    let r3 = compile_and_run_str(src, 123);
    assert_ne!(r1, r3, "Different seeds should produce different results");
}

// ==================== Monte Carlo Pi Estimation ====================

fn test_monte_carlo_pi() {
    // Note: Using explicit variable assignment for ifelse result due to a known
    // issue with `cnt += ifelse(...)` inline syntax (AddAssign accumulation bug)
    let src = r#"
function f(N)
    cnt = 0
    for _ in 1:N
        x = rand()
        y = rand()
        inside = ifelse(x^2 + y^2 < 1, 1, 0)
        cnt += inside
    end
    4cnt / N
end
f(10000)
"#;
    let result = compile_and_run_str(src, 12345);

    // Pi should be approximately 3.14159...
    // With 10000 samples, we expect reasonable accuracy
    assert!(
        (result - std::f64::consts::PI).abs() < 0.1,
        "Monte Carlo pi = {}, expected ~3.14159",
        result
    );
}

fn test_monte_carlo_reproducible() {
    let src = r#"
function f(N)
    cnt = 0
    for _ in 1:N
        cnt += ifelse(rand()^2 + rand()^2 < 1, 1, 0)
    end
    4cnt / N
end
f(1000)
"#;

    // Same seed should produce identical results
    let r1 = compile_and_run_str(src, 42);
    let r2 = compile_and_run_str(src, 42);
    assert_eq!(r1, r2);
}

// Generated aggregate chunks for nextest process amortization.
#[test]
fn chunk_000() {
    test_simple_module();
    test_module_with_function();
    test_module_with_main();
    test_module_qualified_call();
    test_module_qualified_call_with_args();
    test_module_qualified_call_multiple_functions();
    test_module_qualified_call_unknown_module();
    test_module_qualified_call_unknown_function();
    test_module_qualified_alias_does_not_fall_back_to_unrelated_bare_alias_7955();
    test_using_module();
    test_import_module();
    test_using_with_qualified_call();
    test_export_statement();
    test_export_multiple_functions();
    test_qualified_call_bypasses_export();
    test_selective_import();
    test_selective_import_multiple();
}

#[test]
fn chunk_001() {
    test_non_exported_function_blocked();
    test_non_imported_function_blocked();
    test_module_function_without_using();
    test_relative_import_basic();
    test_relative_import_qualified_call();
    test_relative_import_with_export();
    test_relative_import_selective();
    test_nested_module_basic();
    test_nested_module_multiple_levels();
    test_nested_module_with_parent_function();
    test_nested_module_with_arguments();
    test_nested_module_sibling_submodules();
    test_nested_module_unknown_path();
    test_base_sqrt();
    test_base_math_functions();
    test_base_array_functions();
}

#[test]
fn chunk_002() {
    test_base_println();
    test_base_unknown_function();
    test_base_no_implicit_shadowing();
    test_base_explicit_qualified();
    test_base_both_unqualified_and_qualified();
    test_base_higher_order_functions();
    test_base_in_function();
    test_base_math_submodule();
    test_base_math_multiple_functions();
    test_base_io_submodule();
    test_base_collections_submodule();
    test_base_collections_zeros_ones();
    test_base_random_submodule();
    test_base_complex_submodule();
    test_base_iterators_submodule();
    test_base_linearalgebra_is_not_a_submodule();
    test_linearalgebra_det_smoke_8276();
    test_linearalgebra_inv_smoke_8276();
    test_linearalgebra_svd_smoke_8276();
    test_linearalgebra_eigen_smoke_8276();
}

#[test]
fn chunk_003() {
    test_base_submodule_unknown_function();
    test_base_unknown_submodule();
    test_base_parses();
    test_prelude_prod();
    test_prelude_minimum_maximum();
    test_prelude_sign();
    test_prelude_clamp();
    test_prelude_any_all();
    test_prelude_count();
    test_prelude_argmin_argmax();
    test_prelude_cumsum();
    test_statistics_mean();
    test_prelude_hypot();
    test_prelude_iseven_isodd();
    test_include_lowers_to_program_body();
    test_return_constant();
}

#[test]
fn chunk_004() {
    test_simple_multiplication();
    test_addition();
    test_division();
    test_power_of_2();
    test_sqrt();
    test_elementary_functions();
    test_elementary_functions_broadcast();
    test_inverse_trig_functions();
    test_user_defined_function_broadcast();
    test_complex_expression();
    test_variable_assignment();
    test_add_assign();
    test_ifelse_true();
    test_ifelse_false();
    test_logical_and_true();
    test_logical_and_false_left();
}

#[test]
fn chunk_005() {
    test_logical_and_false_right();
    test_logical_or_true_left();
    test_logical_or_true_right();
    test_logical_or_false();
}

#[test]
fn chunk_007() {
    test_logical_operators_with_equality();
    test_logical_and_short_circuit_no_eval();
    test_logical_or_short_circuit_no_eval();
    test_tmp_repro_implicit_mult_newline();
}

#[test]
fn chunk_008() {
    test_tmp_repro_addassign_ifelse();
    test_tmp_repro_inplace_mutation_persists();
    test_tmp_repro_short_circuit_and_print();
    test_tmp_repro_while_if_assignment();
}

#[test]
fn chunk_009() {
    test_tmp_repro_test_isa_macro();
    test_tmp_repro_try_finally_no_error();
    test_tmp_repro_if_elseif_else_prints();
    test_tmp_repro_addassign_ifelse_loop();
}

#[test]
fn chunk_006() {
    test_for_loop_sum();
    test_for_loop_count();
    test_rand_deterministic();
    test_random_seed_function();
    test_random_seed_different_seeds();
    test_random_seed_with_randn();
    test_rand_range();
    test_rand_array_1d();
    test_rand_array_2d();
    test_rand_array_3d();
    test_rand_int_array();
    test_rand_int64_array();
    test_rand_float64_array();
    test_rand_array_deterministic();
    test_monte_carlo_pi();
    test_monte_carlo_reproducible();
}
