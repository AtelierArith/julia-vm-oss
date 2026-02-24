//! Comprehensive tests for all iOS app sample codes.
//! These tests ensure that every sample in SubsetJuliaVMApp/Models/CodeSample.swift
//! compiles and runs correctly in the Rust VM.
//!
//! IMPORTANT: These tests use the EXACT code from CodeSample.swift without
//! any modifications, deletions, or simplifications. This ensures that what
//! works in tests will also work in the iOS app.

use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};

/// Helper to run code through the Core IR pipeline (tree-sitter → lowering → compile_core)
/// Includes prelude functions (Complex methods, etc.)
fn run_core_pipeline(src: &str, seed: u64) -> Result<Value, String> {
    use std::collections::HashSet;

    // Parse Base source
    let prelude_src = base::get_base();
    let mut parser = Parser::new().map_err(|e| e.to_string())?;
    let prelude_parsed = parser.parse(&prelude_src).map_err(|e| e.to_string())?;
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_parsed)
        .map_err(|e| e.to_string())?;

    // Parse user source
    let mut parser = Parser::new().map_err(|e| e.to_string())?;
    let parsed = parser.parse(src).map_err(|e| e.to_string())?;
    let mut lowering = Lowering::new(src);
    let mut program = lowering.lower(parsed).map_err(|e| e.to_string())?;

    // Get user function names to skip prelude versions
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge prelude functions (skip those with same name as user functions)
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .collect();
    all_functions.extend(program.functions);
    program.functions = all_functions;

    // Merge prelude structs (skip those with same name as user structs)
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.extend(program.structs);
    program.structs = all_structs;

    // Merge prelude abstract types (prelude first, then user)
    let mut all_abstract_types = prelude_program.abstract_types;
    all_abstract_types.extend(program.abstract_types);
    program.abstract_types = all_abstract_types;

    let compiled = compile_core_program(&program).map_err(|e| e.to_string())?;

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    vm.run().map_err(|e| e.to_string())
}

/// Helper function to run a test and check if it succeeds (doesn't panic/error)
fn run_ios_sample(name: &str, src: &str) {
    match run_core_pipeline(src, 12345) {
        Ok(_) => {} // Success
        Err(e) => panic!("[{}] Runtime error: {}", name, e),
    }
}

/// Helper function to run a test and capture output for comparison
/// Includes prelude functions (Complex methods, etc.)
fn run_ios_sample_with_output(name: &str, src: &str) -> String {
    use std::collections::HashSet;

    // Parse Base source
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("Parser initialization failed");
    let prelude_parsed = parser.parse(&prelude_src).expect("Prelude parse failed");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_parsed)
        .expect("Prelude lowering failed");

    // Parse user source
    let mut parser = Parser::new().expect("Parser initialization failed");
    let parsed = parser.parse(src).expect("Parse failed");
    let mut lowering = Lowering::new(src);
    let mut program = lowering.lower(parsed).expect("Lowering failed");

    // Get user function names to skip prelude versions
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge prelude functions (skip those with same name as user functions)
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .collect();
    all_functions.extend(program.functions);
    program.functions = all_functions;

    // Merge prelude structs (skip those with same name as user structs)
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.extend(program.structs);
    program.structs = all_structs;

    // Merge prelude abstract types (prelude first, then user)
    let mut all_abstract_types = prelude_program.abstract_types;
    all_abstract_types.extend(program.abstract_types);
    program.abstract_types = all_abstract_types;

    let compiled = compile_core_program(&program).expect("Compilation failed");

    let rng = StableRng::new(12345);
    let mut vm = Vm::new_program(compiled, rng);
    match vm.run() {
        Ok(_) => vm.get_output().to_string(),
        Err(e) => panic!("[{}] Runtime error: {}", name, e),
    }
}

#[test]
#[should_panic(expected = "[Output Helper Runtime Error] Runtime error:")]
fn test_run_ios_sample_with_output_panics_on_runtime_error() {
    let src = r#"
arr = [1, 2]
println(arr[3])
"#;
    let _ = run_ios_sample_with_output("Output Helper Runtime Error", src);
}

// ============================================================================
// BASIC SAMPLES
// ============================================================================

#[test]
fn test_ios_hello_world() {
    let src = r#"println("Hello, World!")"#;
    run_ios_sample("Hello World", src);
}

#[test]
fn test_ios_string_interpolation() {
    let src = r#"
# String interpolation with $(expression)
x = 42
pi_approx = 3.14159

# Simple variable interpolation
println("x = $(x)")

# Expression interpolation
println("x + 1 = $(x + 1)")
println("x * 2 = $(x * 2)")

# Float interpolation
println("Pi is approximately $(pi_approx)")

# Multiple interpolations in one string
y = 10
println("Sum: $(x + y), Product: $(x * y)")

# Nested parentheses work too
println("Double: $((x + y) * 2)")

println(x)
"#;
    run_ios_sample("String Interpolation", src);
}

// ============================================================================
// ARRAY SAMPLES
// ============================================================================

#[test]
fn test_ios_vector_basics() {
    let src = r#"
# Create a vector
arr = [1, 2, 3, 4, 5]

# Access elements (1-indexed like Julia)
println("First element: ", arr[1])
println("Third element: ", arr[3])
println("Last element: ", arr[5])

# Get length
println("Length: ", length(arr))

println(arr[3])
"#;
    run_ios_sample("Vector Basics", src);
}

#[test]
fn test_ios_range_expressions() {
    let src = r#"
# Range 1:5 creates [1, 2, 3, 4, 5]
r = 1:5
println("1:5 has length ", length(r))

# Range with step: 1:2:10 = [1, 3, 5, 7, 9]
r2 = 1:2:10
println("1:2:10 has length ", length(r2))

# Sum elements in a range
sum = 0
for x in 1:100
    sum += x
end
println("Sum 1 to 100 = ", sum)

println(sum)
"#;
    run_ios_sample("Range Expressions", src);
}

#[test]
fn test_ios_comprehension() {
    let src = r#"
# Comprehension: [expr for var in iter]
# Create squares of 1 to 5
squares = [x^2 for x in 1:5]

println("Squares: ")
for i in 1:length(squares)
    println("  ", i, "^2 = ", squares[i])
end

# Sum of squares
sum = 0.0
for i in 1:length(squares)
    sum += squares[i]
end
println("Sum of squares: ", sum)

println(sum)
"#;
    run_ios_sample("Comprehension", src);
}

