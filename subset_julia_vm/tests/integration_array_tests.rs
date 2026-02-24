//! Integration tests: Arrays, matrices, broadcast, strings, complex numbers

mod common;
use common::*;

use subset_julia_vm::builtins::BuiltinId;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{ArrayValue, Instr, Value, Vm};
use subset_julia_vm::*;

// ==================== Array Tests (VM Level) ====================



#[test]
fn test_array_value_zeros() {
    let arr = ArrayValue::zeros(vec![3]);
    assert_eq!(arr.len(), 3);
    assert_eq!(arr.shape, vec![3]);
    for i in 1..=3 {
        assert!((arr.get_f64(&[i]).unwrap() - 0.0).abs() < 1e-10);
    }
}

#[test]
fn test_array_value_ones() {
    let arr = ArrayValue::ones(vec![2, 3]);
    assert_eq!(arr.len(), 6);
    assert_eq!(arr.shape, vec![2, 3]);
    for i in 1..=2 {
        for j in 1..=3 {
            assert!((arr.get_f64(&[i, j]).unwrap() - 1.0).abs() < 1e-10);
        }
    }
}

#[test]
fn test_array_value_fill() {
    let fill_value = 314.0 / 100.0;
    let arr = ArrayValue::fill(fill_value, vec![2, 2]);
    assert_eq!(arr.len(), 4);
    for i in 1..=2 {
        for j in 1..=2 {
            assert!((arr.get_f64(&[i, j]).unwrap() - fill_value).abs() < 1e-10);
        }
    }
}

#[test]
fn test_array_value_get_set() {
    let mut arr = ArrayValue::zeros(vec![3]);
    arr.set_f64(&[1], 10.0).unwrap();
    arr.set_f64(&[2], 20.0).unwrap();
    arr.set_f64(&[3], 30.0).unwrap();

    assert!((arr.get_f64(&[1]).unwrap() - 10.0).abs() < 1e-10);
    assert!((arr.get_f64(&[2]).unwrap() - 20.0).abs() < 1e-10);
    assert!((arr.get_f64(&[3]).unwrap() - 30.0).abs() < 1e-10);
}

#[test]
fn test_array_value_2d_indexing() {
    // Create a 2x3 matrix
    let mut arr = ArrayValue::zeros(vec![2, 3]);

    // Set values: mat[i,j] = i*10 + j
    for i in 1..=2 {
        for j in 1..=3 {
            arr.set_f64(&[i, j], (i * 10 + j) as f64).unwrap();
        }
    }

    // Verify values (column-major order like Julia)
    assert!((arr.get_f64(&[1, 1]).unwrap() - 11.0).abs() < 1e-10);
    assert!((arr.get_f64(&[1, 2]).unwrap() - 12.0).abs() < 1e-10);
    assert!((arr.get_f64(&[2, 1]).unwrap() - 21.0).abs() < 1e-10);
    assert!((arr.get_f64(&[2, 3]).unwrap() - 23.0).abs() < 1e-10);
}