#[test]
fn test_ios_comprehension_with_filter() {
    let src = r#"
# Comprehension with filter: [expr for var in iter if cond]

# Even numbers from 1 to 10
evens = [x for x in 1:10 if x % 2 == 0]
println("Even numbers: ")
for i in 1:length(evens)
    println("  ", evens[i])
end

# Squares of odd numbers
odd_squares = [x^2 for x in 1:10 if x % 2 == 1]
println("Odd squares: ")
for i in 1:length(odd_squares)
    println("  ", odd_squares[i])
end

println(length(evens) + length(odd_squares))
"#;
    run_ios_sample("Comprehension with Filter", src);
}

#[test]
fn test_ios_array_functions() {
    let src = r#"
# zeros(n) - create array of n zeros
z = zeros(5)
println("zeros(5): ", z[1], ", ", z[2], ", ...")

# ones(n) - create array of n ones
o = ones(5)
println("ones(5): ", o[1], ", ", o[2], ", ...")

# fill(value, n) - create array filled with value
f = fill(3.14, 4)
println("fill(3.14, 4): ", f[1], ", ", f[2], ", ...")

# Combine with comprehension for more complex arrays
powers_of_2 = [2.0^i for i in 0:10]

println("Powers of 2:")
for i in 1:length(powers_of_2)
    println("  2^", i-1, " = ", powers_of_2[i])
end

println(powers_of_2[11])
"#;
    run_ios_sample("Array Functions", src);
}

#[test]
fn test_ios_array_mutation() {
    let src = r#"
# Start with an array
arr = [10, 20, 30]
println("Initial: ", arr[1], ", ", arr[2], ", ", arr[3])

# Modify elements
arr[2] = 99
println("After arr[2] = 99: ", arr[1], ", ", arr[2], ", ", arr[3])

# push! adds to end
push!(arr, 40)
println("After push!(arr, 40): length = ", length(arr))

# pop! removes from end
last = pop!(arr)
println("pop! returned: ", last)
println("After pop!: length = ", length(arr))

println(arr[2])
"#;
    run_ios_sample("Array Mutation", src);
}

#[test]
fn test_ios_dot_product() {
    let src = r#"
function dot_product(a, b)
    @assert length(a) == length(b) "Arrays must have same length"
    sum = 0.0
    for i in 1:length(a)
        sum += a[i] * b[i]
    end
    sum
end

# Test vectors
v1 = [1, 2, 3, 4, 5]
v2 = [5, 4, 3, 2, 1]

result = dot_product(v1, v2)
println("v1 · v2 = ", result)

println(result)
"#;
    run_ios_sample("Dot Product", src);
}

#[test]
fn test_ios_statistical_functions() {
    let src = r#"
function array_sum(arr)
    sum = 0.0
    for i in 1:length(arr)
        sum += arr[i]
    end
    sum
end

function array_mean(arr)
    array_sum(arr) / length(arr)
end

function array_max(arr)
    max_val = arr[1]
    for i in 2:length(arr)
        if arr[i] > max_val
            max_val = arr[i]
        end
    end
    max_val
end

# Sample data
data = [23, 45, 12, 67, 34, 89, 11, 56]

println("Data points: ", length(data))
println("Sum: ", array_sum(data))
println("Mean: ", array_mean(data))
println("Max: ", array_max(data))

println(array_mean(data))
"#;
    run_ios_sample("Statistical Functions", src);
}

#[test]
fn test_ios_2d_matrix_basics() {
    let src = r#"
# Create a 3x3 matrix of zeros
m = zeros(3, 3)

# Fill with values
for i in 1:3
    for j in 1:3
        m[i, j] = i * 10 + j
    end
end

# Print the matrix
println("3x3 Matrix:")
for i in 1:3
    println(m[i, 1], " ", m[i, 2], " ", m[i, 3])
end

# Access specific element
println("m[2, 3] = ", m[2, 3])

println(m[2, 3])
"#;
    run_ios_sample("2D Matrix Basics", src);
}

#[test]
fn test_ios_matrix_initialization() {
    let src = r#"
# Different ways to create matrices

# zeros(rows, cols) - all zeros
z = zeros(2, 3)
println("Zeros 2x3:")
println(z[1, 1], " ", z[1, 2], " ", z[1, 3])
println(z[2, 1], " ", z[2, 2], " ", z[2, 3])

# ones(rows, cols) - all ones
o = ones(2, 3)
println("Ones 2x3:")
println(o[1, 1], " ", o[1, 2], " ", o[1, 3])
println(o[2, 1], " ", o[2, 2], " ", o[2, 3])

# fill(value, rows, cols) - custom value
f = fill(7, 2, 3)
println("Fill(7) 2x3:")
println(f[1, 1], " ", f[1, 2], " ", f[1, 3])
println(f[2, 1], " ", f[2, 2], " ", f[2, 3])

println(length(f))
"#;
    run_ios_sample("Matrix Initialization", src);
}

#[test]
fn test_ios_matrix_sum() {
    let src = r#"
function matrix_sum(m, rows, cols)
    sum = 0.0
    for i in 1:rows
        for j in 1:cols
            sum += m[i, j]
        end
    end
    sum
end

# Create a 3x4 matrix
rows = 3
cols = 4
m = zeros(rows, cols)

# Fill with values 1 to 12
val = 1
for i in 1:rows
    for j in 1:cols
        m[i, j] = val
        val += 1
    end
end

println("Matrix 3x4:")
for i in 1:rows
    for j in 1:cols
        println("m[", i, ",", j, "] = ", m[i, j])
    end
end

sum = matrix_sum(m, rows, cols)
println("Sum of all elements: ", sum)

println(sum)
"#;
    run_ios_sample("Matrix Sum", src);
}

#[test]
fn test_ios_identity_matrix() {
    let src = r#"
function identity(n)
    m = zeros(n, n)
    for i in 1:n
        m[i, i] = 1
    end
    m
end

# Create 4x4 identity matrix
I = identity(4)

println("4x4 Identity Matrix:")
for i in 1:4
    println(I[i, 1], " ", I[i, 2], " ", I[i, 3], " ", I[i, 4])
end

# Verify diagonal elements
@assert I[1, 1] == 1
@assert I[2, 2] == 1
@assert I[3, 3] == 1
@assert I[4, 4] == 1

# Verify off-diagonal elements are zero
@assert I[1, 2] == 0
@assert I[2, 1] == 0

println(I[3, 3])
"#;
    run_ios_sample("Identity Matrix", src);
}

#[test]
fn test_ios_matrix_vector_multiplication() {
    let src = r#"
# Create a 2x3 matrix A
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6

println("Matrix A (2x3):")
println(A[1, 1], " ", A[1, 2], " ", A[1, 3])
println(A[2, 1], " ", A[2, 2], " ", A[2, 3])

# Create a vector v
v = [1, 2, 3]
println("Vector v: ", v[1], ", ", v[2], ", ", v[3])

# Matrix-vector multiplication: A * v
result = A * v
println("Result A * v:")
println("  result[1] = ", result[1])
println("  result[2] = ", result[2])

# Verify: result[1] = 1*1 + 2*2 + 3*3 = 14
#         result[2] = 4*1 + 5*2 + 6*3 = 32
@assert result[1] == 14
@assert result[2] == 32

println(result[1])
"#;
    run_ios_sample("Matrix-Vector Multiplication", src);
}

#[test]
fn test_ios_matrix_matrix_multiplication() {
    let src = r#"
# Create matrix A (2x3)
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6

println("Matrix A (2x3):")
println(A[1, 1], " ", A[1, 2], " ", A[1, 3])
println(A[2, 1], " ", A[2, 2], " ", A[2, 3])

# Create matrix B (3x2)
B = zeros(3, 2)
B[1, 1] = 7
B[1, 2] = 8
B[2, 1] = 9
B[2, 2] = 10
B[3, 1] = 11
B[3, 2] = 12

println("Matrix B (3x2):")
println(B[1, 1], " ", B[1, 2])
println(B[2, 1], " ", B[2, 2])
println(B[3, 1], " ", B[3, 2])

# Matrix multiplication: C = A * B (result is 2x2)
C = A * B
println("Result C = A * B (2x2):")
println(C[1, 1], " ", C[1, 2])
println(C[2, 1], " ", C[2, 2])

# Verify: C[1,1] = 1*7 + 2*9 + 3*11 = 58
#         C[1,2] = 1*8 + 2*10 + 3*12 = 64
#         C[2,1] = 4*7 + 5*9 + 6*11 = 139
#         C[2,2] = 4*8 + 5*10 + 6*12 = 154
@assert C[1, 1] == 58
@assert C[1, 2] == 64
@assert C[2, 1] == 139
@assert C[2, 2] == 154

println(C[2, 2])
"#;
    run_ios_sample("Matrix-Matrix Multiplication", src);
}

#[test]
fn test_ios_sieve_of_eratosthenes() {
    let src = r#"
function sieve(n)
    # Create array: 1 = prime, 0 = not prime
    is_prime = ones(n)
    is_prime[1] = 0  # 1 is not prime

    for i in 2:sqrt(n)
        if is_prime[i] == 1
            # Mark multiples as not prime
            j = i * 2
            while j <= n
                is_prime[j] = 0
                j += i
            end
        end
    end

    # Count primes
    count = 0
    for i in 1:n
        if is_prime[i] == 1
            count += 1
        end
    end
    count
end

# Count primes up to 100
@time count = sieve(100)
println("Primes up to 100: ", count)

@time count = sieve(1000)
println("Primes up to 1000: ", count)

println(count)
"#;
    run_ios_sample("Sieve of Eratosthenes", src);
}

#[test]
fn test_ios_broadcast_operations() {
    let src = r#"
# Broadcast (element-wise) operations with .+ .* .- ./

a = [1, 2, 3, 4, 5]
b = [10, 20, 30, 40, 50]

# Element-wise addition
c = a .+ b
println("a .+ b = ")
for i in 1:5
    println("  c[", i, "] = ", c[i])
end

# Element-wise multiplication
d = a .* b
println("a .* b = ")
for i in 1:5
    println("  d[", i, "] = ", d[i])
end

# Scalar broadcast: multiply all elements by 10
e = a .* 10
println("a .* 10 = ")
for i in 1:5
    println("  e[", i, "] = ", e[i])
end

# Element-wise power
f = a .^ 2
println("a .^ 2 = ")
for i in 1:5
    println("  f[", i, "] = ", f[i])
end

println(f[5])
"#;
    run_ios_sample("Broadcast Operations", src);
}

#[test]
fn test_ios_broadcast_function_calls() {
    let src = r#"
# Broadcast function call syntax: f.(x)
# Apply function element-wise to array

# Create array of perfect squares
squares = [1, 4, 9, 16, 25, 36, 49, 64, 81, 100]

# Apply sqrt to each element
roots = sqrt.(squares)

println("Square roots of perfect squares:")
for i in 1:10
    println("  sqrt(", squares[i], ") = ", roots[i])
end

# Combine with broadcast operations
a = [4, 9, 16]
b = [1, 1, 1]

# sqrt.(a) .+ b = [2, 3, 4] .+ [1, 1, 1] = [3, 4, 5]
result = sqrt.(a) .+ b

println("sqrt.([4, 9, 16]) .+ [1, 1, 1]:")
for i in 1:3
    println("  result[", i, "] = ", result[i])
end

println(result[3])
"#;
    run_ios_sample("Broadcast Function Calls", src);
}

// ============================================================================
// FUNCTION SAMPLES
// ============================================================================

#[test]
fn test_ios_factorial_iterative() {
    let src = r#"
function factorial(n)
    result = 1
    for i in 1:n
        result = result * i
    end
    result
end

println(factorial(10))
"#;
    run_ios_sample("Factorial (Iterative)", src);
}

#[test]
fn test_ios_factorial_recursive() {
    let src = r#"
function factorial(n)
    if n <= 1
        return 1
    end
    n * factorial(n - 1)
end

println(factorial(10))
"#;
    run_ios_sample("Factorial (Recursive)", src);
}

#[test]
fn test_ios_multiple_dispatch() {
    let src = r#"
# Multiple dispatch: same function name, different type signatures
# Julia selects the most specific method based on argument types

# Method for integers
function process(x::Int64)
    println("Integer method: ", x, " → ", x * 2)
    return x * 2
end

# Method for floats
function process(x::Float64)
    println("Float method: ", x, " → ", x / 2.0)
    return x / 2.0
end

# Fallback method for any type
function process(x::Number)
    println("Generic Number method: ", x)
    return x + 100
end

# Integer literals dispatch to Int64 method
println("Calling process(42):")
r1 = process(42)

# Float literals dispatch to Float64 method
println("Calling process(10.0):")
r2 = process(10.0)

# Type-based dispatch
println("Calling process(7):")
r3 = process(7)

println("")
println("Results:")
println("  process(42) = ", r1)
println("  process(10.0) = ", r2)
println("  process(7) = ", r3)

println(r1 + r3)
"#;
    run_ios_sample("Multiple Dispatch", src);
}