#[test]
fn test_array_value_push_pop() {
    let mut arr = ArrayValue::vector(vec![1.0, 2.0, 3.0]);
    assert_eq!(arr.len(), 3);

    let _ = arr.push_f64(4.0);
    assert_eq!(arr.len(), 4);
    assert!((arr.get_f64(&[4]).unwrap() - 4.0).abs() < 1e-10);

    let popped = arr.pop_f64().unwrap();
    assert!((popped - 4.0).abs() < 1e-10);
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_array_index_out_of_bounds() {
    let arr = ArrayValue::zeros(vec![3]);
    assert!(arr.get(&[0]).is_err()); // Julia is 1-indexed
    assert!(arr.get(&[4]).is_err()); // Out of bounds
}

#[test]
fn test_vm_array_instructions() {
    // Test creating an array and indexing it
    let code = vec![
        Instr::NewArray(3),
        Instr::PushF64(10.0),
        Instr::PushElem,
        Instr::PushF64(20.0),
        Instr::PushElem,
        Instr::PushF64(30.0),
        Instr::PushElem,
        Instr::FinalizeArray(vec![3]),
        Instr::StoreArray("arr".to_string()),
        Instr::LoadArray("arr".to_string()),
        Instr::PushI64(2), // Index 2
        Instr::IndexLoad(1),
        Instr::ReturnF64,
    ];

    let rng = StableRng::new(0);
    let mut vm = Vm::new(code, rng);
    let result = vm.run().unwrap();

    match result {
        Value::F64(v) => assert!((v - 20.0).abs() < 1e-10),
        _ => panic!("Expected F64"),
    }
}

#[test]
fn test_vm_zeros_instruction() {
    let code = vec![
        Instr::PushI64(5), // Create array of size 5
        Instr::CallBuiltin(BuiltinId::Zeros, 1),
        Instr::CallBuiltin(BuiltinId::Length, 1),
        Instr::ReturnI64,
    ];

    let rng = StableRng::new(0);
    let mut vm = Vm::new(code, rng);
    let result = vm.run().unwrap();

    match result {
        Value::I64(len) => assert_eq!(len, 5),
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_vm_make_range() {
    let code = vec![
        Instr::PushI64(1), // start
        Instr::PushI64(1), // step
        Instr::PushI64(5), // stop
        Instr::MakeRange,
        Instr::CallBuiltin(BuiltinId::Length, 1),
        Instr::ReturnI64,
    ];

    let rng = StableRng::new(0);
    let mut vm = Vm::new(code, rng);
    let result = vm.run().unwrap();

    match result {
        Value::I64(len) => assert_eq!(len, 5), // 1, 2, 3, 4, 5
        _ => panic!("Expected I64"),
    }
}

#[test]
fn test_vm_array_push_instruction() {
    let code = vec![
        Instr::PushI64(2), // Create array of size 2
        Instr::CallBuiltin(BuiltinId::Zeros, 1),
        Instr::PushF64(99.0), // Push new element
        Instr::ArrayPush,
        Instr::CallBuiltin(BuiltinId::Length, 1),
        Instr::ReturnI64,
    ];

    let rng = StableRng::new(0);
    let mut vm = Vm::new(code, rng);
    let result = vm.run().unwrap();

    match result {
        Value::I64(len) => assert_eq!(len, 3), // 2 zeros + 1 pushed
        _ => panic!("Expected I64"),
    }
}

// ==================== Comprehension Tests ====================

#[test]
fn test_comprehension_simple() {
    // Test [x for x in 1:5] - creates array [1.0, 2.0, 3.0, 4.0, 5.0]
    let src = "[x for x in 1:5]";
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::Array(arr) => {
            let arr = arr.borrow();
            assert_eq!(arr.len(), 5);
            assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
            assert!((arr.get_f64(&[5]).unwrap() - 5.0).abs() < 1e-10);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_comprehension_with_expression() {
    // Test [x*x for x in 1:4] - creates array [1.0, 4.0, 9.0, 16.0]
    let src = "[x*x for x in 1:4]";
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::Array(arr) => {
            let arr = arr.borrow();
            assert_eq!(arr.len(), 4);
            assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
            assert!((arr.get_f64(&[2]).unwrap() - 4.0).abs() < 1e-10);
            assert!((arr.get_f64(&[3]).unwrap() - 9.0).abs() < 1e-10);
            assert!((arr.get_f64(&[4]).unwrap() - 16.0).abs() < 1e-10);
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_comprehension_with_filter() {
    // Test [x for x in 1:6 if x > 3] - creates array [4.0, 5.0, 6.0]
    let src = "[x for x in 1:6 if x > 3]";
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::Array(arr) => {
            let arr = arr.borrow();
            assert_eq!(arr.len(), 3);
            assert!((arr.get_f64(&[1]).unwrap() - 4.0).abs() < 1e-10);
            assert!((arr.get_f64(&[2]).unwrap() - 5.0).abs() < 1e-10);
            assert!((arr.get_f64(&[3]).unwrap() - 6.0).abs() < 1e-10);
        }
        _ => panic!("Expected Array"),
    }
}

// ==================== Array Parsing Integration Tests ====================

#[test]
fn test_vector_basics_sample() {
    // Test the exact Vector Basics sample from iOS app
    // Integer arrays return I64 elements now (type-preserving behavior)
    let src = r#"# Create a vector
arr = [1, 2, 3, 4, 5]

# Access elements (1-indexed like Julia)
println("First element: ", arr[1])
println("Third element: ", arr[3])
println("Last element: ", arr[5])

# Get length
println("Length: ", length(arr))

arr[3]"#;

    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::I64(x) => assert_eq!(x, 3),
        Value::F64(x) => assert!((x - 3.0).abs() < 1e-10),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

#[test]
fn test_vector_basics_via_compile_and_run() {
    // Test through the compile_and_run_auto_str API (same as FFI)
    use subset_julia_vm::compile_and_run_auto_str;

    let src = r#"# Create a vector
arr = [1, 2, 3, 4, 5]

# Access elements (1-indexed like Julia)
println("First element: ", arr[1])
println("Third element: ", arr[3])
println("Last element: ", arr[5])

# Get length
println("Length: ", length(arr))

arr[3]"#;

    let result = compile_and_run_auto_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

#[test]
fn test_parse_array_literal() {
    // Test parsing array literal from source
    // Integer arrays return I64 elements now (type-preserving behavior)
    let src = r#"
arr = [1, 2, 3, 4, 5]
arr[3]
"#;
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::I64(x) => assert_eq!(x, 3),
        Value::F64(x) => assert!((x - 3.0).abs() < 1e-10),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

#[test]
fn test_parse_range_expression() {
    // Test parsing range expression
    let src = r#"
r = 1:5
length(r)
"#;
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::I64(x) => assert_eq!(x, 5),
        _ => panic!("Expected I64, got {:?}", result),
    }
}

#[test]
fn test_parse_array_index_assign() {
    // Test parsing array index assignment
    // Integer arrays return I64 elements now (type-preserving behavior)
    let src = r#"
arr = [10, 20, 30]
arr[2] = 99
arr[2]
"#;
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::I64(x) => assert_eq!(x, 99),
        Value::F64(x) => assert!((x - 99.0).abs() < 1e-10),
        _ => panic!("Expected I64 or F64, got {:?}", result),
    }
}

#[test]
fn test_parse_comprehension_from_source() {
    // Test parsing comprehension from source
    let src = r#"[x^2 for x in 1:4]"#;
    let result = run_core_pipeline(src, 0).expect("pipeline failed");

    match result {
        Value::Array(arr) => {
            let arr = arr.borrow();
            assert_eq!(arr.len(), 4);
            assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
            assert!((arr.get_f64(&[2]).unwrap() - 4.0).abs() < 1e-10);
            assert!((arr.get_f64(&[3]).unwrap() - 9.0).abs() < 1e-10);
            assert!((arr.get_f64(&[4]).unwrap() - 16.0).abs() < 1e-10);
        }
        _ => panic!("Expected Array, got {:?}", result),
    }
}

#[test]
fn test_for_loop_with_sqrt_range() {
    // Test for loop with sqrt() in range expression
    let src = r#"
function f(n)
    count = 0
    for i in 2:sqrt(n)
        count += 1
    end
    count
end
f(100)
"#;
    let result = compile_and_run_str(src, 0);
    // sqrt(100) = 10, so loop runs for i = 2, 3, 4, 5, 6, 7, 8, 9, 10 = 9 iterations
    assert!((result - 9.0).abs() < 1e-10, "Expected 9.0, got {}", result);
}

#[test]
fn test_sieve_of_eratosthenes() {
    let src = r#"
function sieve(n)
    is_prime = ones(n)
    is_prime[1] = 0
    for i in 2:sqrt(n)
        if is_prime[i] == 1
            j = i * 2
            while j <= n
                is_prime[j] = 0
                j += i
            end
        end
    end
    count = 0
    for i in 1:n
        if is_prime[i] == 1
            count += 1
        end
    end
    count
end
sieve(100)
"#;
    let result = compile_and_run_str(src, 0);
    // There are 25 primes <= 100
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0, got {}",
        result
    );
}

#[test]
fn test_time_macro_with_assignment() {
    // Test @time with assignment (as used in sieve sample)
    let src = r#"
function f(n)
    return n * 2
end
@time result = f(10)
"#;
    let result = compile_and_run_str(src, 0);
    // @time returns the value of the timed expression
    // f(10) = 10 * 2 = 20
    assert!(
        (result - 20.0).abs() < 1e-10,
        "Expected 20.0, got {}",
        result
    );
}

#[test]
fn test_array_mutation_simple() {
    // Test basic array mutation
    let src = r#"
arr = [10, 20, 30]
arr[2] = 99
arr[2]
"#;
    let result = compile_and_run_str(src, 0);
    println!("Simple array mutation result: {}", result);
    assert!(
        (result - 99.0).abs() < 1e-10,
        "Expected 99.0, got {}",
        result
    );
}

#[test]
fn test_push_pop_basic() {
    // Test push! and pop!
    let src = r#"
arr = [10, 20, 30]
push!(arr, 40)
length(arr)
"#;
    let result = compile_and_run_str(src, 0);
    println!("Push result (length): {}", result);
    assert!((result - 4.0).abs() < 1e-10, "Expected 4.0, got {}", result);
}

#[test]
fn test_println_with_array_index() {
    // Test println with array indexing
    let src = r#"
arr = [10, 20, 30]
println("arr[1] = ", arr[1])
arr[1]
"#;
    let result = compile_and_run_str(src, 0);
    println!("Println with array index result: {}", result);
    assert!(
        (result - 10.0).abs() < 1e-10,
        "Expected 10.0, got {}",
        result
    );
}

#[test]
fn test_array_mutation_full() {
    // Test the full Array Mutation sample
    let src = r#"
arr = [10, 20, 30]
arr[2] = 99
push!(arr, 40)
last = pop!(arr)
arr[2]
"#;
    let result = compile_and_run_str(src, 0);
    println!("Array mutation full result: {}", result);
    // arr[2] should be 99
    assert!(
        (result - 99.0).abs() < 1e-10,
        "Expected 99.0, got {}",
        result
    );
}

#[test]
fn test_pop_returns_value() {
    // Test that pop! returns the correct value when used in assignment
    let src = r#"
arr = [10, 20, 30]
last = pop!(arr)
last
"#;
    let result = compile_and_run_str(src, 0);
    println!("Pop returns value result: {}", result);
    // last should be 30 (the last element)
    assert!(
        (result - 30.0).abs() < 1e-10,
        "Expected 30.0, got {}",
        result
    );
}

#[test]
fn test_array_functions_sample() {
    // Test the Array Functions sample
    let src = r#"
z = zeros(5)
println("zeros(5): ", z[1], ", ", z[2])

o = ones(5)
println("ones(5): ", o[1], ", ", o[2])

f = fill(3.14, 4)
println("fill(3.14, 4): ", f[1], ", ", f[2])

f[1]
"#;
    let result = compile_and_run_str(src, 0);
    println!("Array functions result: {}", result);
    assert!(
        (result - (314.0 / 100.0)).abs() < 1e-10,
        "Expected 3.14, got {}",
        result
    );
}

#[test]
fn test_power_with_variable() {
    // Test 2.0^i where i is a variable (not just ^2)
    let src = r#"
i = 3
result = 2.0^i
result
"#;
    let result = compile_and_run_str(src, 0);
    println!("Power with variable result: {}", result);
    // This should fail because only ^2 is supported
}

#[test]
fn test_array_mutation_sample_full() {
    // Test the full Array Mutation sample from CodeSample.swift
    let src = r#"
arr = [10, 20, 30]
println("Initial: ", arr[1], ", ", arr[2], ", ", arr[3])

arr[2] = 99
println("After arr[2] = 99: ", arr[1], ", ", arr[2], ", ", arr[3])

push!(arr, 40)
println("After push!(arr, 40): length = ", length(arr))

last = pop!(arr)
println("pop! returned: ", last)
println("After pop!: length = ", length(arr))

arr[2]
"#;
    let result = compile_and_run_str(src, 0);
    println!("Array mutation sample result: {}", result);
    assert!(
        (result - 99.0).abs() < 1e-10,
        "Expected 99.0, got {}",
        result
    );
}

#[test]
fn test_sieve_with_time_macro() {
    // Test the actual sieve sample code from CodeSample.swift
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
"#;
    // This should compile and run without errors
    let result = compile_and_run_str(src, 0);
    // @time now returns the value of the timed expression
    // sieve(100) returns 25 (number of primes <= 100)
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0 (count of primes), got {}",
        result
    );
}

#[test]
fn test_array_functions_sample_with_power() {
    // This is the Array Functions sample with 2.0^i (arbitrary power support)
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

powers_of_2[11]
"#;
    let result = compile_and_run_str(src, 0);
    // 2^10 = 1024.0
    assert!(
        (result - 1024.0).abs() < 1e-10,
        "Expected 1024.0, got {}",
        result
    );
}

#[test]
fn test_arbitrary_power() {
    // Test arbitrary power support
    let src = r#"
x = 2.0^10
x
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 1024.0).abs() < 1e-10,
        "Expected 1024.0, got {}",
        result
    );
}

#[test]
fn test_power_with_variable_exponent() {
    // Test power with variable exponent
    let src = r#"
base = 3.0
exp = 4
result = base^exp
result
"#;
    let result = compile_and_run_str(src, 0);
    // 3^4 = 81
    assert!(
        (result - 81.0).abs() < 1e-10,
        "Expected 81.0, got {}",
        result
    );
}

#[test]
fn test_array_mutation_sample() {
    // This is the Array Mutation sample from CodeSample.swift
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

arr[2]
"#;
    let result = compile_and_run_str(src, 0);
    // arr[2] should be 99
    assert!(
        (result - 99.0).abs() < 1e-10,
        "Expected 99.0, got {}",
        result
    );
}

#[test]
fn test_identity_matrix_simple() {
    // Simplified Identity Matrix test
    let src = r#"
m = zeros(4, 4)
for i in 1:4
    m[i, i] = 1
end
m[3, 3]
"#;
    let result = compile_and_run_str(src, 0);
    // m[3, 3] should be 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_function_return_array_simple() {
    // Test function returning an array
    let src = r#"
function make_array()
    arr = [1, 2, 3]
    arr
end

a = make_array()
a[2]
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 2.0).abs() < 1e-10, "Expected 2.0, got {}", result);
}

#[test]
fn test_function_with_param_zeros() {
    // Test function with parameter passed to zeros
    let src = r#"
function make_zeros(n)
    m = zeros(n)
    m
end

a = make_zeros(5)
length(a)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_identity_matrix_with_function() {
    // Identity Matrix with function
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
I[3, 3]
"#;
    let result = compile_and_run_str(src, 0);
    // I[3, 3] should be 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_identity_matrix_sample() {
    // Identity Matrix sample from CodeSample.swift
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

I[3, 3]
"#;
    let result = compile_and_run_str(src, 0);
    // I[3, 3] should be 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

// ==================== Matrix Multiplication Tests ====================

#[test]
fn test_matrix_vector_multiplication() {
    // Test A * v where A is 2x3 matrix and v is 3-element vector
    // First check individual result elements
    let src1 = r#"
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6
v = [1, 2, 3]
result = A * v
result[1]
"#;
    let r1 = compile_and_run_str(src1, 0);
    println!("result[1] = {}", r1);

    let src2 = r#"
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6
v = [1, 2, 3]
result = A * v
result[2]
"#;
    let r2 = compile_and_run_str(src2, 0);
    println!("result[2] = {}", r2);

    // result should be [1*1 + 2*2 + 3*3, 4*1 + 5*2 + 6*3] = [14, 32]
    assert!(
        (r1 - 14.0).abs() < 1e-10,
        "Expected result[1]=14.0, got {}",
        r1
    );
    assert!(
        (r2 - 32.0).abs() < 1e-10,
        "Expected result[2]=32.0, got {}",
        r2
    );
}

#[test]
fn test_matrix_matrix_multiplication() {
    // Test A * B where A is 2x3 and B is 3x2
    let src = r#"
# Create 2x3 matrix A
A = zeros(2, 3)
A[1, 1] = 1
A[1, 2] = 2
A[1, 3] = 3
A[2, 1] = 4
A[2, 2] = 5
A[2, 3] = 6

# Create 3x2 matrix B
B = zeros(3, 2)
B[1, 1] = 7
B[1, 2] = 8
B[2, 1] = 9
B[2, 2] = 10
B[3, 1] = 11
B[3, 2] = 12

# Matrix-matrix multiplication: C = A * B (2x2 result)
C = A * B

# C[1,1] = 1*7 + 2*9 + 3*11 = 7 + 18 + 33 = 58
# C[1,2] = 1*8 + 2*10 + 3*12 = 8 + 20 + 36 = 64
# C[2,1] = 4*7 + 5*9 + 6*11 = 28 + 45 + 66 = 139
# C[2,2] = 4*8 + 5*10 + 6*12 = 32 + 50 + 72 = 154
println("C[1,1] = ", C[1, 1])
println("C[1,2] = ", C[1, 2])
println("C[2,1] = ", C[2, 1])
println("C[2,2] = ", C[2, 2])

@assert C[1, 1] == 58
@assert C[1, 2] == 64
@assert C[2, 1] == 139
@assert C[2, 2] == 154

C[1, 1]
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 58.0).abs() < 1e-10,
        "Expected 58.0, got {}",
        result
    );
}

#[test]
fn test_identity_matrix_multiplication() {
    // Test that I * v = v for identity matrix
    let src = r#"
# Create 3x3 identity matrix
I = zeros(3, 3)
I[1, 1] = 1
I[2, 2] = 1
I[3, 3] = 1

# Create vector
v = [5, 10, 15]

# I * v should equal v
result = I * v

@assert result[1] == 5
@assert result[2] == 10
@assert result[3] == 15

result[1] + result[2] + result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 5 + 10 + 15 = 30
    assert!(
        (result - 30.0).abs() < 1e-10,
        "Expected 30.0, got {}",
        result
    );
}

#[test]
fn test_matrix_sum_sample() {
    // Matrix Sum sample from CodeSample.swift
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

sum
"#;
    let result = compile_and_run_str(src, 0);
    // Sum of 1 to 12 = 78
    assert!(
        (result - 78.0).abs() < 1e-10,
        "Expected 78.0, got {}",
        result
    );
}

// ==================== Broadcast Operations Tests ====================

#[test]
fn test_broadcast_add_arrays() {
    // Test element-wise addition of arrays
    let src = r#"
a = [1, 2, 3]
b = [10, 20, 30]
c = a .+ b

# c should be [11, 22, 33]
@assert c[1] == 11
@assert c[2] == 22
@assert c[3] == 33

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 11 + 22 + 33 = 66
    assert!(
        (result - 66.0).abs() < 1e-10,
        "Expected 66.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_mul_arrays() {
    // Test element-wise multiplication of arrays
    let src = r#"
a = [1, 2, 3]
b = [2, 3, 4]
c = a .* b

# c should be [2, 6, 12]
@assert c[1] == 2
@assert c[2] == 6
@assert c[3] == 12

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 2 + 6 + 12 = 20
    assert!(
        (result - 20.0).abs() < 1e-10,
        "Expected 20.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_sub_arrays() {
    // Test element-wise subtraction
    let src = r#"
a = [10, 20, 30]
b = [1, 2, 3]
c = a .- b

# c should be [9, 18, 27]
@assert c[1] == 9
@assert c[2] == 18
@assert c[3] == 27

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 9 + 18 + 27 = 54
    assert!(
        (result - 54.0).abs() < 1e-10,
        "Expected 54.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_div_arrays() {
    // Test element-wise division
    let src = r#"
a = [10, 20, 30]
b = [2, 4, 5]
c = a ./ b

# c should be [5, 5, 6]
@assert c[1] == 5
@assert c[2] == 5
@assert c[3] == 6

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 5 + 5 + 6 = 16
    assert!(
        (result - 16.0).abs() < 1e-10,
        "Expected 16.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_pow_arrays() {
    // Test element-wise power
    let src = r#"
a = [2, 3, 4]
b = [2, 2, 2]
c = a .^ b

# c should be [4, 9, 16]
@assert c[1] == 4
@assert c[2] == 9
@assert c[3] == 16

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 4 + 9 + 16 = 29
    assert!(
        (result - 29.0).abs() < 1e-10,
        "Expected 29.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_scalar_right() {
    // Test array .* scalar (broadcast scalar to array)
    let src = r#"
a = [1, 2, 3, 4]
c = a .* 10

# c should be [10, 20, 30, 40]
@assert c[1] == 10
@assert c[2] == 20
@assert c[3] == 30
@assert c[4] == 40

c[1] + c[2] + c[3] + c[4]
"#;
    let result = compile_and_run_str(src, 0);
    // 10 + 20 + 30 + 40 = 100
    assert!(
        (result - 100.0).abs() < 1e-10,
        "Expected 100.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_scalar_left() {
    // Test scalar .+ array (broadcast scalar to array)
    let src = r#"
a = [1, 2, 3]
c = 100 .+ a

# c should be [101, 102, 103]
@assert c[1] == 101
@assert c[2] == 102
@assert c[3] == 103

c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 101 + 102 + 103 = 306
    assert!(
        (result - 306.0).abs() < 1e-10,
        "Expected 306.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_chained() {
    // Test chained broadcast operations
    let src = r#"
a = [1, 2, 3]
b = [2, 2, 2]
c = [10, 10, 10]

# (a .* b) .+ c = [2, 4, 6] .+ [10, 10, 10] = [12, 14, 16]
result = (a .* b) .+ c

@assert result[1] == 12
@assert result[2] == 14
@assert result[3] == 16

result[1] + result[2] + result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 12 + 14 + 16 = 42
    assert!(
        (result - 42.0).abs() < 1e-10,
        "Expected 42.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_sqrt() {
    // Test sqrt.(x) - element-wise sqrt
    let src = r#"
a = [1, 4, 9, 16, 25]
b = sqrt.(a)

# b should be [1, 2, 3, 4, 5]
@assert b[1] == 1
@assert b[2] == 2
@assert b[3] == 3
@assert b[4] == 4
@assert b[5] == 5

b[1] + b[2] + b[3] + b[4] + b[5]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 2 + 3 + 4 + 5 = 15
    assert!(
        (result - 15.0).abs() < 1e-10,
        "Expected 15.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_sqrt_with_operations() {
    // Test combining sqrt.() with broadcast operations
    let src = r#"
a = [4, 9, 16]
b = [1, 1, 1]

# sqrt.(a) .+ b = [2, 3, 4] .+ [1, 1, 1] = [3, 4, 5]
result = sqrt.(a) .+ b

@assert result[1] == 3
@assert result[2] == 4
@assert result[3] == 5

result[1] + result[2] + result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 3 + 4 + 5 = 12
    assert!(
        (result - 12.0).abs() < 1e-10,
        "Expected 12.0, got {}",
        result
    );
}

// ==================== Let Block ====================

#[test]
fn test_let_block_basic() {
    // Basic let block with bindings
    let src = r#"
x = 1
y = let a = 10
    a + 5
end
x + y
"#;
    let result = compile_and_run_str(src, 0);
    // x = 1, y = 15, x + y = 16
    assert!(
        (result - 16.0).abs() < 1e-10,
        "Expected 16.0, got {}",
        result
    );
}

#[test]
fn test_let_block_multiple_bindings() {
    // Let block with multiple bindings
    let src = r#"
result = let a = 3, b = 4
    a * a + b * b
end
result
"#;
    let result = compile_and_run_str(src, 0);
    // 3*3 + 4*4 = 9 + 16 = 25
    assert!(
        (result - 25.0).abs() < 1e-10,
        "Expected 25.0, got {}",
        result
    );
}

#[test]
fn test_let_block_shadowing() {
    // Let block should shadow outer variable
    let src = r#"
x = 100
y = let x = 5
    x * 2
end
x + y
"#;
    let result = compile_and_run_str(src, 0);
    // x = 100 (outer), y = 10 (5*2 from let), x + y = 110
    // Note: Current implementation doesn't fully restore x, so this tests basic functionality
    assert!(
        (result - 110.0).abs() < 1e-10,
        "Expected 110.0, got {}",
        result
    );
}

#[test]
fn test_let_block_empty_bindings() {
    // Let block without bindings (just a block)
    let src = r#"
x = 5
y = let
    x + 10
end
y
"#;
    let result = compile_and_run_str(src, 0);
    // y = 5 + 10 = 15
    assert!(
        (result - 15.0).abs() < 1e-10,
        "Expected 15.0, got {}",
        result
    );
}

#[test]
fn test_let_block_with_loop() {
    // Let block with a for loop inside
    let src = r#"
result = let sum = 0
    for i in 1:5
        sum += i
    end
    sum
end
result
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 2 + 3 + 4 + 5 = 15
    assert!(
        (result - 15.0).abs() < 1e-10,
        "Expected 15.0, got {}",
        result
    );
}

// ==================== String Interpolation ====================

#[test]
fn test_string_interpolation_simple() {
    // Simple variable interpolation
    let src = r#"
x = 3
println("x = $(x)")
x
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10);
}

#[test]
fn test_string_interpolation_expression() {
    // Expression inside interpolation
    let src = r#"
x = 3
y = 4
println("sum = $(x + y)")
x + y
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 7.0).abs() < 1e-10);
}

#[test]
fn test_string_interpolation_multiple() {
    // Multiple interpolations in one string
    let src = r#"
x = 3
y = 4
println("x = $(x), y = $(y), sum = $(x + y)")
x * y
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 12.0).abs() < 1e-10);
}

#[test]
fn test_string_interpolation_nested_parens() {
    // Expression with nested parentheses
    let src = r#"
x = 2
println("result = $((x + 1) * 2)")
(x + 1) * 2
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 6.0).abs() < 1e-10);
}

#[test]
fn test_string_interpolation_escaped_dollar() {
    // Escaped dollar sign should be literal
    let src = r#"
x = 5
println("cost: \$$(x)")
x
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10);
}

#[test]
fn test_string_interpolation_float() {
    // Float value interpolation
    let src = r#"
x = 3.14159
println("pi = $(x)")
x
"#;
    let result = compile_and_run_str(src, 0);
    let expected = 314_159.0 / 100_000.0;
    assert!((result - expected).abs() < 1e-10);
}

// ==================== String Concatenation with * ====================

#[test]
fn test_string_concat_two_strings() {
    // Julia uses * for string concatenation
    let src = r#"
str = "Hello" * "World"
println(str)
0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    assert!(
        output.contains("HelloWorld"),
        "Expected 'HelloWorld', got: {}",
        output
    );
}

#[test]
fn test_string_concat_three_strings() {
    // Chain multiple strings with *
    let src = r#"
str = "Hello" * " " * "World"
println(str)
0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    assert!(
        output.contains("Hello World"),
        "Expected 'Hello World', got: {}",
        output
    );
}

#[test]
fn test_string_concat_with_variables() {
    // Concatenate string variables
    let src = r#"
a = "Hello"
b = " "
c = "World"
str = a * b * c
println(str)
0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    assert!(
        output.contains("Hello World"),
        "Expected 'Hello World', got: {}",
        output
    );
}

#[test]
fn test_string_concat_with_expression() {
    // Use string concatenation with string literals
    let src = r#"
prefix = "Hello"
suffix = "World"
result = prefix * ", " * suffix * "!"
println(result)
0
"#;
    let output = compile_and_run_str_with_output(src, 0);
    assert!(
        output.contains("Hello, World!"),
        "Expected 'Hello, World!', got: {}",
        output
    );
}

// ==================== Complex Numbers ====================

#[test]
fn test_complex_literal_im() {
    // im is the imaginary unit
    let src = r#"
z = im
imag(z)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_complex_literal_3im() {
    // 3im = Complex(0, 3)
    let src = r#"
z = 3im
imag(z)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

#[test]
fn test_complex_constructor() {
    // complex(re, im) constructor
    let src = r#"
z = complex(3.0, 4.0)
real(z)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

#[test]
fn test_complex_abs() {
    // abs(3 + 4im) = 5
    let src = r#"
z = complex(3.0, 4.0)
abs(z)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_complex_conj() {
    // conj(3 + 4im) = 3 - 4im
    let src = r#"
z = complex(3.0, 4.0)
w = conj(z)
imag(w)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result + 4.0).abs() < 1e-10,
        "Expected -4.0, got {}",
        result
    );
}

#[test]
fn test_complex_equality() {
    // Complex equality is supported in Julia
    let src = r#"
z = complex(1.0, 2.0)
w = complex(1.0, 2.0)
z == w
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_complex_ordering_error() {
    // Complex ordering comparisons are not supported in Julia
    let src = r#"
z = complex(1.0, 2.0)
z < 3
"#;
    let result = run_core_pipeline(src, 0);
    assert!(
        result.is_err(),
        "Expected error for Complex ordering comparison"
    );
    let err = result.err().unwrap_or_default();
    // Accept any of the possible error message formats for Complex ordering
    assert!(
        err.contains("Complex numbers are not orderable")
            || err.contains("no method matching <(Complex")
            || err.contains("no method matching operator(Complex"),
        "Unexpected error message: {}",
        err
    );
}

#[test]
fn test_complex_add() {
    // (1+2im) + (3+4im) = 4+6im
    let src = r#"
z1 = complex(1.0, 2.0)
z2 = complex(3.0, 4.0)
z3 = z1 + z2
real(z3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 4.0).abs() < 1e-10, "Expected 4.0, got {}", result);
}

#[test]
fn test_complex_sub() {
    // (3+4im) - (1+2im) = 2+2im
    let src = r#"
z1 = complex(3.0, 4.0)
z2 = complex(1.0, 2.0)
z3 = z1 - z2
imag(z3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 2.0).abs() < 1e-10, "Expected 2.0, got {}", result);
}

#[test]
fn test_complex_mul() {
    // (1+2im) * (3+4im) = (1*3 - 2*4) + (1*4 + 2*3)im = -5 + 10im
    let src = r#"
z1 = complex(1.0, 2.0)
z2 = complex(3.0, 4.0)
z3 = z1 * z2
real(z3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result + 5.0).abs() < 1e-10,
        "Expected -5.0, got {}",
        result
    );
}

#[test]
fn test_complex_mul_imag() {
    // (1+2im) * (3+4im) = -5 + 10im
    let src = r#"
z1 = complex(1.0, 2.0)
z2 = complex(3.0, 4.0)
z3 = z1 * z2
imag(z3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!(
        (result - 10.0).abs() < 1e-10,
        "Expected 10.0, got {}",
        result
    );
}

#[test]
fn test_complex_div() {
    // (3+4im) / (1+2im) = (3+4i)(1-2i) / |1+2i|^2 = (11+2i) / 5 = 2.2 + 0.4i
    let src = r#"
z1 = complex(3.0, 4.0)
z2 = complex(1.0, 2.0)
z3 = z1 / z2
real(z3)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 2.2).abs() < 1e-10, "Expected 2.2, got {}", result);
}

/// Test complex sqrt - Issue #1275 resolved.
#[test]
fn test_complex_sqrt() {
    // sqrt(complex(-1, 0)) = im
    let src = r#"
z = complex(-1.0, 0.0)
w = sqrt(z)
imag(w)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_complex_real_for_real() {
    // real(x) for real x returns x
    let src = r#"
x = 5.0
real(x)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
}

#[test]
fn test_complex_imag_for_real() {
    // imag(x) for real x returns 0
    let src = r#"
x = 5.0
imag(x)
"#;
    let result = compile_and_run_str(src, 0);
    assert!((result - 0.0).abs() < 1e-10, "Expected 0.0, got {}", result);
}

// ==================== Broadcast Comparison Operators ====================

#[test]
fn test_broadcast_less_than() {
    // Test .< broadcast comparison
    let src = r#"
a = [1, 5, 3]
b = [2, 4, 3]
c = a .< b
# c should be [1, 0, 0] (true, false, false)
c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 0 + 0 = 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_broadcast_greater_than() {
    // Test .> broadcast comparison
    let src = r#"
a = [1, 5, 3]
b = [2, 4, 3]
c = a .> b
# c should be [0, 1, 0]
c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 0 + 1 + 0 = 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_broadcast_equal() {
    // Test .== broadcast comparison
    let src = r#"
a = [1, 2, 3]
b = [1, 5, 3]
c = a .== b
# c should be [1, 0, 1]
c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 0 + 1 = 2
    assert!((result - 2.0).abs() < 1e-10, "Expected 2.0, got {}", result);
}

#[test]
fn test_broadcast_not_equal() {
    // Test .!= broadcast comparison
    let src = r#"
a = [1, 2, 3]
b = [1, 5, 3]
c = a .!= b
# c should be [0, 1, 0]
c[1] + c[2] + c[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 0 + 1 + 0 = 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_broadcast_comparison_with_scalar() {
    // Test broadcast comparison with scalar
    let src = r#"
a = [1, 2, 3, 4, 5]
c = a .> 2
# c should be [0, 0, 1, 1, 1]
c[1] + c[2] + c[3] + c[4] + c[5]
"#;
    let result = compile_and_run_str(src, 0);
    // 0 + 0 + 1 + 1 + 1 = 3
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

// ==================== Broadcast Logical Operators ====================

#[test]
fn test_broadcast_and() {
    // Test .& broadcast AND
    let src = r#"
a = [1, 0, 1, 0]
b = [1, 1, 0, 0]
c = a .& b
# c should be [1, 0, 0, 0]
c[1] + c[2] + c[3] + c[4]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 0 + 0 + 0 = 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
}

#[test]
fn test_broadcast_or() {
    // Test .| broadcast OR
    let src = r#"
a = [1, 0, 1, 0]
b = [1, 1, 0, 0]
c = a .| b
# c should be [1, 1, 1, 0]
c[1] + c[2] + c[3] + c[4]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 1 + 1 + 0 = 3
    assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
}

// ==================== Broadcast Functions ====================

#[test]
fn test_broadcast_abs() {
    // Test abs.() broadcast function
    let src = r#"
a = [-1, 2, -3, 4]
b = abs.(a)
# b should be [1, 2, 3, 4]
b[1] + b[2] + b[3] + b[4]
"#;
    let result = compile_and_run_str(src, 0);
    // 1 + 2 + 3 + 4 = 10
    assert!(
        (result - 10.0).abs() < 1e-10,
        "Expected 10.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_ifelse() {
    // Test ifelse.() broadcast function - cond must be Bool array
    let src = r#"
cond = [true, false, true, false]
then_val = [10.0, 20.0, 30.0, 40.0]
else_val = [100.0, 200.0, 300.0, 400.0]
result = ifelse.(cond, then_val, else_val)
# result should be [10.0, 200.0, 30.0, 400.0]
result[1] + result[2] + result[3] + result[4]
"#;
    let result = compile_and_run_str(src, 0);
    // 10 + 200 + 30 + 400 = 640
    assert!(
        (result - 640.0).abs() < 1e-10,
        "Expected 640.0, got {}",
        result
    );
}

#[test]
fn test_broadcast_ifelse_with_scalar() {
    // Test ifelse.() with scalar values - cond must be Bool array
    let src = r#"
cond = [true, false, true]
result = ifelse.(cond, 100.0, 0.0)
# result should be [100.0, 0.0, 100.0]
result[1] + result[2] + result[3]
"#;
    let result = compile_and_run_str(src, 0);
    // 100 + 0 + 100 = 200
    assert!(
        (result - 200.0).abs() < 1e-10,
        "Expected 200.0, got {}",
        result
    );
}