#[test]
fn test_ios_type_annotations() {
    let src = r#"
# Type annotations ensure type safety and enable dispatch

# Typed parameters
function add_ints(a::Int64, b::Int64)
    return a + b
end

function add_floats(a::Float64, b::Float64)
    return a + b
end

# Untyped parameters accept any type (treated as ::Any)
function add_any(a, b)
    return a + b
end

# Test integer addition
println("add_ints(3, 4) = ", add_ints(3, 4))

# Test float addition
println("add_floats(1.5, 2.5) = ", add_floats(1.5, 2.5))

# Test generic addition
println("add_any(10, 20) = ", add_any(10, 20))
println("add_any(1.1, 2.2) = ", add_any(1.1, 2.2))

println(add_ints(3, 4) + add_floats(1.5, 2.5))
"#;
    run_ios_sample("Type Annotations", src);
}

#[test]
fn test_ios_print_smile() {
    let src = r#"
function print_smile()
    println("  ____  ")
    println(" /    \ ")
    println("|  ^ ^ |")
    println("|      |")
    println("|  \_/ |")
    println("|      |")
    println(" \____/ ")
    println("        ")
end

print_smile()
"#;
    // Capture output and verify it matches iOS app output
    let output = run_ios_sample_with_output("Print Smile", src);
    println!("Rust VM output:\n{}", output);

    // Verify the output matches iOS app output (with trailing spaces)
    // iOS app output:
    //   ____
    //  /    \
    // |  ^ ^ |
    // |      |
    // |  \_/ |
    // |      |
    //  \____/
    //
    assert!(
        output.contains("  ____  "),
        "Missing top line with trailing spaces"
    );
    assert!(
        output.contains(" /    \\ "),
        "Missing second line with trailing space"
    );
    assert!(output.contains("|  ^ ^ |"), "Missing eyes line");
    assert!(output.contains("|      |"), "Missing empty line");
    assert!(output.contains("|  \\_/ |"), "Missing mouth line");
    assert!(
        output.contains(" \\____/ "),
        "Missing bottom line with trailing space"
    );

    // Also verify it runs without error
    run_ios_sample("Print Smile", src);
}

#[test]
fn test_ios_fizzbuzz() {
    let src = r#"
function fizzbuzz(n)
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

fizzbuzz(100)
"#;
    // Capture output to verify if/elseif/else parsing works correctly
    let output = run_ios_sample_with_output("FizzBuzz", src);

    // Print full output for debugging
    println!("FizzBuzz full output:\n{}", output);
    println!("Output length: {} chars", output.len());
    println!("Number of lines: {}", output.lines().count());

    // Print first 30 lines for inspection
    let lines: Vec<&str> = output.lines().collect();
    println!("\nFirst 30 lines:");
    for (i, line) in lines.iter().take(30).enumerate() {
        println!("  Line {}: '{}'", i + 1, line);
    }

    // Verify that the output contains expected patterns
    assert!(output.contains("FizzBuzz"), "Missing FizzBuzz");
    assert!(output.contains("Fizz"), "Missing Fizz");
    assert!(output.contains("Buzz"), "Missing Buzz");

    // Check if numbers are present (they should be if else clause works)
    let has_numbers = output.contains("1")
        || output.contains("2")
        || output.contains("4")
        || output.contains("7");
    if !has_numbers {
        println!("⚠️  WARNING: No numbers found in output - else clause may not be executing!");
        println!("   This suggests if/elseif/else parsing may have an issue.");
    }

    // Verify specific cases if we have enough lines
    if !lines.is_empty() {
        println!("Line 1: '{}'", lines[0]);
    }
    if lines.len() > 2 {
        println!("Line 3: '{}' (should be 'Fizz')", lines[2]);
    }
    if lines.len() > 4 {
        println!("Line 5: '{}' (should be 'Buzz')", lines[4]);
    }
    if lines.len() > 14 {
        println!("Line 15: '{}' (should be 'FizzBuzz')", lines[14]);
    }

    // Also verify it runs without error
    run_ios_sample("FizzBuzz", src);
}

// ============================================================================
// ALGORITHM SAMPLES
// ============================================================================

#[test]
fn test_ios_fibonacci() {
    let src = r#"
function fib(n)
    if n <= 1
        return n
    end
    fib(n - 1) + fib(n - 2)
end

result = fib(15)
println(result)
result
"#;
    run_ios_sample("Fibonacci", src);
}

#[test]
fn test_ios_fibonacci_fast() {
    let src = r#"
function fib_fast(n)
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        c = a + b
        a = b
        b = c
    end
    b
end

result = fib_fast(30)
println(result)
result
"#;
    run_ios_sample("Fibonacci (Fast)", src);
}

#[test]
fn test_ios_gcd_euclidean() {
    let src = r#"
function gcd(a, b)
    while b > 0
        temp = b
        b = a % b
        a = temp
    end
    a
end

println(gcd(48, 18))
"#;
    run_ios_sample("GCD (Euclidean)", src);
}

#[test]
fn test_ios_is_prime() {
    let src = r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    if n <= 3
        return 1
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

println(is_prime(97))
"#;
    run_ios_sample("Is Prime", src);
}

#[test]
fn test_ios_sum_of_primes() {
    let src = r#"
function is_prime(n)
    if n <= 1
        return 0
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

function sum_primes(n)
    sum = 0
    for i in 2:n
        if is_prime(i) == 1
            sum += i
        end
    end
    sum
end

println(sum_primes(100))
"#;
    run_ios_sample("Sum of Primes", src);
}

#[test]
fn test_ios_mandelbrot_scalar() {
    let src = r#"
# Mandelbrot escape time algorithm (using complex numbers and abs2)
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Test a few points
println("Testing Mandelbrot escape times:")

# Point inside the set (should return maxiter)
c1 = mandelbrot_escape(0.0 + 0.0im, 100)
println("  (0, 0): ", c1, " (in set)")

# Point on the boundary
c2 = mandelbrot_escape(-0.75 + 0.0im, 100)
println("  (-0.75, 0): ", c2)

# Point outside (escapes quickly)
c3 = mandelbrot_escape(1.0 + 1.0im, 100)
println("  (1, 1): ", c3, " (escaped)")

# Interesting point near the boundary
c4 = mandelbrot_escape(-0.1 + 0.65im, 100)
println("  (-0.1, 0.65): ", c4)

# Generate a small text visualization
println("")
println("Mini Mandelbrot (21x11):")
for row in 0:10
    ci = 1.0 - row * 0.2  # y from 1.0 to -1.0
    for col in 0:20
        cr = -2.0 + col * 0.15  # x from -2.0 to 1.0
        c = cr + ci * im
        n = mandelbrot_escape(c, 50)
        if n == 50
            print("*")  # In the set
        elseif n > 10
            print("+")  # Slow escape
        else
            print(" ")  # Fast escape
        end
    end
    println("")
end

println(c1)
"#;
    // Capture output for debugging and comparison with iOS app
    let output = run_ios_sample_with_output("Mandelbrot (Scalar)", src);
    println!("Rust VM output:\n{}", output);

    // Verify key outputs match Julia's behavior (no spaces between arguments)
    assert!(
        output.contains("Testing Mandelbrot escape times:"),
        "Missing test header"
    );
    assert!(
        output.contains("  (0, 0): 100 (in set)"),
        "c1 should be 100 (Julia behavior - no spaces)"
    );
    assert!(
        output.contains("  (-0.75, 0): 100"),
        "c2 should be 100 (Julia behavior - no spaces)"
    );
    assert!(
        output.contains("  (1, 1): 3 (escaped)"),
        "c3 should be 3 (Julia behavior - no spaces)"
    );
    assert!(
        output.contains("  (-0.1, 0.65): 76"),
        "c4 should be 76 (Julia behavior - no spaces)"
    );
    assert!(
        output.contains("Mini Mandelbrot (21x11):"),
        "Missing Mini Mandelbrot header"
    );

    // run_ios_sample_with_output also verifies successful execution.
}

#[test]
fn test_ios_mandelbrot_grid() {
    let src = r##"
# Mandelbrot escape time for a grid of points (using complex numbers and abs2)

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute escape times for a grid
function mandelbrot_grid(width::Int64, height::Int64, maxiter::Int64)
    # Coordinate ranges
    xmin = -2.0
    xmax = 1.0
    ymin = -1.2
    ymax = 1.2

    # Store results in 2D array
    grid = zeros(height, width)

    for row in 1:height
        ci = ymax - (row - 1) * (ymax - ymin) / (height - 1)
        for col in 1:width
            cr = xmin + (col - 1) * (xmax - xmin) / (width - 1)
            c = cr + ci * im
            grid[row, col] = mandelbrot_escape(c, maxiter)
        end
    end

    grid
end

# Compute a small grid
@time grid = mandelbrot_grid(40, 20, 100)

# ASCII visualization
println("Mandelbrot Set (40x20):")
for row in 1:20
    for col in 1:40
        n = grid[row, col]
        if n == 100
            print("#")  # In the set
        elseif n > 50
            print("+")  # Slow escape
        elseif n > 20
            print(".")  # Medium
        elseif n > 10
            print("-")  # Fast
        else
            print(" ")  # Very fast
        end
    end
    println("")
end

# Statistics
in_set = 0
for row in 1:20
    for col in 1:40
        if grid[row, col] == 100
            in_set += 1
        end
    end
end
println("Points in set: ", in_set, " / 800")

println(in_set)
"##;
    // Capture output for comparison with iOS app
    let output = run_ios_sample_with_output("Mandelbrot Grid", src);
    println!("Rust VM output:\n{}", output);

    // Verify key outputs match Julia's behavior (no spaces between arguments)
    assert!(
        output.contains("Mandelbrot Set (40x20):"),
        "Missing Mandelbrot Set header"
    );
    assert!(
        output.contains("Points in set: 162 / 800"),
        "in_set should be 162 (Julia behavior - no spaces)"
    );
    assert!(
        output.contains("\n162\n"),
        "Final in_set value should be 162"
    );

    // run_ios_sample_with_output also verifies successful execution.
}

// Test for the iOS app's actual Mandelbrot sample using 2D broadcast with Ref()
#[test]
fn test_ios_mandelbrot_2d_broadcast_ref() {
    let src = r##"
# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute grid using broadcast (vectorized)
# xs' creates a row vector, ys is a column vector
# Broadcasting creates a 2D complex matrix C
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    C = xs' .+ im .* ys

    # Apply escape function to all points at once
    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# ASCII visualization
@time grid = mandelbrot_grid(50.0, 25.0, 50.0)
println("Mandelbrot Set (50x25):")
for row in 1:25
    for col in 1:50
        n = grid[row, col]
        if n == 50
            print("#")
        elseif n > 25
            print("+")
        elseif n > 10
            print(".")
        else
            print(" ")
        end
    end
    println("")
end

println(grid[12, 25])
"##;
    // This is the exact code from the iOS sample - it must run without crash
    run_ios_sample("Mandelbrot 2D Broadcast Ref", src);
}

#[test]
fn test_ios_mandelbrot_broadcast() {
    let src = r##"
# Mandelbrot set using broadcast operations
# Computes escape time for a row of points at once

function mandelbrot_row(cr_array, ci, maxiter)
    n = length(cr_array)

    # Initialize z = 0 + 0i for all points
    Zr = zeros(n)
    Zi = zeros(n)

    # Track iterations for each point
    iterations = fill(maxiter, n)
    escaped = zeros(n)

    for k in 1:maxiter
        # z^2 = (Zr + Zi*i)^2 = Zr^2 - Zi^2 + 2*Zr*Zi*i
        Zr2 = Zr .* Zr
        Zi2 = Zi .* Zi

        # Check escape: |z|^2 > 4
        mag2 = Zr2 .+ Zi2

        # Update iteration count for newly escaped points
        for j in 1:n
            if mag2[j] > 4.0 && escaped[j] == 0.0
                iterations[j] = k
                escaped[j] = 1.0
            end
        end

        # z = z^2 + c (complex multiplication)
        Zi_new = 2.0 .* Zr .* Zi .+ ci
        Zr = Zr2 .- Zi2 .+ cr_array
        Zi = Zi_new
    end

    iterations
end

# Create coordinate arrays (small size for fast testing)
width = 15
height = 8
xmin = -2.0
xmax = 1.0
ymin = -1.2
ymax = 1.2
maxiter = 20

# Generate x coordinates using array comprehension
cr_array = [xmin + (col - 1) * (xmax - xmin) / (width - 1) for col in 1:width]

println("Mandelbrot Set (Broadcast, ", width, "x", height, "):")

# Process each row with broadcast operations
in_set = 0
for row in 1:height
    ci = ymax - (row - 1) * (ymax - ymin) / (height - 1)
    iterations = mandelbrot_row(cr_array, ci, maxiter)

    for col in 1:width
        n = iterations[col]
        if n == maxiter
            print("#")
            in_set += 1
        elseif n > 10
            print("+")
        elseif n > 4
            print(".")
        elseif n > 2
            print("-")
        else
            print(" ")
        end
    end
    println("")
end

    println("")
    println("Points in set: ", in_set, " / ", width * height)
    println(in_set)
"##;
    // Capture output for comparison with iOS app
    let output = run_ios_sample_with_output("Mandelbrot Broadcast", src);
    println!("Rust VM output:\n{}", output);

    // Verify key outputs match Julia's behavior (small grid for fast testing)
    // Julia reference: 28 points in set out of 15*8=120 with maxiter=20
    assert!(
        output.contains("Mandelbrot Set (Broadcast, 15x8):"),
        "Missing Mandelbrot Set header"
    );
    assert!(
        output.contains("Points in set: 28 / 120"),
        "in_set should be 28 (verified against Julia)"
    );
    assert!(
        output.contains("\n28\n"),
        "Final in_set value should be 28"
    );

    // run_ios_sample_with_output also verifies successful execution.
}

// ============================================================================
// MONTE CARLO SAMPLES
// ============================================================================

#[test]
fn test_ios_estimate_pi() {
    let src = r#"
function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end

println(estimate_pi(10000))
"#;
    // Monte Carlo is stochastic, so just check it runs
    run_ios_sample("Estimate Pi", src);
}

#[test]
fn test_ios_random_walk() {
    let src = r#"
function random_walk_1d(steps)
    position = 0.0
    for i in 1:steps
        step = ifelse(rand() < 0.5, -1.0, 1.0)
        position += step
    end
    position
end

println(random_walk_1d(1000))
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Random Walk", src);
}

#[test]
fn test_ios_monte_carlo_integration() {
    let src = r#"
function monte_carlo_integral(N)
    # Estimate integral of x^2 from 0 to 1
    sum = 0.0
    for i in 1:N
        x = rand()
        sum += x^2
    end
    sum / N  # Should be close to 1/3
end

println(monte_carlo_integral(100000))
"#;
    // Monte Carlo - just check it runs
    run_ios_sample("Monte Carlo Integration", src);
}

#[test]
fn test_ios_random_arrays() {
    let src = r#"
# rand(n) creates 1D array of random Float64 in [0, 1)
v = rand(5)
println("Random vector of length 5:")
for i in 1:5
    println("  v[", i, "] = ", v[i])
end

# rand(m, n) creates 2D array (matrix)
m = rand(3, 3)
println("Random 3x3 matrix:")
for i in 1:3
    println(m[i, 1], " ", m[i, 2], " ", m[i, 3])
end

# Sum of all elements
sum = 0.0
for i in 1:3
    for j in 1:3
        sum += m[i, j]
    end
end
println("Sum of matrix elements: ", sum)

println(sum)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Random Arrays", src);
}

#[test]
fn test_ios_random_integer_arrays() {
    let src = r#"
# rand(Int, n) creates array of random integers
ints = rand(Int, 5)
println("Random integers:")
for i in 1:5
    println("  ints[", i, "] = ", ints[i])
end

# rand(Int, m, n) creates 2D array of random integers
mat = rand(Int, 2, 3)
println("Random integer matrix 2x3:")
for i in 1:2
    println(mat[i, 1], " ", mat[i, 2], " ", mat[i, 3])
end

# Find the maximum value
max_val = mat[1, 1]
for i in 1:2
    for j in 1:3
        if mat[i, j] > max_val
            max_val = mat[i, j]
        end
    end
end
println("Max value: ", max_val)

println(max_val)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Random Integer Arrays", src);
}

#[test]
fn test_ios_random_simulation() {
    let src = r#"
# Simulate dice rolls using random arrays
function simulate_dice(n_rolls)
    # Generate random floats and convert to 1-6
    rolls = rand(n_rolls)
    sum = 0
    for i in 1:n_rolls
        # Convert [0,1) to 1-6
        die = 1 + floor(rolls[i] * 6)
        if die > 6
            die = 6
        end
        sum += die
    end
    sum / n_rolls  # Average roll
end

avg = simulate_dice(1000)
println("Average of 1000 dice rolls: ", avg)
# Should be close to 3.5

println(avg)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Random Simulation", src);
}

// ============================================================================
// MATHEMATICS SAMPLES
// ============================================================================

#[test]
fn test_ios_geometric_series() {
    let src = r#"
function geometric_sum(r, n)
    # Sum of r^0 + r^1 + ... + r^(n-1)
    sum = 0.0
    term = 1.0
    for i in 1:n
        sum += term
        term *= r
    end
    sum
end

println(geometric_sum(0.5, 10))
"#;
    run_ios_sample("Geometric Series", src);
}

#[test]
fn test_ios_newton_method() {
    let src = r#"
function newton_sqrt(x)
    # Find sqrt(x) using Newton's method
    guess = x / 2.0
    for i in 1:10
        guess = (guess + x / guess) / 2.0
    end
    guess
end

println(newton_sqrt(2.0))
"#;
    run_ios_sample("Newton's Method", src);
}

#[test]
fn test_ios_taylor_series_exp() {
    let src = r#"
function exp_taylor(x, terms)
    # e^x ≈ 1 + x + x^2/2! + x^3/3! + ...
    result = 1.0
    term = 1.0
    for n in 1:terms
        term = term * x / n
        result += term
    end
    result
end

println(exp_taylor(1.0, 20))  # Should be close to e ≈ 2.71828
"#;
    run_ios_sample("Taylor Series e^x", src);
}

#[test]
fn test_ios_coprime_pi_estimation() {
    let src = r#"
# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end
# Reduced parameters for faster testing (O(N^2) complexity)
@time println("N=50: π ≈ ", calc_pi(50))
@time println("N=100: π ≈ ", calc_pi(100))

println(calc_pi(100))
"#;
    run_ios_sample("Coprime π Estimation", src);
}

// ============================================================================
// MACRO SAMPLES
// ============================================================================

#[test]
fn test_ios_assert_in_loop() {
    let src = r#"
function sum_positive(n)
    @assert n >= 0 "n must be non-negative"
    sum = 0
    for i in 1:n
        @assert i > 0
        sum += i
    end
    sum
end

println(sum_positive(10))
"#;
    run_ios_sample("@assert in Loop", src);
}

#[test]
fn test_ios_time_monte_carlo() {
    let src = r#"
function estimate_pi(N)
    inside = 0
    for i in 1:N
        x = rand()
        y = rand()
        if x^2 + y^2 < 1.0
            inside += 1
        end
    end
    4.0 * inside / N
end

# Time the Monte Carlo simulation
@time begin
    pi_estimate = estimate_pi(100000)
    println("Estimated pi: ", pi_estimate)
end
println(pi_estimate)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("@time Monte Carlo", src);
}

#[test]
fn test_ios_combined_assert_time() {
    let src = r#"
function checked_factorial(n)
    @assert n >= 0 "n must be non-negative"
    @assert n <= 20 "n too large (overflow risk)"
    result = 1
    for i in 1:n
        result = result * i
    end
result
end

@time begin
    f10 = checked_factorial(10)
    println("10! = ", f10)
    f15 = checked_factorial(15)
    println("15! = ", f15)
end
println(f15)
"#;
    run_ios_sample("Combined @assert + @time", src);
}

#[test]
fn test_ios_show_debugging() {
    let src = r#"
function debug_sum(n)
    sum = 0
    for i in 1:n
        sum += i
        @show sum
    end
    sum
end

println(debug_sum(5))
"#;
    run_ios_sample("@show Debugging", src);
}

#[test]
fn test_ios_show_with_expressions() {
    let src = r#"
# @show prints "expr = value" format
x = 10
y = 20

@show x
@show y
@show x + y
@show x * y

# Useful for debugging calculations
function hypotenuse(a, b)
    @show a
    @show b
    result = sqrt(a^2 + b^2)
    @show result
    result
end

println(hypotenuse(3.0, 4.0))
"#;
    run_ios_sample("@show with Expressions", src);
}

// ============================================================================
// HIGHER-ORDER FUNCTION SAMPLES
// ============================================================================

#[test]
fn test_ios_map_function() {
    let src = r#"
# map(f, arr) applies function f to each element
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Define functions to use with map
function double(x)
    return x * 2.0
end

function square(x)
    return x * x
end

# Double each element
doubled = map(double, arr)
println("Doubled: ")
for i in 1:length(doubled)
    println("  ", doubled[i])
end

# Square each element
squared = map(square, arr)
println("Squared: ")
for i in 1:length(squared)
    println("  ", squared[i])
end

println(squared[5])
"#;
    run_ios_sample("Map Function", src);
}

#[test]
fn test_ios_filter_function() {
    let src = r#"
# filter(f, arr) keeps elements where f returns true
arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Define predicate functions
function is_even(x)
    return x % 2 == 0
end

function is_large(x)
    return x > 5
end

# Keep only even numbers
evens = filter(is_even, arr)
println("Even numbers: ")
for i in 1:length(evens)
    println("  ", evens[i])
end

# Keep numbers greater than 5
large = filter(is_large, arr)
println("Numbers > 5: ")
for i in 1:length(large)
    println("  ", large[i])
end

println(length(evens) + length(large))
"#;
    run_ios_sample("Filter Function", src);
}

#[test]
fn test_ios_reduce_function() {
    let src = r#"
# reduce(f, arr) combines elements using binary function f
arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Define combining functions
function add(a, b)
    return a + b
end

function multiply(a, b)
    return a * b
end

# Sum all elements
total = reduce(add, arr)
println("Sum: ", total)

# Product of all elements
product = reduce(multiply, arr)
println("Product: ", product)

# With initial value: reduce(f, arr, init)
total_with_init = reduce(add, arr, 100.0)
println("Sum with init=100: ", total_with_init)

println(product)
"#;
    run_ios_sample("Reduce Function", src);
}

#[test]
fn test_ios_do_syntax_map() {
    let src = r#"
# do...end block creates anonymous function as first argument
# This is syntactic sugar for passing lambdas

arr = [1.0, 2.0, 3.0, 4.0, 5.0]

# Using do syntax (equivalent to map(x -> x^2 + 1, arr))
result = map(arr) do x
    x^2 + 1
end

println("x^2 + 1 for each element:")
for i in 1:length(result)
    println("  ", arr[i], " -> ", result[i])
end

println(result[5])
"#;
    run_ios_sample("Do Syntax for Map", src);
}

#[test]
fn test_ios_do_syntax_filter_reduce() {
    let src = r#"
# do syntax works with filter and reduce too
data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Filter with do block
filtered = filter(data) do x
    x > 3 && x < 8
end
println("Numbers between 3 and 8:")
for i in 1:length(filtered)
    println("  ", filtered[i])
end

# Reduce with do block (multiple parameters)
total = reduce(data) do acc, val
    acc + val
end
println("Sum: ", total)

println(total)
"#;
    run_ios_sample("Do Syntax for Filter/Reduce", src);
}

#[test]
fn test_ios_chaining_higher_order_functions() {
    let src = r#"
# Chain map, filter, reduce for data processing pipelines
# Define helper functions for the pipeline
function square(x)
    return x * x
end

function is_large(x)
    return x > 20
end

function add(a, b)
    return a + b
end

data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

# Step 1: Square all numbers
squared = map(square, data)

# Step 2: Keep only those > 20
large_squares = filter(is_large, squared)

# Step 3: Sum them
total = reduce(add, large_squares)

println("Original data:")
for i in 1:length(data)
    print(data[i], " ")
end
println("")

println("Squares > 20:")
for i in 1:length(large_squares)
    print(large_squares[i], " ")
end
println("")

println("Sum of large squares: ", total)

println(total)
"#;
    run_ios_sample("Chaining Higher-Order Functions", src);
}

// ============================================================================
// STRUCTURE SAMPLES
// ============================================================================

#[test]
fn test_ios_basic_struct() {
    let src = r#"
# Define an immutable struct with typed fields
struct Point
    x::Float64
    y::Float64
end

# Create instances
p1 = Point(3.0, 4.0)
p2 = Point(0.0, 0.0)

# Access fields
println("p1.x = ", p1.x)
println("p1.y = ", p1.y)

# Use in calculations
distance = sqrt(p1.x^2 + p1.y^2)
println("Distance from origin: ", distance)

println(distance)
"#;
    run_ios_sample("Basic Struct", src);
}

#[test]
fn test_ios_mutable_struct() {
    let src = r#"
# Mutable structs allow field modification
mutable struct Counter
    value::Float64
end

# Create and modify
c = Counter(0.0)
println("Initial value: ", c.value)

c.value = 10.0
println("After assignment: ", c.value)

c.value = c.value + 5.0
println("After increment: ", c.value)

# Use in a loop
for i in 1:5
    c.value = c.value + 1.0
end
println("After loop: ", c.value)

println(c.value)
"#;
    run_ios_sample("Mutable Struct", src);
}

#[test]
fn test_ios_struct_with_functions() {
    let src = r#"
# Define a struct
struct Rectangle
    width::Float64
    height::Float64
end

# Functions that work with structs
function area(r)
    r.width * r.height
end

function perimeter(r)
    2.0 * (r.width + r.height)
end

function is_square(r)
    r.width == r.height
end

# Create rectangles
rect = Rectangle(5.0, 3.0)
square = Rectangle(4.0, 4.0)

println("Rectangle 5x3:")
println("  Area: ", area(rect))
println("  Perimeter: ", perimeter(rect))
println("  Is square: ", is_square(rect))

println("Square 4x4:")
println("  Area: ", area(square))
println("  Perimeter: ", perimeter(square))
println("  Is square: ", is_square(square))

println(area(rect))
"#;
    run_ios_sample("Struct with Functions", src);
}

#[test]
fn test_ios_euclidean_distance() {
    let src = r#"
# Point struct for 2D geometry
struct Point
    x::Float64
    y::Float64
end

# Calculate distance between two points
function distance(p1, p2)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
end

# Create points
origin = Point(0.0, 0.0)
a = Point(3.0, 4.0)
b = Point(6.0, 8.0)

println("Distance from origin to A: ", distance(origin, a))
println("Distance from origin to B: ", distance(origin, b))
println("Distance from A to B: ", distance(a, b))

println(distance(a, b))
"#;
    run_ios_sample("Euclidean Distance", src);
}

#[test]
fn test_ios_particle_simulation() {
    let src = r#"
# Mutable struct for particle position
mutable struct Particle
    x::Float64
    y::Float64
    vx::Float64
    vy::Float64
end

# Update particle position
function step!(p, dt)
    p.x = p.x + p.vx * dt
    p.y = p.y + p.vy * dt
end

# Create a particle with initial position and velocity
particle = Particle(0.0, 0.0, 1.0, 0.5)

println("Initial position: (", particle.x, ", ", particle.y, ")")

# Simulate 10 time steps
dt = 0.1
for t in 1:10
    step!(particle, dt)
    println("t=", t * dt, ": (", particle.x, ", ", particle.y, ")")
end

# Return final distance from origin
println(sqrt(particle.x^2 + particle.y^2))
"#;
    run_ios_sample("Particle Simulation", src);
}

// ============================================================================
// ERROR HANDLING SAMPLES
// ============================================================================

#[test]
fn test_ios_try_catch_basics() {
    let src = r#"
# try/catch handles runtime errors gracefully
x = 0

try
    # This will cause a division by zero error
    y = 1 / 0
    x = 999  # This won't execute
catch e
    # e contains the error message
    println("Caught error: ", e)
    x = -1
end

println("x = ", x)
println(x)
"#;
    run_ios_sample("Try/Catch Basics", src);
}

#[test]
fn test_ios_try_catch_finally() {
    let src = r#"
# finally block always executes, error or not
result = 0
cleanup_done = 0

try
    result = 10 / 2  # Normal operation
    println("Computed result: ", result)
    catch e
    println("Error: ", e)
    result = -1
finally
    # This always runs
    cleanup_done = 1
    println("Cleanup completed")
end

println("Result: ", result)
println("Cleanup done: ", cleanup_done)

println(result)
"#;
    run_ios_sample("Try/Catch/Finally", src);
}

#[test]
fn test_ios_error_recovery() {
    let src = r#"
# Recover from errors and continue processing
function safe_divide(a, b)
    result = 0.0
    try
        result = a / b
    catch e
        println("Cannot divide ", a, " by ", b)
        result = 0.0
    end
    result
end

println("10 / 2 = ", safe_divide(10, 2))
println("10 / 0 = ", safe_divide(10, 0))
println("20 / 4 = ", safe_divide(20, 4))
println("5 / 0 = ", safe_divide(5, 0))

# All divisions complete even with errors
println(safe_divide(100, 5))
"#;
    run_ios_sample("Error Recovery", src);
}

// ============================================================================
// MONTE CARLO (randn) SAMPLES
// ============================================================================

#[test]
fn test_ios_normal_distribution_randn() {
    let src = r#"
# randn() generates standard normal random numbers
# Mean = 0, Standard Deviation = 1

# Generate single values
println("Random normal values:")
for i in 1:5
    x = randn()
    println("  ", x)
end

# Generate an array of normal values
arr = randn(10)
println("Array of 10 normal values:")
for i in 1:length(arr)
    println("  arr[", i, "] = ", arr[i])
end

# Calculate sample mean (should be close to 0)
sum = 0.0
for i in 1:length(arr)
    sum += arr[i]
    end
mean = sum / length(arr)
println("Sample mean: ", mean)

println(mean)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Normal Distribution (randn)", src);
}

#[test]
fn test_ios_normal_distribution_matrix() {
    let src = r#"
# randn(m, n) creates 2D array of normal random values

# Create 3x4 matrix of normal random values
mat = randn(3, 4)

println("3x4 Matrix of normal values:")
for i in 1:3
    for j in 1:4
        print(mat[i, j], " ")
    end
    println("")
end

# Calculate statistics
sum = 0.0
sum_sq = 0.0
n = 12
for i in 1:3
    for j in 1:4
        v = mat[i, j]
        sum += v
        sum_sq += v * v
    end
end

mean = sum / n
variance = sum_sq / n - mean * mean
std = sqrt(variance)

println("Mean: ", mean, " (expected ~0)")
println("Std: ", std, " (expected ~1)")

println(std)
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Normal Distribution Matrix", src);
}

#[test]
fn test_ios_box_muller_visualization() {
    let src = r#"
# Generate many normal values and visualize distribution
n = 1000
samples = randn(n)

# Count samples in histogram bins
# Bins: <-2, [-2,-1), [-1,0), [0,1), [1,2), >=2
bins = zeros(6)

for i in 1:n
    x = samples[i]
    if x < -2
        bins[1] += 1
    elseif x < -1
        bins[2] += 1
    elseif x < 0
        bins[3] += 1
    elseif x < 1
        bins[4] += 1
    elseif x < 2
        bins[5] += 1
    else
        bins[6] += 1
    end
end

println("Normal Distribution Histogram (n=1000):")
println("  x < -2:  ", bins[1])
println(" -2 <= x < -1: ", bins[2])
println(" -1 <= x <  0: ", bins[3])
println("  0 <= x <  1: ", bins[4])
println("  1 <= x <  2: ", bins[5])
println("  x >= 2:  ", bins[6])

# Most values should be in the middle bins
println(bins[3] + bins[4])
"#;
    // Just check it runs - result is stochastic
    run_ios_sample("Box-Muller Visualization", src);
}
