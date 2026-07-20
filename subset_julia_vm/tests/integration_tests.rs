//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod common;

mod integration_array_tests {
    //! Integration tests: Arrays, matrices, broadcast, strings, complex numbers
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm::builtins::BuiltinId;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;
    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::value::{array_wrapper_value_to_array_value, ArrayValue};
    use subset_julia_vm_bytecode::{Instr, Value};

    // ==================== Array Tests (VM Level) ====================

    fn test_array_value_zeros() {
        let arr = ArrayValue::zeros(vec![3]);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.shape, vec![3]);
        for i in 1..=3 {
            assert!((arr.get_f64(&[i]).unwrap() - 0.0).abs() < 1e-10);
        }
    }

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

    fn test_array_value_get_set() {
        let mut arr = ArrayValue::zeros(vec![3]);
        arr.set_f64(&[1], 10.0).unwrap();
        arr.set_f64(&[2], 20.0).unwrap();
        arr.set_f64(&[3], 30.0).unwrap();

        assert!((arr.get_f64(&[1]).unwrap() - 10.0).abs() < 1e-10);
        assert!((arr.get_f64(&[2]).unwrap() - 20.0).abs() < 1e-10);
        assert!((arr.get_f64(&[3]).unwrap() - 30.0).abs() < 1e-10);
    }

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

    fn test_array_index_out_of_bounds() {
        let arr = ArrayValue::zeros(vec![3]);
        assert!(arr.get(&[0]).is_err()); // Julia is 1-indexed
        assert!(arr.get(&[4]).is_err()); // Out of bounds
    }

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

    fn test_comprehension_simple() {
        // Test [x for x in 1:5] - creates array [1.0, 2.0, 3.0, 4.0, 5.0]
        let src = "[x for x in 1:5]";
        let result = run_core_pipeline(src, 0).expect("pipeline failed");

        let arr = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array"));
        assert_eq!(arr.len(), 5);
        assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
        assert!((arr.get_f64(&[5]).unwrap() - 5.0).abs() < 1e-10);
    }

    fn test_comprehension_with_expression() {
        // Test [x*x for x in 1:4] - creates array [1.0, 4.0, 9.0, 16.0]
        let src = "[x*x for x in 1:4]";
        let result = run_core_pipeline(src, 0).expect("pipeline failed");

        let arr = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array"));
        assert_eq!(arr.len(), 4);
        assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
        assert!((arr.get_f64(&[2]).unwrap() - 4.0).abs() < 1e-10);
        assert!((arr.get_f64(&[3]).unwrap() - 9.0).abs() < 1e-10);
        assert!((arr.get_f64(&[4]).unwrap() - 16.0).abs() < 1e-10);
    }

    fn test_comprehension_with_filter() {
        // Test [x for x in 1:6 if x > 3] - creates array [4.0, 5.0, 6.0]
        let src = "[x for x in 1:6 if x > 3]";
        let result = run_core_pipeline(src, 0).expect("pipeline failed");

        let arr = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array"));
        assert_eq!(arr.len(), 3);
        assert!((arr.get_f64(&[1]).unwrap() - 4.0).abs() < 1e-10);
        assert!((arr.get_f64(&[2]).unwrap() - 5.0).abs() < 1e-10);
        assert!((arr.get_f64(&[3]).unwrap() - 6.0).abs() < 1e-10);
    }

    // ==================== Array Parsing Integration Tests ====================

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

    fn test_parse_comprehension_from_source() {
        // Test parsing comprehension from source
        let src = r#"[x^2 for x in 1:4]"#;
        let result = run_core_pipeline(src, 0).expect("pipeline failed");

        let arr = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array, got {:?}", result));
        assert_eq!(arr.len(), 4);
        assert!((arr.get_f64(&[1]).unwrap() - 1.0).abs() < 1e-10);
        assert!((arr.get_f64(&[2]).unwrap() - 4.0).abs() < 1e-10);
        assert!((arr.get_f64(&[3]).unwrap() - 9.0).abs() < 1e-10);
        assert!((arr.get_f64(&[4]).unwrap() - 16.0).abs() < 1e-10);
    }

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

    fn test_complex_literal_im() {
        // im is the imaginary unit
        let src = r#"
    z = im
    imag(z)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 1.0).abs() < 1e-10, "Expected 1.0, got {}", result);
    }

    fn test_complex_literal_3im() {
        // 3im = Complex(0, 3)
        let src = r#"
    z = 3im
    imag(z)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
    }

    fn test_complex_constructor() {
        // complex(re, im) constructor
        let src = r#"
    z = complex(3.0, 4.0)
    real(z)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 3.0).abs() < 1e-10, "Expected 3.0, got {}", result);
    }

    fn test_complex_abs() {
        // abs(3 + 4im) = 5
        let src = r#"
    z = complex(3.0, 4.0)
    abs(z)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
    }

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

    fn test_complex_real_for_real() {
        // real(x) for real x returns x
        let src = r#"
    x = 5.0
    real(x)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 5.0).abs() < 1e-10, "Expected 5.0, got {}", result);
    }

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

    // Generated aggregate chunks for nextest process amortization.
    #[test]
    fn chunk_000() {
        test_array_value_zeros();
        test_array_value_ones();
        test_array_value_fill();
        test_array_value_get_set();
        test_array_value_2d_indexing();
        test_array_value_push_pop();
        test_array_index_out_of_bounds();
        test_vm_array_instructions();
        test_vm_zeros_instruction();
        test_vm_make_range();
        test_vm_array_push_instruction();
        test_comprehension_simple();
        test_comprehension_with_expression();
        test_comprehension_with_filter();
        test_vector_basics_sample();
        test_vector_basics_via_compile_and_run();
    }

    #[test]
    fn chunk_001() {
        test_parse_array_literal();
        test_parse_range_expression();
        test_parse_array_index_assign();
        test_parse_comprehension_from_source();
        test_for_loop_with_sqrt_range();
        test_sieve_of_eratosthenes();
        test_time_macro_with_assignment();
        test_array_mutation_simple();
        test_push_pop_basic();
        test_println_with_array_index();
        test_array_mutation_full();
        test_pop_returns_value();
        test_array_functions_sample();
        test_power_with_variable();
        test_array_mutation_sample_full();
        test_sieve_with_time_macro();
    }

    #[test]
    fn chunk_002() {
        test_array_functions_sample_with_power();
        test_arbitrary_power();
        test_power_with_variable_exponent();
        test_array_mutation_sample();
        test_identity_matrix_simple();
        test_function_return_array_simple();
        test_function_with_param_zeros();
        test_identity_matrix_with_function();
        test_identity_matrix_sample();
        test_matrix_vector_multiplication();
        test_matrix_matrix_multiplication();
        test_identity_matrix_multiplication();
        test_matrix_sum_sample();
        test_broadcast_add_arrays();
        test_broadcast_mul_arrays();
        test_broadcast_sub_arrays();
    }

    #[test]
    fn chunk_003() {
        test_broadcast_div_arrays();
        test_broadcast_pow_arrays();
        test_broadcast_scalar_right();
        test_broadcast_scalar_left();
        test_broadcast_chained();
        test_broadcast_sqrt();
        test_broadcast_sqrt_with_operations();
        test_let_block_basic();
        test_let_block_multiple_bindings();
        test_let_block_shadowing();
        test_let_block_empty_bindings();
        test_let_block_with_loop();
        test_string_interpolation_simple();
        test_string_interpolation_expression();
        test_string_interpolation_multiple();
        test_string_interpolation_nested_parens();
    }

    #[test]
    fn chunk_004() {
        test_string_interpolation_escaped_dollar();
        test_string_interpolation_float();
        test_string_concat_two_strings();
        test_string_concat_three_strings();
        test_string_concat_with_variables();
        test_string_concat_with_expression();
        test_complex_literal_im();
        test_complex_literal_3im();
        test_complex_constructor();
        test_complex_abs();
        test_complex_conj();
        test_complex_equality();
        test_complex_ordering_error();
        test_complex_add();
        test_complex_sub();
        test_complex_mul();
    }

    #[test]
    fn chunk_005() {
        test_complex_mul_imag();
        test_complex_div();
        test_complex_sqrt();
        test_complex_real_for_real();
        test_complex_imag_for_real();
        test_broadcast_less_than();
        test_broadcast_greater_than();
        test_broadcast_equal();
        test_broadcast_not_equal();
        test_broadcast_comparison_with_scalar();
        test_broadcast_and();
        test_broadcast_or();
        test_broadcast_abs();
        test_broadcast_ifelse();
        test_broadcast_ifelse_with_scalar();
    }
}

mod integration_compile_sample_tests {
    //! Integration tests: IR compilation, compile module, program compilation, code samples, macros
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::Value;

    // ==================== IR Compilation ====================

    fn test_compile_to_ir() {
        let src = r#"
    function f(N)
        return N
    end
    "#;
        let json = compile_to_ir_str(src);
        assert!(json.is_some());

        let json = json.unwrap();
        // The IR uses "functions" array not "Func"
        assert!(
            json.contains("\"functions\""),
            "JSON does not contain functions: {}",
            json
        );
        assert!(json.contains("\"name\":\"f\""));
    }

    fn test_run_ir_json() {
        // The run_ir_json_str function runs the main block, not a function call
        // So we need to include the function call in the source
        let src = r#"
    function f(N)
        return 2N
    end
    f(50)
    "#;
        let json = compile_to_ir_str(src).unwrap();
        let result = run_ir_json_str(&json, 0, 0); // n parameter is unused
        assert!((result - 100.0).abs() < 1e-10);
    }

    // ==================== Error Handling ====================

    fn test_invalid_syntax_returns_nan() {
        let src = "this is not valid julia code";
        let result = compile_and_run_str(src, 0);
        assert!(result.is_nan());
    }

    fn test_empty_function_call_wrong_name() {
        // Function is named 'f' but called as 'g'
        let src = r#"
    function f(N)
        return N
    end
    g(100)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(result.is_nan());
    }

    // ==================== Edge Cases ====================

    fn test_zero_iterations() {
        let src = r#"
    function f(N)
        sum = 0
        for i in 1:N
            sum += i
        end
        return sum
    end
    f(0)
    "#;
        let result = compile_and_run_str(src, 0);
        // Loop should not execute, sum stays 0
        assert!((result - 0.0).abs() < 1e-10);
    }

    fn test_single_iteration() {
        let src = r#"
    function f(N)
        sum = 0
        for i in 1:N
            sum += i
        end
        return sum
    end
    f(1)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 1.0).abs() < 1e-10);
    }

    fn test_large_n() {
        let src = r#"
    function f(N)
        return N
    end
    f(1000000)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 1_000_000.0).abs() < 1e-10);
    }

    // ==================== Float Operations ====================

    fn test_float_literal() {
        let src = r#"
    function f(N)
        return 3.14159
    end
    f(1)
    "#;
        let result = compile_and_run_str(src, 0);
        let expected = 314_159.0 / 100_000.0;
        assert!((result - expected).abs() < 1e-10);
    }

    fn test_float_arithmetic() {
        // Test float addition directly without variable assignment
        let src = r#"
    function f(N)
        return 1.5 + 2.5
    end
    f(1)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 4.0).abs() < 1e-10);
    }

    // ==================== Implicit Multiplication ====================

    fn test_implicit_mult_4n() {
        let src = r#"
    function f(N)
        return 4N
    end
    f(10)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 40.0).abs() < 1e-10);
    }

    fn test_implicit_mult_in_expression() {
        let src = r#"
    function f(N)
        cnt = 10
        return 4cnt / N
    end
    f(20)
    "#;
        let result = compile_and_run_str(src, 0);
        // 4 * 10 / 20 = 2
        assert!((result - 2.0).abs() < 1e-10);
    }

    // ==================== Program Tests (println) ====================

    fn test_compile_and_run_auto_println() {
        let src = r#"println("Hello")"#;
        let result = compile_and_run_auto_str(src, 0);
        // println returns Unit, which maps to -4 in the FFI
        assert!((result - (-4.0)).abs() < 1e-10);
    }

    fn test_compile_and_run_auto_function() {
        let src = r#"
    function f(N)
        return 2N
    end
    f(100)
    "#;
        let result = compile_and_run_auto_str(src, 0);
        assert!((result - 200.0).abs() < 1e-10);
    }

    // ==================== Edge Cases ====================

    fn test_zero_loop_iterations() {
        let src = r#"
    function f(N)
        sum = 100
        for i in 1:0
            sum += 1
        end
        return sum
    end
    f(1)
    "#;
        let result = compile_and_run_str(src, 0);
        // Loop doesn't execute, sum stays 100
        assert!((result - 100.0).abs() < 1e-10);
    }

    fn test_one_loop_iteration() {
        let src = r#"
    function f(N)
        sum = 0
        for i in 1:1
            sum += 10
        end
        return sum
    end
    f(1)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 10.0).abs() < 1e-10);
    }

    // ==================== Compile Module Tests ====================

    fn test_compile_simple_return() {
        let src = r#"
    function f(N)
        return N
    end
    f(42)
    "#;
        match compile_and_run_func(src, 42, 0) {
            Value::I64(v) => assert_eq!(v, 42),
            _ => panic!("Expected I64"),
        }
    }

    fn test_compile_constant_return() {
        let src = r#"
    function f(N)
        return 100
    end
    f(0)
    "#;
        match compile_and_run_func(src, 0, 0) {
            Value::I64(v) => assert_eq!(v, 100),
            _ => panic!("Expected I64"),
        }
    }

    fn test_compile_division_direct() {
        let src = r#"
    function f(N)
        return N / 2
    end
    f(10)
    "#;
        match compile_and_run_func(src, 10, 0) {
            Value::F64(v) => assert!((v - 5.0).abs() < 1e-10),
            _ => panic!("Expected F64"),
        }
    }

    fn test_compile_power_direct() {
        let src = r#"
    function f(N)
        return N^2
    end
        f(7)
        "#;
        match compile_and_run_func(src, 7, 0) {
            // Julia preserves integer exponentiation for `Int ^ Int` (Issue #5608).
            Value::I64(v) => assert_eq!(v, 49),
            _ => panic!("Expected I64"),
        }
    }

    fn test_compile_sqrt_direct() {
        let src = r#"
    function f(N)
        return sqrt(N)
    end
    f(16)
    "#;
        match compile_and_run_func(src, 16, 0) {
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
            _ => panic!("Expected F64"),
        }
    }

    fn test_compile_for_loop_direct() {
        let src = r#"
    function f(N)
        cnt = 0
        for i in 1:N
            cnt += 1
        end
        return cnt
    end
    f(5)
    "#;
        match compile_and_run_func(src, 5, 0) {
            Value::I64(v) => assert_eq!(v, 5),
            _ => panic!("Expected I64"),
        }
    }

    fn test_compile_for_loop_sum_direct() {
        let src = r#"
    function f(N)
        sum = 0
        for i in 1:N
            sum += i
        end
        return sum
    end
    f(5)
    "#;
        // 1+2+3+4+5 = 15
        match compile_and_run_func(src, 5, 0) {
            Value::I64(v) => assert_eq!(v, 15),
            _ => panic!("Expected I64"),
        }
    }

    fn test_compile_rand_direct() {
        let src = r#"
    function f()
        return rand()
    end
    f()
    "#;
        match compile_and_run_func(src, 0, 42) {
            Value::F64(v) => {
                assert!((0.0..1.0).contains(&v), "rand() should be in [0, 1)");
            }
            _ => panic!("Expected F64"),
        }
    }

    fn test_compile_rand_deterministic_direct() {
        let src = r#"
    function f()
        return rand()
    end
    f()
    "#;
        let r1 = compile_and_run_func(src, 0, 123);
        let r2 = compile_and_run_func(src, 0, 123);
        match (r1, r2) {
            (Value::F64(v1), Value::F64(v2)) => {
                assert_eq!(v1, v2, "Same seed should produce same result");
            }
            _ => panic!("Expected F64"),
        }
    }

    // ==================== Program Compilation (println) Tests ====================

    fn test_compile_println_string() {
        let src = r#"println("Hello")"#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::Nothing));
        assert_eq!(output, "Hello\n");
    }

    fn test_compile_println_multiple() {
        let src = r#"
    println("Line 1")
    println("Line 2")
    "#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output, "Line 1\nLine 2\n");
    }

    fn test_compile_println_escape_newline() {
        let src = r#"println("Hello\nWorld")"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output, "Hello\nWorld\n");
    }

    fn test_compile_println_escape_tab() {
        let src = r#"println("A\tB")"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output, "A\tB\n");
    }

    fn test_compile_print_no_newline() {
        // Test print() without trailing newline
        let src = r#"
    print("A")
    print("B")
    print("C")
    1
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(1)));
        assert_eq!(output, "ABC");
    }

    fn test_compile_print_mixed_with_println() {
        // Test mixing print() and println()
        let src = r#"
    print("Hello")
    print(" ")
    println("World")
    print("A")
    println("B")
    1
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(1)));
        assert_eq!(output, "Hello World\nAB\n");
    }

    fn test_compile_print_i64_no_newline() {
        // Test print() with integer without trailing newline
        let src = r#"
    print(1)
    print(2)
    print(3)
    0
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(0)));
        assert_eq!(output, "123");
    }

    fn test_compile_print_ascii_art_grid() {
        // Test print() for ASCII art like Mandelbrot sample
        let src = r#"
    for row in 1:3
        for col in 1:5
            if col > 3
                print("*")
            else
                print(".")
            end
        end
        println("")
    end
    0
    "#;
        let (result, output) = compile_and_run_script_direct(src, 0);
        assert!(matches!(result, Value::I64(0)));
        assert_eq!(output, "...**\n...**\n...**\n");
    }

    fn test_mandelbrot_scalar_sample() {
        // Test the actual Mandelbrot sample from iOS app (now using complex numbers and abs2)
        let src = r#"
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

    println("Testing Mandelbrot:")
    c1 = mandelbrot_escape(0.0 + 0.0im, 100)
    println("c1 = ", c1)

    println("Mini Mandelbrot (5x3):")
    for row in 0:2
        ci = 1.0 - row * 1.0
        for col in 0:4
            cr = -2.0 + col * 0.75
            c = cr + ci * im
            n = mandelbrot_escape(c, 50)
            if n == 50
                print("*")
            elseif n > 10
                print("+")
            else
                print(" ")
            end
        end
        println("")
    end

    c1
    "#;
        let (result, output) = compile_and_run_script_direct(src, 0);
        println!("Mandelbrot output:\n{}", output);
        println!("Result: {:?}", result);
        // The result can be I64 or F64 depending on type inference
        match result {
            Value::I64(v) => assert_eq!(v, 100),
            Value::F64(v) => assert!((v - 100.0).abs() < 1e-10),
            _ => panic!("Expected I64 or F64, got {:?}", result),
        }
        assert!(output.contains("Testing Mandelbrot:"));
        assert!(output.contains("Mini Mandelbrot"));
    }

    fn test_mandelbrot_via_ffi() {
        // Test using the actual FFI function (now using complex numbers and abs2)
        let src = r#"
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

    println("Test:")
    for row in 0:2
        for col in 0:4
            c = (-2.0 + col * 0.75) + (1.0 - row * 1.0) * im
            n = mandelbrot_escape(c, 50)
            if n == 50
                print("*")
            else
                print(" ")
            end
        end
        println("")
    end
    0
    "#;
        let output = compile_and_run_str_with_output(src, 0);
        println!("FFI output:\n{}", output);
        assert!(output.contains("Test:"));
        // With complex number implementation, c=-2.0 escapes at iteration 16
        // (floating-point boundary behavior), so we get 3 stars instead of 4
        assert!(output.contains("***"));
    }

    fn test_mandelbrot_ios_sample_exact() {
        // Test the EXACT iOS sample code (now using complex numbers and abs2)
        let src = r#"
    # Mandelbrot escape time algorithm
    # Type annotations required for Complex dispatch
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

    c1
    "#;
        let output = compile_and_run_str_with_output(src, 0);
        println!("iOS sample output:\n{}", output);
        assert!(
            output.contains("Testing Mandelbrot escape times:"),
            "Missing test header"
        );
        assert!(
            output.contains("Mini Mandelbrot"),
            "Missing Mini Mandelbrot header"
        );
    }

    // ==================== Error Cases ====================

    fn test_compile_unknown_variable_error() {
        // Unknown variables should cause a runtime error when the function is actually called
        let src = r#"
    function f(N)
        return unknown_var
    end
    f(1)
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_err());
    }

    fn test_arbitrary_power_cubed() {
        // Test N^3 (arbitrary power support)
        let src = r#"
    function f(N)
        return N^3
    end
    f(4)
    "#;
        let result = compile_and_run_str(src, 0);
        // 4^3 = 64
        assert!(
            (result - 64.0).abs() < 1e-10,
            "Expected 64.0, got {}",
            result
        );
    }

    // ==================== Code Sample Tests ====================

    fn test_sample_simple_arithmetic_output() {
        let src = r#"
    x = 10
    y = 20
    sum = x + y
    product = x * y
    println("Sum: ", sum)
    println("Product: ", product)
    sum
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(30)));
        assert_eq!(output, "Sum: 30\nProduct: 200\n");
    }

    fn test_sample_countdown_output() {
        let src = r#"
    function countdown(n)
        for i in n:-1:1
            println(i)
        end
        println("Liftoff!")
    end

    countdown(5)
    "#;
        let (_, output) = compile_and_run_script_direct(src, 0);
        assert_eq!(output, "5\n4\n3\n2\n1\nLiftoff!\n");
    }

    fn test_sample_geometric_series_star_eq() {
        let src = r#"
    function geometric_sum(r, n)
        sum = 0.0
        term = 1.0
        for i in 1:n
            sum += term
            term *= r
        end
        sum
    end

    geometric_sum(0.5, 10)
    "#;
        let result = compile_and_run_auto_str(src, 0);
        assert!((result - 1.998046875).abs() < 1e-9);
    }

    fn test_sample_sum_primes_script() {
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

    sum_primes(100)
    "#;
        let result = run_core_pipeline(src, 0).expect("pipeline failed");
        let value = match result {
            Value::I64(v) => v as f64,
            Value::F64(v) => v,
            _ => f64::NAN,
        };
        assert!((value - 1060.0).abs() < 1e-9);
    }

    fn test_samples_smoke() {
        let samples = [
            r#"println("Hello, World!")"#,
            r#"
    x = 10
    y = 20
    sum = x + y
    product = x * y
    println("Sum: ", sum)
    println("Product: ", product)
    sum
    "#,
            r#"
    x = 16.0
    result = sqrt(x)
    println("sqrt(", x, ") = ", result)
    result
    "#,
            r#"
    function sum_to_n(N)
        sum = 0
        for i in 1:N
            sum += i
        end
        sum
    end

    sum_to_n(100)
    "#,
            r#"
    function countdown(n)
        for i in n:-1:1
            println(i)
        end
        println("Liftoff!")
    end

    countdown(10)
    "#,
            r#"
    function power_of_2(n)
        result = 1
        for i in 1:n
            result = result * 2
        end
        result
    end

    power_of_2(10)
    "#,
            r#"
    function double(x)
        2 * x
    end

    double(21)
    "#,
            r#"
    function max2(a, b)
        ifelse(a > b, a, b)
    end

    max2(42, 17)
    "#,
            r#"
    function factorial(n)
        result = 1
        for i in 1:n
            result = result * i
        end
        result
    end

    factorial(10)
    "#,
            r#"
    function factorial(n)
        if n <= 1
            return 1
        end
        n * factorial(n - 1)
    end

    factorial(10)
    "#,
            r#"
    function fib(n)
        if n <= 1
            return n
        end
        fib(n - 1) + fib(n - 2)
    end

    fib(15)
    "#,
            r#"
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

    fib_fast(30)
    "#,
            r#"
    function gcd(a, b)
        while b > 0
            temp = b
            b = a % b
            a = temp
        end
        a
    end

    gcd(48, 18)
    "#,
            r#"
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

    is_prime(97)
    "#,
            r#"
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

    sum_primes(100)
    "#,
            r#"
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

    estimate_pi(10000)
    "#,
            r#"
    function random_walk_1d(steps)
        position = 0.0
        for i in 1:steps
            step = ifelse(rand() < 0.5, -1.0, 1.0)
            position += step
        end
        position
    end

    random_walk_1d(1000)
    "#,
            r#"
    function monte_carlo_integral(N)
        # Estimate integral of x^2 from 0 to 1
        sum = 0.0
        for i in 1:N
            x = rand()
            sum += x^2
        end
        sum / N  # Should be close to 1/3
    end

    monte_carlo_integral(100000)
    "#,
            r#"
    function harmonic(n)
        sum = 0.0
        for i in 1:n
            sum += 1.0 / i
        end
        sum
    end

    harmonic(100)
    "#,
            r#"
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

    geometric_sum(0.5, 10)
    "#,
            r#"
    function newton_sqrt(x)
        # Find sqrt(x) using Newton's method
        guess = x / 2.0
        for i in 1:10
            guess = (guess + x / guess) / 2.0
        end
        guess
    end

    newton_sqrt(2.0)
    "#,
            r#"
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

    exp_taylor(1.0, 20)  # Should be close to e ≈ 2.71828
    "#,
        ];

        for sample in samples {
            let result = compile_and_run_auto_str(sample, 42);
            assert!(!result.is_nan(), "sample failed: {}", sample);
        }
    }

    // ==================== Macro Tests ====================

    fn test_assert_success() {
        // Assert with true condition should pass
        let src = r#"
    @assert 1 > 0
    42
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 42.0).abs() < 1e-10);
    }

    fn test_assert_with_message() {
        // Assert with true condition and message
        let src = r#"
    x = 10
    @assert x > 5 "x must be greater than 5"
    x
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 10.0).abs() < 1e-10);
    }

    fn test_time_expression() {
        // @time should execute and return the result
        let src = r#"
    @time 1 + 2 + 3
    "#;
        let result = compile_and_run_str(src, 0);
        // Result should be 6 (the expression value is not captured, but execution should succeed)
        // Since @time wraps in Stmt::Timed, it doesn't return the value
        // Let's check it runs without error
        assert!(!result.is_nan());
    }

    fn test_time_block() {
        // @time with begin...end block
        let src = r#"
    @time begin
        x = 0
        for i in 1:100
            x += i
        end
    end
    100
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 100.0).abs() < 1e-10);
    }

    fn test_assert_in_function() {
        let src = r#"
    function checked_sqrt(x)
        @assert x >= 0 "cannot take sqrt of negative number"
        sqrt(x)
    end
    checked_sqrt(16.0)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 4.0).abs() < 1e-10);
    }

    // ==================== main.jl Syntax Tests ====================

    fn test_unicode_function_name_pi() {
        // Test function with π in name (calcπ from main.jl)
        let src = r#"
    function calcπ(N)
        N * 3
    end
    calcπ(10)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 30.0).abs() < 1e-10);
    }

    fn test_pi_constant_ascii() {
        let src = r#"
    function f(N)
        return Float64(pi)
    end
    f(0)
    "#;
        let result = compile_and_run_str(src, 0);
        eprintln!(
            "DEBUG: result={}, PI={}, diff={}",
            result,
            std::f64::consts::PI,
            (result - std::f64::consts::PI).abs()
        );
        assert!(
            (result - std::f64::consts::PI).abs() < 1e-10,
            "Expected PI, got {}",
            result
        );
    }

    fn test_pi_constant_unicode() {
        let src = r#"
    function f(N)
        return Float64(π)
    end
    f(0)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - std::f64::consts::PI).abs() < 1e-10);
    }

    fn test_pi_shadowed_by_loop_var() {
        // When "pi" is used as a loop variable, it should shadow the builtin constant
        let src = r#"
    sum = 0
    for pi in 1:10
        sum += pi
    end
    sum
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 55.0).abs() < 1e-10);
    }

    fn test_unicode_for_in_operator() {
        // Test for loop with ∈ instead of 'in'
        let src = r#"
    function sum_range(N)
        total = 0
        for i ∈ 1:N
            total += i
        end
        total
    end
    sum_range(5)
    "#;
        let result = compile_and_run_str(src, 0);
        // 1 + 2 + 3 + 4 + 5 = 15
        assert!((result - 15.0).abs() < 1e-10);
    }

    fn test_mainjl_gcd_function() {
        // Test GCD function from main.jl (uses while, !=, %)
        let src = r#"
    function mygcd(a, b)
        while b != 0
            tmp = b
            b = a % b
            a = tmp
        end
        return a
    end
    mygcd(48, 18)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 6.0).abs() < 1e-10);
    }

    fn test_mainjl_calc_pi() {
        // Test π calculation using coprime probability (main.jl style)
        let src = r#"
    function mygcd(a, b)
        while b != 0
            tmp = b
            b = a % b
            a = tmp
        end
        return a
    end

    function calcπ(N)
        cnt = 0
        for a ∈ 1:N
            for b = 1:N
                if mygcd(a, b) == 1
                    cnt += 1
                end
            end
        end
        prob = cnt / N / N
        return sqrt(6.0 / prob)
    end

    calcπ(30)
    "#;
        let result = compile_and_run_str(src, 0);
        // Should be approximately π (3.14159...)
        // With N=30, accuracy is limited, so use wider tolerance
        assert!(
            (result - std::f64::consts::PI).abs() < 0.5,
            "calcπ(30) = {}, expected ~3.14159",
            result
        );
    }

    fn test_unqualified_sqrt_in_function_returns_float64() {
        let src = r#"
    function f()
        sqrt(6.0)
    end

    f()
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 6.0_f64.sqrt()).abs() < 1e-10,
            "f() = {}, expected sqrt(6.0)",
            result
        );
    }

    fn test_vector_intersect_empty_result_dispatches_as_vector() {
        let src = r#"
    length(intersect([1, 2, 3], [4, 5]))
    "#;
        let result = compile_and_run_str(src, 0);
        assert_eq!(result as i64, 0);
    }

    fn test_mainjl_with_time() {
        // Test with @time macro (as in main.jl)
        let src = r#"
    function mygcd(a, b)
        while b != 0
            tmp = b
            b = a % b
            a = tmp
        end
        return a
    end

    function calcπ(N)
        cnt = 0
        for a in 1:N
            for b = 1:N
                if mygcd(a, b) == 1
                    cnt += 1
                end
            end
        end
        prob = cnt / N / N
        return sqrt(6.0 / prob)
    end

    @time calcπ(10)
    "#;
        let result = compile_and_run_str(src, 0);
        // @time wraps the expression, result should still be valid
        assert!(!result.is_nan());
    }

    fn test_time_with_println() {
        // Test @time with println (as in iOS CodeSample)
        let src = r#"
    function f(N)
        return N * 2
    end

    @time println(f(5))
    "#;
        let result = compile_and_run_str(src, 0);
        // Should not fail - println returns Unit, @time wraps it
        assert!(!result.is_nan());
    }

    fn test_coprime_pi_ios_sample() {
        // Exact code from iOS CodeSample "Coprime π Estimation"
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
    #=
    Note that it takes a 15 seconds to complete
    =#
    @time println(calc_pi(10))
    "#;
        let result = compile_and_run_str(src, 0);
        // Should complete without error
        assert!(!result.is_nan(), "Expected valid result, got NaN");
    }

    // ==================== @show Macro Tests ====================

    fn test_show_variable() {
        let src = r#"
    x = 42
    @show x
    x
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(42)));
        assert_eq!(output, "x = 42\n");
    }

    fn test_show_expression() {
        let src = r#"
    a = 10
    b = 20
    @show a + b
    a + b
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(30)));
        // Expression is displayed as source text
        assert_eq!(output, "a + b = 30\n");
    }

    fn test_show_function_call() {
        let src = r#"
    @show sqrt(16.0)
    sqrt(16.0)
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        match result {
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
            _ => panic!("Expected F64"),
        }
        // Expression is displayed as source text
        assert_eq!(output, "sqrt(16.0) = 4.0\n");
    }

    fn test_show_in_function() {
        let src = r#"
    function debug_sum(N)
        sum = 0
        for i in 1:N
            sum += i
            @show sum
        end
        sum
    end
    debug_sum(3)
    "#;
        let (result, output) = compile_and_run_script_direct(src, 0);
        assert!(matches!(result, Value::I64(6)));
        // sum is shown 3 times: 1, 3, 6
        assert_eq!(output, "sum = 1\nsum = 3\nsum = 6\n");
    }

    fn test_show_with_println() {
        let src = r#"
    x = 100
    println("Before show")
    @show x
    println("After show")
    x
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(100)));
        assert_eq!(output, "Before show\nx = 100\nAfter show\n");
    }

    fn test_show_literal_integer() {
        let src = r#"
    println("Hello, World!")
    @show 1
    1
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(1)));
        assert_eq!(output, "Hello, World!\n1 = 1\n");
    }

    fn test_show_literal_float() {
        let src = r#"
    @show 3.14
    3.14
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        match result {
            Value::F64(v) => {
                let expected = 314.0 / 100.0;
                assert!((v - expected).abs() < 1e-10);
            }
            _ => panic!("Expected F64"),
        }
        assert_eq!(output, "3.14 = 3.14\n");
    }

    fn test_ffi_output_array_result_uses_formatter() {
        // The C ABI `compile_and_run_with_output` (moved to the `subset_julia_vm_ffi`
        // cdylib crate, Issue #7808) builds its `[result] …` line from the shared
        // `vm_format_value` display. The FFI crate is staticlib/cdylib-only and cannot
        // be linked as a Rust test dependency (Issue #7821), so test that shared
        // formatter directly: a Julia array must display as `[1, 2, 3]`, never Rust debug.
        let (value, _output) = run_pipeline_with_output("[1, 2, 3]", 0);
        let formatted = subset_julia_vm::ffi_support::vm_format_value(&value);
        assert!(
            formatted.contains("[1, 2, 3]"),
            "expected Julia-like array display, got: {formatted}"
        );
        assert!(
            !formatted.contains("ArrayValue"),
            "value display should not expose Rust debug formatting: {formatted}"
        );
    }

    // Generated aggregate chunks for nextest process amortization.
    #[test]
    fn chunk_000() {
        test_compile_to_ir();
        test_run_ir_json();
        test_invalid_syntax_returns_nan();
        test_empty_function_call_wrong_name();
        test_zero_iterations();
        test_single_iteration();
        test_large_n();
        test_float_literal();
        test_float_arithmetic();
        test_implicit_mult_4n();
        test_implicit_mult_in_expression();
        test_compile_and_run_auto_println();
        test_compile_and_run_auto_function();
        test_zero_loop_iterations();
        test_one_loop_iteration();
        test_compile_simple_return();
    }

    #[test]
    fn chunk_001() {
        test_compile_constant_return();
        test_compile_division_direct();
        test_compile_power_direct();
        test_compile_sqrt_direct();
    }

    #[test]
    fn chunk_008() {
        test_compile_for_loop_direct();
        test_compile_for_loop_sum_direct();
        test_compile_rand_direct();
        test_compile_rand_deterministic_direct();
    }

    #[test]
    fn chunk_009() {
        test_compile_println_string();
        test_compile_println_multiple();
        test_compile_println_escape_newline();
        test_compile_println_escape_tab();
    }

    #[test]
    fn chunk_010() {
        test_compile_print_no_newline();
        test_compile_print_mixed_with_println();
        test_compile_print_i64_no_newline();
        test_compile_print_ascii_art_grid();
    }

    #[test]
    fn chunk_002() {
        test_mandelbrot_scalar_sample();
        test_mandelbrot_via_ffi();
        test_mandelbrot_ios_sample_exact();
        test_compile_unknown_variable_error();
    }

    #[test]
    fn chunk_005() {
        test_arbitrary_power_cubed();
        test_sample_simple_arithmetic_output();
        test_sample_countdown_output();
        test_sample_geometric_series_star_eq();
    }

    #[test]
    fn chunk_006() {
        test_sample_sum_primes_script();
        test_samples_smoke();
        test_assert_success();
        test_assert_with_message();
    }

    #[test]
    fn chunk_007() {
        test_time_expression();
        test_time_block();
        test_assert_in_function();
        test_unicode_function_name_pi();
    }

    #[test]
    fn chunk_003() {
        test_pi_constant_ascii();
        test_pi_constant_unicode();
        test_pi_shadowed_by_loop_var();
        test_unicode_for_in_operator();
        test_mainjl_gcd_function();
        test_mainjl_calc_pi();
        test_unqualified_sqrt_in_function_returns_float64();
        test_vector_intersect_empty_result_dispatches_as_vector();
        test_mainjl_with_time();
        test_time_with_println();
        test_coprime_pi_ios_sample();
        test_show_variable();
        test_show_expression();
        test_show_function_call();
        test_show_in_function();
        test_show_with_println();
    }

    #[test]
    fn chunk_004() {
        test_show_literal_integer();
        test_show_literal_float();
        test_ffi_output_array_result_uses_formatter();
    }
}

mod integration_dict_broadcast_tests {
    //! Integration tests: Dict, compound assignment, broadcast calls, Mandelbrot, try/catch, JSON IR
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::value::array_wrapper_value_to_array_value;
    use subset_julia_vm_bytecode::Value;

    // =============================================================================
    // Dict Tests - Testing Dict{K,V}() parametric constructor syntax
    // =============================================================================

    fn test_dict_empty_constructor() {
        // Test Dict() - empty dict constructor
        let src = r#"
    d = Dict()
    length(d)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0, "Empty dict should have length 0"),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_dict_parametric_constructor_empty() {
        // Test Dict{String, Int}() - empty dict with type parameters
        let src = r#"
    d = Dict{String, Int}()
    length(d)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0, "Empty Dict{{String, Int}}() should have length 0"),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_dict_set_and_get() {
        // Test setting and getting dict values
        let src = r#"
    d = Dict{String, Int}()
    d["apple"] = 10
    d["banana"] = 20
    d["apple"] + d["banana"]
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 30, "Expected 10 + 20 = 30"),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_dict_haskey() {
        // Test haskey function
        let src = r#"
    d = Dict{String, Int}()
    d["key1"] = 100
    haskey(d, "key1")
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::Bool(v) => assert!(v, "haskey should return true for existing key"),
            _ => panic!("Expected Bool, got {:?}", result),
        }
    }

    fn test_dict_haskey_missing() {
        // Test haskey function for missing key
        let src = r#"
    d = Dict{String, Int}()
    d["key1"] = 100
    haskey(d, "nonexistent")
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::Bool(v) => assert!(!v, "haskey should return false for missing key"),
            _ => panic!("Expected Bool, got {:?}", result),
        }
    }

    fn test_dict_get_with_default() {
        // Test get function with default value
        let src = r#"
    d = Dict{String, Int}()
    d["existing"] = 42
    get(d, "nonexistent", -1)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, -1, "get should return default for missing key"),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_dict_get_existing_key() {
        // Test get function for existing key
        let src = r#"
    d = Dict{String, Int}()
    d["existing"] = 42
    get(d, "existing", -1)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 42, "get should return value for existing key"),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_dict_pairs() {
        // Julia's pairs(::AbstractDict) returns the dict itself; iteration yields Pair values.
        let src = r#"
    d = Dict{String, Int}()
    d["apple"] = 10
    d["banana"] = 20
    saw_apple = false
    saw_banana = false
    for p in pairs(d)
        if p.first == "apple" && p.second == 10
            saw_apple = true
        end
        if p.first == "banana" && p.second == 20
            saw_banana = true
        end
    end
    length(pairs(d)) == 2 && saw_apple && saw_banana
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::Bool(v) => assert!(v, "pairs(dict) should iterate both inserted pairs"),
            _ => panic!("Expected Bool, got {:?}", result),
        }
    }

    fn test_time_println_string_literal() {
        // Regression test: @time println("string") was incorrectly parsed as two statements
        // Before fix: "@time println" was parsed as Var, and "Hello, World!" as a separate Str statement
        let result = compile_and_run_str(r#"@time println("Hello, World!")"#, 0);
        assert!(
            !result.is_nan(),
            "@time println with string literal should work"
        );
    }

    // ==================== zero(), trues(), falses() functions ====================

    fn test_zero_function_float() {
        // Test zero(x) for Float64
        let src = r#"
    x = 5.0
    zero(x)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(5.0) should be 0.0"),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_zero_function_int() {
        // Test zero(x) for Int64
        let src = r#"
    x = 42
    zero(x)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0, "zero(42) should be 0"),
            Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(42) should be 0.0"),
            _ => panic!("Expected I64 or F64, got {:?}", result),
        }
    }

    fn test_zero_function_complex() {
        // Test zero(z) for Complex
        // Note: Using complex(re, im) constructor since binary ops with Complex aren't fully supported in compile_core
        let src = r#"
    z = complex(1.0, 2.0)
    w = zero(z)
    real(w) + imag(w)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 0.0).abs() < 1e-10, "zero(complex) should be 0.0+0.0im"),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_trues_function() {
        // Test trues(n)
        let src = r#"
    t = trues(3)
    t[1] + t[2] + t[3]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 3, "trues(3) should have all true values, sum = 3"),
            Value::F64(v) => assert!(
                (v - 3.0).abs() < 1e-10,
                "trues(3) should have all 1.0s, sum = 3.0"
            ),
            _ => panic!("Expected I64 or F64, got {:?}", result),
        }
    }

    fn test_falses_function() {
        // Test falses(n)
        let src = r#"
    f = falses(3)
    f[1] + f[2] + f[3]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0, "falses(3) should have all false values, sum = 0"),
            Value::F64(v) => assert!(
                (v - 0.0).abs() < 1e-10,
                "falses(3) should have all 0.0s, sum = 0.0"
            ),
            _ => panic!("Expected I64 or F64, got {:?}", result),
        }
    }

    // ==================== Complex binary operations ====================

    fn test_complex_binary_add() {
        // Test 1.0 + 2.0im using complex constructor
        let src = r#"
    z = 1.0 + complex(0.0, 2.0)
    real(z)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_binary_add_imag() {
        // Test imaginary part of 1.0 + 2.0im
        let src = r#"
    z = 1.0 + complex(0.0, 2.0)
    imag(z)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 2.0).abs() < 1e-10, "Expected 2.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_binary_mul() {
        // Test (0+1i) * (0+1i) = -1
        let src = r#"
    i = complex(0.0, 1.0)
    z = i * i
    real(z)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_binary_pow() {
        // Test i^2 = -1
        let src = r#"
    i = complex(0.0, 1.0)
    z = i^2
    real(z)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_neg() {
        // Test -(1+2i) = -1-2i
        let src = r#"
    z = complex(1.0, 2.0)
    w = -z
    real(w) + imag(w)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - (-3.0)).abs() < 1e-10,
                "Expected -1 + -2 = -3.0, got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== Complex array literals ====================

    fn test_complex_array_literal() {
        // Test [complex(1,2), complex(3,4)] with direct field access
        let src = r#"
    zs = [complex(1.0, 2.0), complex(3.0, 4.0)]
    z = zs[1]
    z.re + z.im
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - 3.0).abs() < 1e-10,
                "Expected 1.0 + 2.0 = 3.0, got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_array_literal_second_element() {
        // Test accessing second element of complex array with direct field access
        let src = r#"
    zs = [complex(1.0, 2.0), complex(3.0, 4.0)]
    z = zs[2]
    z.re + z.im
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - 7.0).abs() < 1e-10,
                "Expected 3.0 + 4.0 = 7.0, got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_complex_array_literal_mixed() {
        // Test [1.0, complex(2.0, 3.0)] - mixed real and complex should promote to complex array
        let src = r#"
    zs = [1.0, complex(2.0, 3.0)]
    real(zs[1]) + imag(zs[1])
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - 1.0).abs() < 1e-10,
                "Expected 1.0 + 0.0 = 1.0, got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== Broadcast Function Call Tests ====================

    fn test_broadcast_sqrt_core_pipeline() {
        // Test sqrt.(x) - element-wise sqrt using tree-sitter lowering
        let src = r#"
    a = [1.0, 4.0, 9.0, 16.0, 25.0]
    b = sqrt.(a)
    b[1] + b[2] + b[3] + b[4] + b[5]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run sqrt broadcast test");
        match result {
            Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_broadcast_abs_core_pipeline() {
        // Test abs.(x) - element-wise abs using tree-sitter lowering
        let src = r#"
    a = [-1.0, -2.0, 3.0, -4.0]
    b = abs.(a)
    b[1] + b[2] + b[3] + b[4]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run abs broadcast test");
        match result {
            Value::F64(v) => assert!((v - 10.0).abs() < 1e-10, "Expected 10.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_broadcast_sin_cos_core_pipeline() {
        // Test sin.(x) and cos.(x)
        let src = r#"
    a = [0.0]
    b = sin.(a)
    c = cos.(a)
    b[1] + c[1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run sin/cos broadcast test");
        match result {
            // sin(0) + cos(0) = 0 + 1 = 1
            Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_broadcast_exp_log_core_pipeline() {
        // Test exp.(x) and log.(x)
        let src = r#"
    a = [1.0]
    b = exp.(a)
    c = log.(b)
    c[1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run exp/log broadcast test");
        match result {
            // log(exp(1)) = 1
            Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_broadcast_ifelse_core_pipeline() {
        // Test ifelse.(cond, then, else) - element-wise ternary
        let src = r#"
    cond = [true, false, true, false]
    a = [10.0, 20.0, 30.0, 40.0]
    b = [1.0, 2.0, 3.0, 4.0]
    result = ifelse.(cond, a, b)
    result[1] + result[2] + result[3] + result[4]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run ifelse broadcast test");
        match result {
            // ifelse.([1,0,1,0], [10,20,30,40], [1,2,3,4]) = [10, 2, 30, 4] -> 46
            Value::F64(v) => assert!((v - 46.0).abs() < 1e-10, "Expected 46.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_broadcast_ifelse_non_bool_condition_error() {
        // Test that ifelse.() raises MethodError for non-Bool condition
        // In Julia, ifelse requires Bool condition type
        let src = r#"
    cond = [1, 0, 1, 0]  # Integer, not Bool
    a = [10.0, 20.0, 30.0, 40.0]
    b = [1.0, 2.0, 3.0, 4.0]
    result = ifelse.(cond, a, b)
    result[1]
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "Expected error for non-Bool condition in ifelse"
        );
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("condition must be Bool") || err.contains("no method matching ifelse"),
            "Expected MethodError for non-Bool condition, got: {}",
            err
        );
    }

    // ==================== iOS App Sample: Mandelbrot Broadcast ====================

    fn test_mandelbrot_broadcast_ios_sample() {
        // Test the exact Mandelbrot Broadcast sample from iOS app (SubsetJuliaVMApp)
        // This test uses broadcast operations for row-parallel computation
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
    in_set
    "##;
        let result = run_core_pipeline(src, 0).expect("Failed to run Mandelbrot Broadcast test");
        // Julia reference: 28 points in set for 15x8 grid with maxiter=20
        match result {
            Value::I64(in_set) => {
                assert_eq!(
                    in_set, 28,
                    "Expected 28 points in set (Julia reference), got {}",
                    in_set
                );
            }
            Value::F64(in_set) => {
                assert!(
                    (in_set - 28.0).abs() < 1.0,
                    "Expected ~28 points in set (Julia reference), got {}",
                    in_set
                );
            }
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    fn test_try_catch_finally_debug() {
        // Minimal test case
        let src = r#"
    result = 0
    try
        result = 5
    finally
        x = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        println!("Result: {:?}", result);
        match result {
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_try_catch_finally_no_error_simple() {
        // Same as failing test but without catch block
        let src = r#"
    result = 0
    try
        result = 5
    catch e
        result = -1
    finally
        x = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        println!("Simple test: {:?}", result);
        match result {
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_top_level_try_preserves_only_preexisting_binding() {
        let src = r#"
    result = 0
    try
        result = 5
        clause_fresh_11281 = 9
    finally
    end
    fresh_defined = @isdefined clause_fresh_11281
    result + ifelse(fresh_defined, 100, 0)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        match result {
            Value::I64(x) => assert_eq!(
                x, 5,
                "preexisting result must survive; fresh local must not"
            ),
            other => panic!("Unexpected result type: {:?}", other),
        }
    }

    fn test_try_catch_finally_no_error_f64() {
        // Test with F64 to see if it's a type issue
        let src = r#"
    result = 0.0
    try
        result = 10.0 / 2.0
    catch e
        result = -1.0
    finally
        x = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        println!("F64 test: {:?}", result);
        match result {
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_try_catch_finally_type_change() {
        // Test where initial type is I64 but try block assigns F64
        let src = r#"
    result = 0
    try
        result = 10.0 / 2.0
    catch e
        result = -1
    finally
        x = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        println!("Type change test: {:?}", result);
        match result {
            Value::I64(x) => println!("Got I64: {}", x),
            Value::F64(x) => println!("Got F64: {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_try_catch_finally_catch_type_matters() {
        // When catch block also assigns F64, it should work
        let src = r#"
    result = 0
    try
        result = 10.0 / 2.0
    catch e
        result = -1.0
    finally
        x = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed");
        println!("Catch F64 test: {:?}", result);
        match result {
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            Value::I64(x) => panic!("Got I64: {} - type inference is wrong!", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Julia-Style Broadcasting Tests ====================

    fn test_julia_broadcast_outer_product_3x3() {
        // (1:3)' .* (1:3) → 3×3 matrix (multiplication table)
        // Expected: [[1,2,3], [2,4,6], [3,6,9]] in column-major order
        let src = r#"
    result = (1:3)' .* (1:3)
    # Access element at position [2,2] (should be 2*2 = 4)
    result[2, 2]
    "#;
        let result = run_core_pipeline(src, 0);
        println!("Julia broadcast outer product result: {:?}", result);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
            other => panic!("Expected numeric, got {:?}", other),
        }
    }

    fn test_julia_broadcast_outer_product_corners() {
        // Test corner elements of the multiplication table
        let src = r#"
    result = (1:3)' .* (1:3)
    # [1,1] = 1*1 = 1
    a = result[1, 1]
    # [3,3] = 3*3 = 9
    b = result[3, 3]
    # [1,3] = 1*3 = 3
    c = result[1, 3]
    # [3,1] = 3*1 = 3
    d = result[3, 1]
    a + b + c + d
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        // 1 + 9 + 3 + 3 = 16
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 16, "Expected 16, got {}", v),
            Value::F64(v) => assert!((v - 16.0).abs() < 1e-10, "Expected 16.0, got {}", v),
            other => panic!("Expected numeric, got {:?}", other),
        }
    }

    fn test_julia_broadcast_row_col_add() {
        // Row vector .+ column vector → matrix
        let src = r#"
    row = (1:3)'  # [1, 3] shape: [[1, 2, 3]]
    col = 1:3    # [3] shape: [1, 2, 3]
    result = row .+ col
    # Result should be 3x3:
    # [[2,3,4], [3,4,5], [4,5,6]]
    # [2,2] = 2 + 2 = 4
    result[2, 2]
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
            other => panic!("Expected numeric, got {:?}", other),
        }
    }

    fn test_julia_broadcast_2d_with_1d() {
        // 2D array [3, 2] .+ 1D array [3] → broadcasts to [3, 2]
        // In Julia, [3] is treated as [3, 1] in 2D context
        // [3, 2] .+ [3, 1] → [3, 2] (column broadcasts to both columns)
        let src = r#"
    mat = zeros(3, 2)
    mat[1, 1] = 1.0
    mat[2, 1] = 2.0
    mat[3, 1] = 3.0
    mat[1, 2] = 4.0
    mat[2, 2] = 5.0
    mat[3, 2] = 6.0
    # mat = [[1,4], [2,5], [3,6]] (column-major)
    vec = [10.0, 20.0, 30.0]  # shape [3] → treated as [3, 1]
    result = mat .+ vec
    # Broadcasting: [3,2] .+ [3,1] → [3,2]
    # result = [[11,14], [22,25], [33,36]]
    result[2, 2]  # 5 + 20 = 25
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
            other => panic!("Expected F64, got {:?}", other),
        }
    }

    fn test_julia_broadcast_same_shape_still_works() {
        // Verify same-shape broadcasting still works (fast path)
        let src = r#"
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    result = a .+ b
    result[2]  # 2 + 20 = 22
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::F64(v) => assert!((v - 22.0).abs() < 1e-10, "Expected 22.0, got {}", v),
            other => panic!("Expected F64, got {:?}", other),
        }
    }

    fn test_julia_broadcast_incompatible_shapes_error() {
        // Incompatible shapes should still error
        let src = r#"
    a = [1.0, 2.0, 3.0]    # shape [3]
    b = [10.0, 20.0]       # shape [2]
    result = a .+ b        # Should error: 3 vs 2 not compatible
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(result.is_err(), "Expected error for incompatible shapes");
    }

    fn test_broadcast_op_function_call_syntax() {
        // Test .*() function call syntax (instead of infix a .* b)
        let src = r#"
    result = .*((1:3)', 1:3)
    # Access [2,2] element (should be 2*2 = 4)
    result[2, 2]
    "#;
        let result = run_core_pipeline(src, 0);
        println!("Broadcast op function call result: {:?}", result);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 4, "Expected 4, got {}", v),
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10, "Expected 4.0, got {}", v),
            other => panic!("Expected numeric, got {:?}", other),
        }
    }

    fn test_broadcast_add_function_call_syntax() {
        // Test .+() function call syntax
        let src = r#"
    result = .+([1.0, 2.0, 3.0], [10.0, 20.0, 30.0])
    result[2]  # 2 + 20 = 22
    "#;
        let result = run_core_pipeline(src, 0);
        println!("Broadcast add function call result: {:?}", result);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        match result.unwrap() {
            Value::F64(v) => assert!((v - 22.0).abs() < 1e-10, "Expected 22.0, got {}", v),
            other => panic!("Expected F64, got {:?}", other),
        }
    }

    // ==================== Additional String Interpolation Tests ====================

    fn test_string_interpolation_no_interpolation_string() {
        // Test that strings without interpolation still work
        let src = r#"
    println("Hello, World!")
    42
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(42)));
        assert_eq!(output, "Hello, World!\n");
    }

    fn test_string_interpolation_full_sample_from_bug_report() {
        // Test the full sample from the user's bug report
        let src = r#"
    x = 42
    pi_approx = 3.14159
    println("x = $(x)")
    println("x + 1 = $(x + 1)")
    println("x * 2 = $(x * 2)")
    println("Pi is approximately $(pi_approx)")
    y = 10
    println("Sum: $(x + y), Product: $(x * y)")
    println("Double: $((x + y) * 2)")
    println(x)
    x
    "#;
        let (result, output) = compile_and_run_program_direct(src, 0);
        assert!(matches!(result, Value::I64(42)));
        let expected = "x = 42\nx + 1 = 43\nx * 2 = 84\nPi is approximately 3.14159\nSum: 52, Product: 420\nDouble: 104\n42\n";
        assert_eq!(output, expected);
    }

    // ==================== JSON IR Flow Tests (Web Simulation) ====================

    fn test_base_loads() {
        use subset_julia_vm::base_loader::get_base_program;

        let base = get_base_program();
        assert!(base.is_some(), "Base should load");
        let base = base.unwrap();
        assert!(
            base.structs.iter().any(|s| s.name == "Complex"),
            "Complex struct should exist"
        );
        assert!(
            base.functions
                .iter()
                .any(|f| f.name == "+" && f.is_base_extension),
            "+ function should exist"
        );

        // Check for generic method signature +(Real, Complex{T})
        // This method handles Real + Complex arithmetic via generic dispatch (Issue #2427)
        let has_real_complex = base.functions.iter().any(|f| {
            if f.name != "+" || !f.is_base_extension {
                return false;
            }
            if f.params.len() != 2 {
                return false;
            }
            let p0 = &f.params[0].type_annotation;
            let p1 = &f.params[1].type_annotation;
            // Generic method: +(x::Real, z::Complex{T}) where {T<:Real}
            matches!(p0, Some(subset_julia_vm::types::JuliaType::Real))
                && matches!(p1, Some(subset_julia_vm::types::JuliaType::Struct(s)) if s == "Complex{T}")
        });
        assert!(
            has_real_complex,
            "+(Real, Complex{{T}}) method should exist in base"
        );
    }

    fn test_merge_base() {
        use std::collections::HashSet;
        use subset_julia_vm::base_loader::get_base_program;

        // Simulate an empty program
        let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
        let ir_json = format!(
            r#"{{
            "structs": [],
            "functions": [],
            "modules": [],
            "usings": [],
            "main": {{ "stmts": [], "span": {sp} }}
        }}"#
        );

        let mut program: Program = serde_json::from_str(&ir_json).expect("parse empty program");

        // Before merge
        assert_eq!(
            program.structs.len(),
            0,
            "Empty program should have no structs"
        );
        assert_eq!(
            program.functions.len(),
            0,
            "Empty program should have no functions"
        );

        // Merge base
        if let Some(base) = get_base_program() {
            let user_func_names: HashSet<_> =
                program.functions.iter().map(|f| f.name.as_str()).collect();
            let user_struct_names: HashSet<_> =
                program.structs.iter().map(|s| s.name.as_str()).collect();

            let mut all_structs: Vec<_> = base
                .structs
                .iter()
                .filter(|&s| !user_struct_names.contains(s.name.as_str()))
                .cloned()
                .collect();
            all_structs.append(&mut program.structs);
            program.structs = all_structs;

            let mut all_functions: Vec<_> = base
                .functions
                .iter()
                .filter(|&f| !user_func_names.contains(f.name.as_str()))
                .cloned()
                .collect();
            all_functions.append(&mut program.functions);
            program.functions = all_functions;
        }

        // After merge
        assert!(
            program.structs.iter().any(|s| s.name == "Complex"),
            "Complex struct should be merged"
        );
        assert!(
            program.functions.iter().any(|f| f.name == "+"),
            "+ function should be merged"
        );

        // Compile should work
        let compiled = compile_core_program(&program);
        assert!(
            compiled.is_ok(),
            "Empty program with merged base should compile: {:?}",
            compiled.err()
        );
    }

    fn test_simple_float_plus_complex() {
        // Minimal test: just 0.0 + complex(0.0, 0.0) at top level
        let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
        let ir_json = format!(
            r#"{{
            "structs": [],
            "functions": [],
            "modules": [],
            "usings": [],
            "main": {{
                "stmts": [{{
                    "Expr": {{
                        "expr": {{
                            "BinaryOp": {{
                                "op": "Add",
                                "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                                "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}],
                "span": {sp}
            }}
        }}"#
        );

        let result = run_from_ir_json(&ir_json, 42);
        assert!(
            result.is_ok(),
            "Simple Float64 + Complex failed: {:?}",
            result.err()
        );
    }

    fn test_float_plus_complex_in_function() {
        // Test: Float64 + Complex inside a function
        let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
        let ir_json = format!(
            r#"{{
            "structs": [],
            "functions": [{{
                "name": "test_add",
                "params": [],
                "kwparams": [],
                "body": {{
                    "stmts": [{{
                        "Return": {{
                            "value": {{
                                "BinaryOp": {{
                                    "op": "Add",
                                    "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                                    "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                                    "span": {sp}
                                }}
                            }},
                            "span": {sp}
                        }}
                    }}],
                    "span": {sp}
                }},
                "return_type": null,
                "span": {sp}
            }}],
            "modules": [],
            "usings": [],
            "main": {{
                "stmts": [{{
                    "Expr": {{
                        "expr": {{
                            "Call": {{
                                "function": "test_add",
                                "args": [],
                                "kwargs": [],
                                "splat_mask": [],
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}],
                "span": {sp}
            }}
        }}"#
        );

        let result = run_from_ir_json(&ir_json, 42);
        assert!(
            result.is_ok(),
            "Float64 + Complex in function failed: {:?}",
            result.err()
        );
    }

    fn test_complex_plus_typed_param() {
        // Test: Complex + Float64 param - with type annotation, static dispatch works
        let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
        let ir_json = format!(
            r#"{{
            "structs": [],
            "functions": [{{
                "name": "test_add",
                "params": [{{"name": "c", "type_annotation": "Float64", "span": {sp}}}],
                "kwparams": [],
                "body": {{
                    "stmts": [{{
                        "Return": {{
                            "value": {{
                                "BinaryOp": {{
                                    "op": "Add",
                                    "left": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                                    "right": {{"Var": ["c", {sp}]}},
                                    "span": {sp}
                                }}
                            }},
                            "span": {sp}
                        }}
                    }}],
                    "span": {sp}
                }},
                "return_type": null,
                "span": {sp}
            }}],
            "modules": [],
            "usings": [],
            "main": {{
                "stmts": [{{
                    "Expr": {{
                        "expr": {{
                            "Call": {{
                                "function": "test_add",
                                "args": [{{"Literal": [{{"Float": 1.0}}, {sp}]}}],
                                "kwargs": [],
                                "splat_mask": [],
                                "span": {sp}
                            }}
                        }},
                        "span": {sp}
                    }}
                }}],
                "span": {sp}
            }}
        }}"#
        );

        let result = run_from_ir_json(&ir_json, 42);
        assert!(
            result.is_ok(),
            "Complex + typed param failed: {:?}",
            result.err()
        );
    }

    fn test_complex_with_typed_param_from_json() {
        // This test simulates the web flow where IR comes from JavaScript lowering as JSON
        // Tests that Complex arithmetic with typed params works correctly
        // Note: Type annotations are required for static method dispatch
        let sp = r#"{"start": 0, "end": 1, "start_line": 0, "end_line": 0, "start_column": 0, "end_column": 1}"#;
        let ir_json = format!(
            r#"
        {{
            "structs": [],
            "functions": [{{
                "name": "test_complex",
                "params": [
                    {{"name": "c", "type_annotation": {{"Struct": "Complex{{Float64}}"}}, "span": {sp}}}
                ],
                "kwparams": [],
                "body": {{
                    "stmts": [
                        {{
                            "Assign": {{
                                "var": "z",
                                "value": {{
                                    "BinaryOp": {{
                                        "op": "Add",
                                        "left": {{"Literal": [{{"Float": 0.0}}, {sp}]}},
                                        "right": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 0.0}}, {{"Float": 0.0}}]]}}, {sp}]}},
                                        "span": {sp}
                                    }}
                                }},
                                "span": {sp}
                            }}
                        }},
                        {{
                            "Assign": {{
                                "var": "result",
                                "value": {{
                                    "BinaryOp": {{
                                        "op": "Add",
                                        "left": {{"Var": ["z", {sp}]}},
                                        "right": {{"Var": ["c", {sp}]}},
                                        "span": {sp}
                                    }}
                                }},
                                "span": {sp}
                            }}
                        }},
                        {{
                            "Return": {{
                                "value": {{"Literal": [{{"Int": 1}}, {sp}]}},
                                "span": {sp}
                            }}
                        }}
                    ],
                    "span": {sp}
                }},
                "return_type": null,
                "span": {sp}
            }}],
            "modules": [],
            "usings": [],
            "main": {{
                "stmts": [
                    {{
                        "Assign": {{
                            "var": "c",
                            "value": {{"Literal": [{{"Struct": ["Complex{{Float64}}", [{{"Float": 1.0}}, {{"Float": 2.0}}]]}}, {sp}]}},
                            "span": {sp}
                        }}
                    }},
                    {{
                        "Expr": {{
                            "expr": {{
                                "Call": {{
                                    "function": "test_complex",
                                    "args": [{{"Var": ["c", {sp}]}}],
                                    "kwargs": [],
                                    "splat_mask": [],
                                    "span": {sp}
                                }}
                            }},
                            "span": {sp}
                        }}
                    }}
                ],
                "span": {sp}
            }}
        }}
        "#
        );

        let result = run_from_ir_json(&ir_json, 42);
        assert!(
            result.is_ok(),
            "Complex + typed param failed: {:?}",
            result.err()
        );
        assert!(matches!(result.unwrap(), Value::I64(1)));
    }

    // ==================== sum(arr) Tests ====================

    fn test_sum_array() {
        // sum([1, 2, 3, 4, 5]) = 15
        let src = r#"
    arr = [1, 2, 3, 4, 5]
    sum(arr)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 15.0).abs() < 1e-10,
            "sum([1,2,3,4,5]) should be 15, got {}",
            result
        );
    }

    fn test_sum_array_floats() {
        // sum([1.5, 2.5, 3.0]) = 7.0
        let src = r#"
    arr = [1.5, 2.5, 3.0]
    sum(arr)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 7.0).abs() < 1e-10,
            "sum([1.5, 2.5, 3.0]) should be 7.0, got {}",
            result
        );
    }

    fn test_sum_with_function() {
        // sum(f, arr) - sum of squares
        let src = r#"
    function square(x)
        x * x
    end
    arr = [1, 2, 3, 4, 5]
    sum(square, arr)
    "#;
        let result = compile_and_run_str(src, 0);
        // 1 + 4 + 9 + 16 + 25 = 55
        assert!(
            (result - 55.0).abs() < 1e-10,
            "sum(square, [1,2,3,4,5]) should be 55, got {}",
            result
        );
    }

    fn test_sum_in_expression() {
        // Using sum in a larger expression
        let src = r#"
    arr = [1, 2, 3, 4, 5]
    mean = sum(arr) / length(arr)
    mean
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 3.0).abs() < 1e-10,
            "mean of [1,2,3,4,5] should be 3.0, got {}",
            result
        );
    }

    // ==================== Ref() Tests ====================

    fn test_ref_basic() {
        // Basic Ref creation - Ref wraps a value and is used inline
        // Ref(x) protects x from broadcasting, treating it as a scalar
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    arr .+ Ref(100)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run Ref test");
        // [1+100, 2+100, 3+100] = [101, 102, 103]
        let arr = array_wrapper_value_to_array_value(&result, &[])
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("Expected Array, got {:?}", result));
        assert_eq!(arr.len(), 3);
        let data = arr.try_data_f64().unwrap();
        assert!((data[0] - 101.0).abs() < 1e-10);
        assert!((data[1] - 102.0).abs() < 1e-10);
        assert!((data[2] - 103.0).abs() < 1e-10);
    }

    fn test_ref_broadcast_scalar() {
        // Ref protects value from broadcasting - treated as scalar
        // arr .+ Ref(10) should add 10 to each element
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    result = arr .+ Ref(10)
    sum(result)
    "#;
        let result = compile_and_run_str(src, 0);
        // [1+10, 2+10, 3+10] = [11, 12, 13], sum = 36
        assert!(
            (result - 36.0).abs() < 1e-10,
            "arr .+ Ref(10) sum should be 36, got {}",
            result
        );
    }

    fn test_ref_broadcast_multiply() {
        // Ref in multiplication broadcast
        let src = r#"
    arr = [2.0, 3.0, 4.0]
    result = arr .* Ref(5)
    sum(result)
    "#;
        let result = compile_and_run_str(src, 0);
        // [2*5, 3*5, 4*5] = [10, 15, 20], sum = 45
        assert!(
            (result - 45.0).abs() < 1e-10,
            "arr .* Ref(5) sum should be 45, got {}",
            result
        );
    }

    fn test_ref_multi_arg_broadcast() {
        // Multi-argument broadcast with Ref: f.(arr, Ref(x))
        let src = r#"
    function add_val(x, y)
        return x + y
    end

    arr = [1.0, 2.0, 3.0]
    result = add_val.(arr, Ref(10))
    sum(result)
    "#;
        let result = compile_and_run_str(src, 0);
        // [1+10, 2+10, 3+10] = [11, 12, 13], sum = 36
        assert!(
            (result - 36.0).abs() < 1e-10,
            "add_val.(arr, Ref(10)) sum should be 36, got {}",
            result
        );
    }

    /// Tests for complex array operations including HOF functions with nested calls
    fn test_complex_array_basic_ops() {
        // Test 1: Complex array creation and length
        let src1 = r#"
    C = [1.0 + 2.0im, 3.0 + 4.0im]
    length(C)
    "#;
        let result1 = compile_and_run_str(src1, 0);
        assert!(
            (result1 - 2.0).abs() < 1e-10,
            "Complex array should have 2 elements, got {}",
            result1
        );

        // Test 2: Broadcast over F64 array
        let src2 = r#"
    function double(x)
        x * 2
    end
    arr = [1.0, 2.0, 3.0]
    result = double.(arr)
    sum(result)
    "#;
        let result2 = compile_and_run_str(src2, 0);
        assert!(
            (result2 - 12.0).abs() < 1e-10,
            "double.([1,2,3]) sum should be 12, got {}",
            result2
        );

        // Test 3: Map over F64 array
        let src3 = r#"
    function double(x)
        x * 2
    end
    arr = [1.0, 2.0, 3.0]
    result = map(double, arr)
    sum(result)
    "#;
        let result3 = compile_and_run_str(src3, 0);
        assert!(
            (result3 - 12.0).abs() < 1e-10,
            "map(double, arr) sum should be 12, got {}",
            result3
        );

        // Test 4: Map over complex array with nested function call
        // Tests that HOF correctly handles when the user function calls another function
        let src4 = r#"
    function get_real(c)
        real(c)
    end
    C = [1.0 + 2.0im, 3.0 + 4.0im]
    result = map(get_real, C)
    sum(result)
    "#;
        let result4 = compile_and_run_str(src4, 0);
        assert!(
            (result4 - 4.0).abs() < 1e-10,
            "map(get_real, C) sum should be 4, got {}",
            result4
        );
    }

    fn test_ref_multi_arg_broadcast_complex() {
        // First test: simple broadcast over complex array without Ref
        let src1 = r#"
    function get_real(c)
        real(c)
    end
    C = [1.0 + 2.0im, 3.0 + 4.0im]
    result = get_real.(C)
    sum(result)
    "#;
        let result1 = compile_and_run_str(src1, 0);
        println!("Simple broadcast over complex: {}", result1);
        assert!(
            (result1 - 4.0).abs() < 1e-10,
            "get_real.(C) sum should be 4 (1+3), got {}",
            result1
        );

        // Second test: broadcast with Ref over complex array
        let src2 = r#"
    function add_val(c, x)
        real(c) + x
    end
    C = [1.0 + 2.0im, 3.0 + 4.0im]
    result = add_val.(C, Ref(10))
    sum(result)
    "#;
        let result2 = compile_and_run_str(src2, 0);
        println!("Broadcast with Ref over complex: {}", result2);
        // (1+10) + (3+10) = 11 + 13 = 24
        assert!(
            (result2 - 24.0).abs() < 1e-10,
            "add_val.(C, Ref(10)) sum should be 24, got {}",
            result2
        );

        // Original test: Mandelbrot escape with type annotations
        let src3 = r#"
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

    # c = 2.0 escapes at k=3, others don't escape
    C = [2.0 + 0.0im, -1.0 + 0.0im, 0.0 + 0.0im]
    result = mandelbrot_escape.(C, Ref(50))
    sum(result)
    "#;
        let result3 = compile_and_run_str(src3, 0);
        println!("Mandelbrot escape broadcast: {}", result3);
        // 3 + 50 + 50 = 103
        assert!(
            (result3 - 103.0).abs() < 1e-10,
            "mandelbrot_escape.(C, Ref(50)) sum should be 103, got {}",
            result3
        );
    }

    fn test_2d_broadcast_shape_preservation() {
        // Test that broadcasting over a 2D matrix preserves shape
        // Step 1: Verify xs' creates row vector
        let src_step1 = r#"
    xs = range(-2.0, 1.0; length=3)
    println("xs length: ", length(xs))
    xt = xs'
    println("xt total: ", length(xt))
    length(xt)
    "#;
        let output1 = compile_and_run_str_with_output(src_step1, 0);
        println!("Step 1 output: {}", output1);
        let result1 = compile_and_run_str(src_step1, 0);
        println!("Step 1 result (xt length): {}", result1);
        assert!(
            (result1 - 3.0).abs() < 1e-10,
            "xt should have 3 elements, got {}",
            result1
        );

        // Step 2: Verify 2D complex matrix creation
        let src_step2 = r#"
    xs = range(-2.0, 1.0; length=3)
    ys = range(1.2, -1.2; length=2)
    C = xs' .+ im .* ys
    println("C total: ", length(C))
    length(C)
    "#;
        let output2 = compile_and_run_str_with_output(src_step2, 0);
        println!("Step 2 output: {}", output2);
        let result2 = compile_and_run_str(src_step2, 0);
        println!("Step 2 result (C length): {}", result2);
        // 2 rows * 3 cols = 6 elements
        assert!(
            (result2 - 6.0).abs() < 1e-10,
            "C should have 6 elements (2x3), got {}",
            result2
        );

        // Step 3: Verify broadcast result shape (without complex indexing which requires more work)
        let src_step3 = r#"
    xs = range(-2.0, 1.0; length=3)
    ys = range(1.2, -1.2; length=2)
    C = xs' .+ im .* ys

    # Use a function that returns real (not complex) for simpler testing
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

    result = mandelbrot_escape.(C, Ref(10))
    println("result total: ", length(result))
    length(result)
    "#;
        let output3 = compile_and_run_str_with_output(src_step3, 0);
        println!("Step 3 output: {}", output3);
        let result3 = compile_and_run_str(src_step3, 0);
        println!("Step 3 result (result length): {}", result3);
        // Should still have 6 elements (2x3) if shape is preserved
        assert!(
            (result3 - 6.0).abs() < 1e-10,
            "2D broadcast result should have 6 elements (2x3), got {}",
            result3
        );
    }

    fn test_range_with_length_output() {
        // Test range with keyword length argument
        // Note: Julia's range(start, stop; length=N) requires Integer for length
        let src = r#"
    n = 5
    xs = range(0.0, 1.0; length=n)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range test");
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_range_with_length_in_function() {
        // Test range with length parameter passed to function
        // Note: Julia's range(start, stop; length=N) requires Integer for length
        let src = r#"
    function make_range(n::Int64)
        range(0.0, 1.0; length=n)
    end
    xs = make_range(5)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range test");
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_range_positional_in_function() {
        // Test using positional range(start, stop, length) to verify core logic works
        // Note: Julia's range(start, stop, length::Integer) requires Int64 for positional length arg
        let src = r#"
    function make_range(n::Int64)
        range(0.0, 1.0, n)
    end
    xs = make_range(5)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range test");
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_from_function_param() {
        // Simple test: pass a value through kwarg
        // Note: Use Int64 to avoid any type conversion issues
        let src = r#"
    function simple(; val=0)
        return val
    end

    simple(; val=42)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
        match result {
            Value::I64(v) => assert_eq!(v, 42, "Expected 42, got {}", v),
            Value::F64(v) => assert_eq!(v, 42.0, "Expected 42.0, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_2d_broadcast_mandelbrot() {
        // Test minimal function call with range
        // Note: Julia's range requires Integer for length argument
        let src1 = r#"
    function test_grid(width, height)
        xs = range(-2.0, 1.0; length=width)
        ys = range(1.2, -1.2; length=height)
        C = xs' .+ im .* ys
        length(C)
    end
    test_grid(5, 3)
    "#;
        let output1 = compile_and_run_str_with_output(src1, 0);
        println!("Simple grid test output: {}", output1);
        let result1 = compile_and_run_str(src1, 0);
        println!("Simple grid test result: {}", result1);
        assert!(
            (result1 - 15.0).abs() < 1e-10,
            "Grid should have 15 elements (5x3), got {}",
            result1
        );
    }

    fn test_sleep_basic_float() {
        let src = r#"
    sleep(0.001)
    42
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 42.0).abs() < 1e-10,
            "Should return 42 after sleep"
        );
    }

    fn test_sleep_integer() {
        let src = r#"
    sleep(0)
    100
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 100.0).abs() < 1e-10,
            "Should return 100 after sleep(0)"
        );
    }

    fn test_sleep_zero() {
        let src = r#"
    sleep(0)
    42
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 42.0).abs() < 1e-10, "Should handle sleep(0)");
    }

    fn test_sleep_returns_nothing() {
        let src = r#"
    result = sleep(0.0)
    # Check if result is nothing by seeing if we can call a function on it
    # nothing can't be used in arithmetic, so we just return a success value
    42
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 42.0).abs() < 1e-10,
            "sleep() should execute successfully"
        );
    }

    fn test_sleep_negative_error() {
        let src = "sleep(-1)";
        let result = run_core_pipeline(src, 0);
        assert!(result.is_err(), "Should error on negative duration");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("negative") || err_msg.contains("Domain error"),
            "Error should mention negative: {}",
            err_msg
        );
    }

    fn test_sleep_infinity_error() {
        // Note: Division by zero is caught before sleep() can see Inf
        // This is acceptable behavior - the important thing is that invalid values are rejected
        let src = r#"
    x = 1.0
    y = 0.0
    inf = x / y
    sleep(inf)
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "Should error on division by zero or infinite duration"
        );
        // Either "Division by zero" or "finite" error is acceptable
    }

    fn test_sleep_nan_error() {
        // Note: Division by zero is caught before sleep() can see NaN
        // This is acceptable behavior - the important thing is that invalid values are rejected
        let src = r#"
    x = 0.0
    y = 0.0
    nan_val = x / y
    sleep(nan_val)
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "Should error on division by zero or NaN duration"
        );
        // Either "Division by zero" or "finite" error is acceptable
    }

    fn test_sleep_in_loop() {
        let src = r#"
    for i in 1:3
        sleep(0.0)
    end
    42
    "#;
        let result = compile_and_run_str(src, 0);
        assert!((result - 42.0).abs() < 1e-10, "Should handle sleep in loop");
    }

    // ==================== Rational Number Operator (//) ====================

    fn test_base_min_function() {
        // Test that a simple base function like min() works
        let src = r#"
    min(3, 5)
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 3.0).abs() < 1e-10,
            "min(3, 5) should be 3, got {}",
            result
        );
    }

    fn test_rational_struct_direct() {
        // Test creating Rational struct directly from prelude
        let src = r#"
    r = Rational(1, 2)
    r.num
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "Rational(1,2).num should be 1, got {}",
            result
        );
    }

    fn test_rational_operator_basic() {
        // Test that // operator is lowered to rational() call
        // We define our own simple rational to verify the lowering works
        let src = r#"
    struct MyRational
        num::Int64
        den::Int64
    end

    function rational(n, d)
        return MyRational(n, d)
    end

    r = 1 // 2
    r.num
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "1//2.num should be 1, got {}",
            result
        );
    }

    fn test_rational_operator_denominator() {
        // Test that // operator correctly passes denominator
        let src = r#"
    struct MyRational
        num::Int64
        den::Int64
    end

    function rational(n, d)
        return MyRational(n, d)
    end

    r = 1 // 2
    r.den
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 2.0).abs() < 1e-10,
            "1//2.den should be 2, got {}",
            result
        );
    }

    fn test_rational_operator_with_expressions() {
        // Test that // operator works with expressions
        let src = r#"
    struct MyRational
        num::Int64
        den::Int64
    end

    function rational(n, d)
        return MyRational(n, d)
    end

    x = 3
    y = 4
    r = x // y
    r.num + r.den
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - 7.0).abs() < 1e-10,
            "3//4 should have num+den = 7, got {}",
            result
        );
    }

    fn test_rational_operator_negative() {
        // Test that // operator works with negative numbers
        let src = r#"
    struct MyRational
        num::Int64
        den::Int64
    end

    function rational(n, d)
        return MyRational(n, d)
    end

    r = -1 // 2
    r.num
    "#;
        let result = compile_and_run_str(src, 0);
        assert!(
            (result - (-1.0)).abs() < 1e-10,
            "(-1)//2.num should be -1, got {}",
            result
        );
    }

    // ==================== Prelude Function Parameter Reassignment Tests ====================
    // These tests verify that prelude functions that reassign their parameters work correctly.
    // This was a bug where parameter reassignment (e.g., a = abs(a)) caused type mismatch.

    fn test_prelude_gcd_basic() {
        // Test prelude gcd function which reassigns its parameters: a = abs(a), b = abs(b)
        let src = "gcd(12, 8)";
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "gcd(12, 8) should succeed, got {:?}",
            result.err()
        );
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 4, "gcd(12, 8) should be 4"),
            Value::F64(v) => assert!(
                (v - 4.0).abs() < 1e-10,
                "gcd(12, 8) should be 4.0, got {}",
                v
            ),
            other => panic!("Expected numeric value, got {:?}", other),
        }
    }

    fn test_prelude_gcd_negative() {
        // Test gcd with negative numbers (should use abs internally)
        let src = "gcd(-12, 8)";
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "gcd(-12, 8) should succeed, got {:?}",
            result.err()
        );
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 4, "gcd(-12, 8) should be 4"),
            Value::F64(v) => assert!(
                (v - 4.0).abs() < 1e-10,
                "gcd(-12, 8) should be 4.0, got {}",
                v
            ),
            other => panic!("Expected numeric value, got {:?}", other),
        }
    }

    fn test_prelude_lcm_uses_gcd() {
        // Test lcm which internally calls gcd (with parameter reassignment)
        let src = "lcm(4, 6)";
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "lcm(4, 6) should succeed, got {:?}",
            result.err()
        );
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 12, "lcm(4, 6) should be 12"),
            other => panic!("Expected numeric value, got {:?}", other),
        }
    }

    fn test_prelude_powermod() {
        // Test powermod which reassigns base parameter: base = base % m
        let src = "powermod(2, 10, 1000)";
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "powermod(2, 10, 1000) should succeed, got {:?}",
            result.err()
        );
        match result.unwrap() {
            Value::I64(v) => assert_eq!(v, 24, "2^10 mod 1000 should be 24"),
            Value::F64(v) => assert!(
                (v - 24.0).abs() < 1e-10,
                "2^10 mod 1000 should be 24.0, got {}",
                v
            ),
            other => panic!("Expected numeric value, got {:?}", other),
        }
    }

    // ==================== typeof tests ====================

    fn test_typeof_int64() {
        let src = r#"println(typeof(42))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Int64");
    }

    fn test_typeof_float64() {
        let src = r#"println(typeof(3.14))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Float64");
    }

    fn test_typeof_string() {
        let src = r#"println(typeof("hello"))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "String");
    }

    fn test_typeof_nothing() {
        let src = r#"println(typeof(nothing))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Nothing");
    }

    fn test_typeof_vector() {
        let src = r#"println(typeof([1.0, 2.0, 3.0]))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Vector{Float64}");
    }

    fn test_typeof_matrix() {
        let src = r#"println(typeof(zeros(2, 3)))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Matrix{Float64}");
    }

    // Range literals are now lazy (issue #520), returning UnitRange or StepRange types.
    fn test_typeof_range_as_lazy() {
        // Range literals now create lazy Range values
        let src = r#"
    r = 1:10
    println(typeof(r))
    "#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        // Range literals with integer bounds produce UnitRange{Int64} (Issue #3550).
        assert_eq!(output.trim(), "UnitRange{Int64}");
    }

    fn test_typeof_step_range_as_lazy() {
        // StepRange literals now create lazy Range values
        let src = r#"
    r = 1:2:10
    println(typeof(r))
    "#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        // StepRange literals with step produce StepRange{Int64, Int64} (Issue #3550).
        assert_eq!(output.trim(), "StepRange{Int64, Int64}");
    }

    fn test_typeof_complex() {
        // `1.0 + 2.0im` is a `Complex{Float64}`, which upstream Julia (1.x) DISPLAYS
        // through its `ComplexF64` type alias — `println(typeof(1.0 + 2.0im))` prints
        // `ComplexF64`, not `Complex{Float64}`. The runtime adopted the same alias
        // display (Issue #5775), so the prior expectation was stale (Issue #5854).
        let src = r#"println(typeof(1.0 + 2.0im))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "ComplexF64");
    }

    fn test_typeof_tuple() {
        let src = r#"println(typeof((1, 2.0, "a")))"#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Tuple{Int64, Float64, String}");
    }

    // ===========================================================================
    // @test macro tests - require `using Test`
    // ===========================================================================

    fn test_test_macro_without_using_test() {
        // @test without `using Test` should fail at lowering phase
        let src = r#"
    @test 1 + 1 == 2
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "Expected error when using @test without 'using Test', but got: {:?}",
            result
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("using Test"),
            "Error message should mention 'using Test': {}",
            err_msg
        );
    }

    fn test_testset_macro_without_using_test() {
        // @testset without `using Test` should fail at lowering phase
        let src = r#"
    @testset "Basic" begin
        x = 1
    end
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "Expected error when using @testset without 'using Test', but got: {:?}",
            result
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("using Test"),
            "Error message should mention 'using Test': {}",
            err_msg
        );
    }

    fn test_test_macro_with_using_test() {
        // @test with `using Test` should work
        let src = r#"
    using Test
    @test 1 + 1 == 2
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "Expected success when using @test with 'using Test', but got error: {:?}",
            result
        );
    }

    fn test_testset_macro_with_using_test() {
        // @testset with `using Test` should work
        let src = r#"
    using Test
    @testset "Basic" begin
        @test 1 + 1 == 2
        @test 2 * 3 == 6
    end
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_ok(),
            "Expected success when using @testset with 'using Test', but got error: {:?}",
            result
        );
    }

    // ==================== Iterator Protocol Tests ====================

    fn test_iterate_array_first() {
        // iterate(array) should return (first_element, state)
        let src = r#"
    arr = [10.0, 20.0, 30.0]
    result = iterate(arr)
    result[1]  # first element
    "#;
        let result = run_core_pipeline(src, 0);
        match result {
            Ok(Value::F64(v)) => assert!((v - 10.0).abs() < 0.001, "Expected 10.0, got {}", v),
            Ok(other) => panic!("Expected F64(10.0), got {:?}", other),
            Err(e) => panic!("iterate(array) failed: {}", e),
        }
    }

    fn test_iterate_array_next() {
        // iterate(array, state) should return next element
        let src = r#"
    arr = [10.0, 20.0, 30.0]
    first = iterate(arr)
    second = iterate(arr, first[2])
    second[1]  # second element
    "#;
        let result = run_core_pipeline(src, 0);
        match result {
            Ok(Value::F64(v)) => assert!((v - 20.0).abs() < 0.001, "Expected 20.0, got {}", v),
            Ok(other) => panic!("Expected F64(20.0), got {:?}", other),
            Err(e) => panic!("iterate(array, state) failed: {}", e),
        }
    }

    fn test_iterate_empty_array() {
        // iterate on empty array should return nothing
        // Check by using println(typeof(...)) to get the type name
        let src = r#"
    arr = zeros(0)
    println(typeof(iterate(arr)))
    "#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(output.trim(), "Nothing");
    }

    fn test_iterate_range() {
        // iterate on range
        let src = r#"
    r = 1:5
    first = iterate(r)
    first[1]  # should be 1
    "#;
        let result = run_core_pipeline(src, 0);
        match result {
            Ok(Value::I64(v)) => assert_eq!(v, 1, "Expected 1, got {}", v),
            // Range uses F64 internally, so iterate returns F64
            Ok(Value::F64(v)) => assert!((v - 1.0).abs() < 1e-10, "Expected 1.0, got {}", v),
            Ok(other) => panic!("Expected I64(1) or F64(1.0), got {:?}", other),
            Err(e) => panic!("iterate(range) failed: {}", e),
        }
    }

    fn test_collect_range() {
        // collect(range) should return an array
        let src = r#"
    r = 1:5
    arr = collect(r)
    length(arr)
    "#;
        let result = run_core_pipeline(src, 0);
        match result {
            Ok(Value::I64(5)) => (),
            Ok(other) => panic!("Expected I64(5), got {:?}", other),
            Err(e) => panic!("collect(range) failed: {}", e),
        }
    }

    fn test_collect_range_step() {
        // collect step range
        let src = r#"
    r = 1:2:9
    arr = collect(r)
    arr[3]  # should be 5 (1, 3, 5, 7, 9)
    "#;
        let result = run_core_pipeline(src, 0);
        match result {
            Ok(Value::I64(v)) => assert_eq!(v, 5, "Expected 5, got {}", v),
            Ok(other) => panic!("Expected I64(5), got {:?}", other),
            Err(e) => panic!("collect step range failed: {}", e),
        }
    }

    // ==================================================================================
    // Generator tests
    // ==================================================================================

    fn test_generator_typeof() {
        // Julia prints parametric Generator runtime types.
        let src = r#"
    square(x) = x * x
    g = Generator(square, 1:5)
    println(typeof(g))
    "#;
        let (_, output) = compile_and_run_program_direct(src, 0);
        assert_eq!(
            output.trim(),
            "Base.Generator{UnitRange{Int64}, typeof(square)}"
        );
    }

    // TODO: Generator iteration is not yet implemented
    // The VM has GeneratorValue but iterate(Generator) isn't supported yet
    // #[test]
    // fn test_generator_collect_simple() {
    //     // collect(Generator) should materialize the iterator
    //     // Note: current implementation returns underlying iterator without applying function
    //     let src = r#"
    // identity(x) = x
    // g = Generator(identity, 1:3)
    // arr = collect(g)
    // length(arr)
    // "#;
    //     let result = run_core_pipeline(src, 0);
    //     match result {
    //         Ok(Value::I64(len)) => assert_eq!(len, 3, "Expected length 3, got {}", len),
    //         Ok(other) => panic!("Expected I64(3), got {:?}", other),
    //         Err(e) => panic!("Generator collect failed: {}", e),
    //     }
    // }

    // Generated aggregate chunks for nextest process amortization.
    #[test]
    fn chunk_000() {
        test_dict_empty_constructor();
        test_dict_parametric_constructor_empty();
        test_dict_set_and_get();
        test_dict_haskey();
        test_dict_haskey_missing();
        test_dict_get_with_default();
        test_dict_get_existing_key();
        test_dict_pairs();
        test_time_println_string_literal();
        test_zero_function_float();
        test_zero_function_int();
        test_zero_function_complex();
        test_trues_function();
        test_falses_function();
        test_complex_binary_add();
        test_complex_binary_add_imag();
    }

    #[test]
    fn chunk_001() {
        test_complex_binary_mul();
        test_complex_binary_pow();
        test_complex_neg();
        test_complex_array_literal();
        test_complex_array_literal_second_element();
        test_complex_array_literal_mixed();
        test_broadcast_sqrt_core_pipeline();
        test_broadcast_abs_core_pipeline();
        test_broadcast_sin_cos_core_pipeline();
        test_broadcast_exp_log_core_pipeline();
        test_broadcast_ifelse_core_pipeline();
        test_broadcast_ifelse_non_bool_condition_error();
        test_mandelbrot_broadcast_ios_sample();
        test_try_catch_finally_debug();
        test_try_catch_finally_no_error_simple();
        test_top_level_try_preserves_only_preexisting_binding();
        test_try_catch_finally_no_error_f64();
    }

    #[test]
    fn chunk_002() {
        test_try_catch_finally_type_change();
        test_try_catch_finally_catch_type_matters();
        test_julia_broadcast_outer_product_3x3();
        test_julia_broadcast_outer_product_corners();
        test_julia_broadcast_row_col_add();
        test_julia_broadcast_2d_with_1d();
        test_julia_broadcast_same_shape_still_works();
        test_julia_broadcast_incompatible_shapes_error();
        test_broadcast_op_function_call_syntax();
        test_broadcast_add_function_call_syntax();
        test_string_interpolation_no_interpolation_string();
        test_string_interpolation_full_sample_from_bug_report();
        test_base_loads();
        test_merge_base();
        test_simple_float_plus_complex();
        test_float_plus_complex_in_function();
    }

    #[test]
    fn chunk_003() {
        test_complex_plus_typed_param();
        test_complex_with_typed_param_from_json();
        test_sum_array();
        test_sum_array_floats();
        test_sum_with_function();
        test_sum_in_expression();
        test_ref_basic();
        test_ref_broadcast_scalar();
        test_ref_broadcast_multiply();
        test_ref_multi_arg_broadcast();
        test_complex_array_basic_ops();
        test_ref_multi_arg_broadcast_complex();
        test_2d_broadcast_shape_preservation();
        test_range_with_length_output();
        test_range_with_length_in_function();
        test_range_positional_in_function();
    }

    #[test]
    fn chunk_004() {
        test_kwarg_from_function_param();
        test_2d_broadcast_mandelbrot();
        test_sleep_basic_float();
        test_sleep_integer();
        test_sleep_zero();
        test_sleep_returns_nothing();
        test_sleep_negative_error();
        test_sleep_infinity_error();
        test_sleep_nan_error();
        test_sleep_in_loop();
        test_base_min_function();
        test_rational_struct_direct();
        test_rational_operator_basic();
        test_rational_operator_denominator();
        test_rational_operator_with_expressions();
        test_rational_operator_negative();
    }

    #[test]
    fn chunk_005() {
        test_prelude_gcd_basic();
        test_prelude_gcd_negative();
        test_prelude_lcm_uses_gcd();
        test_prelude_powermod();
    }

    #[test]
    fn chunk_007() {
        test_typeof_int64();
        test_typeof_float64();
        test_typeof_string();
        test_typeof_nothing();
    }

    #[test]
    fn chunk_008() {
        test_typeof_vector();
        test_typeof_matrix();
        test_typeof_range_as_lazy();
        test_typeof_step_range_as_lazy();
    }

    #[test]
    fn chunk_009() {
        test_typeof_complex();
        test_typeof_tuple();
        test_test_macro_without_using_test();
        test_testset_macro_without_using_test();
    }

    #[test]
    fn chunk_006() {
        test_test_macro_with_using_test();
        test_testset_macro_with_using_test();
        test_iterate_array_first();
        test_iterate_array_next();
        test_iterate_empty_array();
        test_iterate_range();
        test_collect_range();
        test_collect_range_step();
        test_generator_typeof();
    }
}

mod integration_module_base_tests {
    //! Integration tests: Module support, Base module, basic arithmetic, random numbers
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::Value;

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
        // Plain `import` binds only the module name, matching Julia.
        let src = r#"
    module Utils
        function double(x)
            return x * 2
        end
    end

    import Utils

    result = Utils.double(7)
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
    using Random
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
    using Random
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
    using Random
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
        // Issue #9328: rand(Int, n) is a Vector{Int64} of full-range random
        // integers (negatives included, matching upstream and the scalar
        // rand(rng, Int) stream) — NOT a Float64-backed non-negative array as the
        // old `.abs()`-based RandIntArray runtime produced. Assert the element type
        // and integer-ness, not a (now-incorrect) non-negativity invariant.
        let src = r#"
    arr = rand(Int, 5)
    @assert length(arr) == 5
    @assert eltype(arr) == Int64
    ok = true
    for i in 1:5
        ok = ok && (arr[i] isa Int64)
    end
    Float64(ok)
    "#;
        let result = compile_and_run_str(src, 42);
        assert_eq!(
            result, 1.0,
            "Expected rand(Int, 5) to be a Vector{{Int64}} with Int64 elements, got: {result}"
        );
    }

    fn test_rand_int64_array() {
        // Issue #9328: rand(Int64, m, n) is a Matrix{Int64}.
        let src = r#"
    mat = rand(Int64, 2, 3)
    @assert length(mat) == 6
    @assert eltype(mat) == Int64
    Float64(mat[1, 1] isa Int64)
    "#;
        let result = compile_and_run_str(src, 42);
        assert_eq!(
            result, 1.0,
            "Expected rand(Int64, 2, 3) to have Int64 elements"
        );
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
}

mod integration_string_type_tests {
    //! Integration tests: Char, string methods, math constants, abstract types, BigInt, numeric literals
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm::*;
    use subset_julia_vm_bytecode::Value;

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
            Value::Str(s) => assert_eq!(s.as_ref(), "HELLO"),
            other => panic!("Expected Str(HELLO), got {:?}", other),
        }
    }

    fn test_string_lowercase() {
        let src = r#"lowercase("HELLO")"#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s.as_ref(), "hello"),
            other => panic!("Expected Str(hello), got {:?}", other),
        }
    }

    fn test_string_strip() {
        let src = r#"strip("  hello  ")"#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s.as_ref(), "hello"),
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
            Value::Str(s) => assert_eq!(s.as_ref(), "ababab"),
            other => panic!("Expected Str(ababab), got {:?}", other),
        }
    }

    fn test_string_chop() {
        let src = r#"chop("hello")"#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s.as_ref(), "hell"),
            other => panic!("Expected Str(hell), got {:?}", other),
        }
    }

    fn test_string_chomp() {
        let src = r#"chomp("hello\n")"#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s.as_ref(), "hello"),
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
            Value::Str(s) => assert_eq!(s.as_ref(), "a"),
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
            Value::Str(s) => assert_eq!(s.as_ref(), "HéLLO"),
            other => panic!("Expected Str(HéLLO), got {:?}", other),
        }
    }

    fn test_multibyte_lowercase() {
        // lowercase works on ASCII characters; non-ASCII characters pass through unchanged
        // (Pure Julia implementation in base/strings/unicode.jl, Issue #2565)
        let src = r#"lowercase("HÉLLO")"#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::Str(s) => assert_eq!(s.as_ref(), "hÉllo"),
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
            Ok(Value::Str(s)) => assert_eq!(s.as_ref(), "BigInt"),
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
                    e.contains("non-boolean")
                        && e.contains("Int64")
                        && e.contains("boolean context"),
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
                    e.contains("non-boolean")
                        && e.contains("Int64")
                        && e.contains("boolean context"),
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
}

mod integration_struct_hof_tests {
    //! Integration tests: Structs, parametric types, HOFs, kwargs, do syntax, randn, iOS samples
    #![allow(dead_code)]

    use crate::common::*;

    use subset_julia_vm_bytecode::Value;

    // ==================== Struct Tests ====================
    // These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

    fn test_struct_basic_immutable() {
        // Basic immutable struct with typed fields
        let src = r#"
    struct Point
        x::Float64
        y::Float64
    end

    p = Point(3.0, 4.0)
    p.x + p.y
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
        match result {
            Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_struct_field_access() {
        // Test accessing individual fields
        let src = r#"
    struct Vector2D
        x::Float64
        y::Float64
    end

    v = Vector2D(10.0, 20.0)
    v.y - v.x
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
        match result {
            Value::F64(x) => assert!((x - 10.0).abs() < 1e-10, "Expected 10.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 10, "Expected 10, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_mutable_struct_field_assignment() {
        // Test mutable struct field assignment
        let src = r#"
    mutable struct Counter
        value::Float64
    end

    c = Counter(0.0)
    c.value = 42.0
    c.value
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
        match result {
            Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_struct_in_expression() {
        // Test struct fields in arithmetic expressions
        let src = r#"
    struct Rectangle
        width::Float64
        height::Float64
    end

    r = Rectangle(5.0, 3.0)
    area = r.width * r.height
    area
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
        match result {
            Value::F64(x) => assert!((x - 15.0).abs() < 1e-10, "Expected 15.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 15, "Expected 15, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_struct_euclidean_distance() {
        // Test struct with sqrt calculation (Euclidean distance)
        let src = r#"
    struct Point
        x::Float64
        y::Float64
    end

    p1 = Point(0.0, 0.0)
    p2 = Point(3.0, 4.0)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run struct test");
        match result {
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Parametric Type Tests ====================

    fn test_parametric_struct_explicit_type() {
        // Test parametric struct with explicit type parameter
        let src = r#"
    struct Point{T}
        x::T
        y::T
    end

    p = Point{Float64}(3.0, 4.0)
    p.x + p.y
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct test");
        match result {
            Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_struct_type_inference() {
        // Test parametric struct with type inference from constructor arguments
        let src = r#"
    struct Point{T}
        x::T
        y::T
    end

    p = Point(1.5, 2.5)
    p.x + p.y
    "#;
        let result =
            run_core_pipeline(src, 0).expect("Failed to run parametric struct type inference test");
        match result {
            Value::F64(x) => assert!((x - 4.0).abs() < 1e-10, "Expected 4.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 4, "Expected 4, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_struct_int_type() {
        // Test parametric struct with Int64 type parameter
        let src = r#"
    struct Point{T}
        x::T
        y::T
    end

    p = Point{Int64}(3, 4)
    p.x + p.y
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct int test");
        match result {
            Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
            Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_struct_multiple_params() {
        // Test parametric struct with multiple type parameters
        let src = r#"
    struct Pair{A, B}
        first::A
        second::B
    end

    pair = Pair{Int64, Float64}(10, 2.5)
    pair.second * 4.0
    "#;
        let result = run_core_pipeline(src, 0)
            .expect("Failed to run parametric struct multiple params test");
        match result {
            Value::F64(x) => assert!((x - 10.0).abs() < 1e-10, "Expected 10.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 10, "Expected 10, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_struct_with_bound() {
        // Test parametric struct with type bound
        let src = r#"
    struct Numeric{T<:Number}
        value::T
    end

    n = Numeric{Float64}(42.0)
    n.value
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run parametric struct bound test");
        match result {
            Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_nested_array_field() {
        // Test parametric struct with Array{T} field type
        let src = r#"
    struct Container{T}
        items::Array{T}
    end

    c = Container{Float64}([1.0, 2.0, 3.0])
    c.items[1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run nested array field test");
        match result {
            Value::F64(x) => assert!((x - 1.0).abs() < 1e-10, "Expected 1.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_nested_struct_type() {
        // Test nested parametric struct: Container{Point{Float64}}
        let src = r#"
    struct Point{T}
        x::T
        y::T
    end

    struct Container{T}
        item::T
    end

    p = Point{Float64}(1.0, 2.0)
    c = Container{Point{Float64}}(p)
    c.item.x + c.item.y
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run nested struct type test");
        match result {
            Value::F64(x) => assert!((x - 3.0).abs() < 1e-10, "Expected 3.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 3, "Expected 3, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_parametric_double_nested() {
        // Test doubly nested parametric struct: Wrapper{Wrapper{Float64}}
        let src = r#"
    struct Wrapper{T}
        value::T
    end

    inner = Wrapper{Float64}(42.0)
    outer = Wrapper{Wrapper{Float64}}(inner)
    outer.value.value
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run double nested test");
        match result {
            Value::F64(x) => assert!((x - 42.0).abs() < 1e-10, "Expected 42.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 42, "Expected 42, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Break and Continue ====================

    fn test_while_break() {
        // Test break statement in while loop
        let src = r#"
    i = 0
    sum = 0
    while i < 10
        i += 1
        sum += i
        if sum > 20
            break
        end
    end
    sum
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run break test");
        match result {
            Value::I64(x) => assert!(x > 20, "Expected sum > 20, got {}", x),
            Value::F64(x) => assert!(x > 20.0, "Expected sum > 20, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_for_break() {
        // Test break statement in for loop
        let src = r#"
    sum = 0
    for i in 1:100
        sum += i
        if sum > 50
            break
        end
    end
    sum
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run for break test");
        match result {
            Value::I64(x) => assert!(x > 50, "Expected sum > 50, got {}", x),
            Value::F64(x) => assert!(x > 50.0, "Expected sum > 50, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_while_continue() {
        // Test continue statement in while loop (skip numbers less than 5)
        let src = r#"
    i = 0
    sum = 0
    while i < 10
        i += 1
        if i < 5
            continue
        end
        sum += i
    end
    sum
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run continue test");
        match result {
            Value::I64(x) => assert_eq!(x, 45, "Expected sum of 5+6+7+8+9+10 = 45, got {}", x),
            Value::F64(x) => assert!((x - 45.0).abs() < 1e-10, "Expected 45.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_for_continue() {
        // Test continue statement in for loop (skip numbers less than 5)
        let src = r#"
    sum = 0
    for i in 1:10
        if i < 5
            continue
        end
        sum += i
    end
    sum
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run for continue test");
        match result {
            Value::I64(x) => assert_eq!(x, 45, "Expected sum of 5+6+7+8+9+10 = 45, got {}", x),
            Value::F64(x) => assert!((x - 45.0).abs() < 1e-10, "Expected 45.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_nested_loops_break() {
        // Test break in nested loops (should only break inner loop)
        let src = r#"
    outer_sum = 0
    inner_sum = 0
    for i in 1:5
        outer_sum += i
        for j in 1:10
            inner_sum += j
            if inner_sum > 20
                break
            end
        end
    end
    outer_sum + inner_sum
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run nested break test");
        match result {
            Value::I64(x) => assert!(x > 20, "Expected sum > 20, got {}", x),
            Value::F64(x) => assert!(x > 20.0, "Expected sum > 20, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Try/Catch/Finally ====================

    fn test_try_catch_finally_with_message() {
        let src = r#"
    x = 0
    try
        # Use integer division to trigger error (float division returns Inf per IEEE 754)
        y = div(1, 0)
    catch e
        x = 10
        println(e)
    finally
        x += 1
    end
    x
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run try/catch/finally test");
        match result {
            Value::I64(x) => assert_eq!(x, 11, "Expected 11, got {}", x),
            Value::F64(x) => assert!((x - 11.0).abs() < 1e-10, "Expected 11.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_try_else_finally_no_error() {
        let src = r#"
    x = 0
    try
        x = 1
    catch e
        x = 2
    else
        x += 3
    finally
        x += 4
    end
    x
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run try/else/finally test");
        match result {
            Value::I64(x) => assert_eq!(x, 8, "Expected 8, got {}", x),
            Value::F64(x) => assert!((x - 8.0).abs() < 1e-10, "Expected 8.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_try_catch_finally_no_error_no_else() {
        // This is the failing case from code_samples_tests
        let src = r#"
    result = 0
    try
        result = 10 / 2
    catch e
        result = -1
    finally
        cleanup_done = 1
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run try/catch/finally test");
        match result {
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Array Slicing ====================

    fn test_slice_range_1d() {
        let src = r#"
    a = [10, 20, 30, 40]
    b = a[1:3]
    b[2] + b[3]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run slice range test");
        match result {
            Value::F64(x) => assert!((x - 50.0).abs() < 1e-10, "Expected 50.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 50, "Expected 50, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_slice_full_matrix() {
        let src = r#"
    m = [1 2; 3 4]
    s = m[:, :]
    s[1, 2] + s[2, 1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run full slice test");
        match result {
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Phase A: Mandelbrot Broadcast Support ====================

    fn test_transpose_1d_array() {
        // 1D array [n] becomes row vector [1, n]
        let src = r#"
    a = [1.0, 2.0, 3.0]
    b = a'
    length(b)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run transpose 1D test");
        match result {
            Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_transpose_2d_basic() {
        // Test that transpose changes the shape
        // [1 2; 3 4] is 2x2, transpose is also 2x2
        // Just verify transpose runs without error
        let src = r#"
    m = [1 2; 3 4]
    t = m'
    t[1, 1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run transpose 2D test");
        match result {
            Value::F64(x) => assert!((x - 1.0).abs() < 1e-10, "Expected 1.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_im_literal() {
        // im should be complex(0, 1)
        // Use complex operations to verify
        let src = r#"
    z = im
    z
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run im literal test");
        if let Some((re, im)) = result.as_complex_parts() {
            assert!((re - 0.0).abs() < 1e-10, "Expected real=0.0, got {}", re);
            assert!((im - 1.0).abs() < 1e-10, "Expected imag=1.0, got {}", im);
        } else {
            panic!("Unexpected result type: {:?}", result);
        }
    }

    fn test_im_in_expression() {
        // Test that im can be used in assignment and returned
        // Full Complex arithmetic (2.0 * im) is planned for Phase C
        let src = r#"
    a = im
    b = im
    a
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run im expression test");
        if let Some((re, imag)) = result.as_complex_parts() {
            assert!((re - 0.0).abs() < 1e-10, "Expected real=0.0, got {}", re);
            assert!(
                (imag - 1.0).abs() < 1e-10,
                "Expected imag=1.0, got {}",
                imag
            );
        } else {
            panic!("Unexpected result type: {:?}", result);
        }
    }

    fn test_range_with_length() {
        // range(0.0, 1.0; length=5) should give [0.0, 0.25, 0.5, 0.75, 1.0]
        let src = r#"
    xs = range(0.0, 1.0; length=5)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range length test");
        match result {
            Value::I64(x) => assert_eq!(x, 5, "Expected length 5, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_range_length_first_element() {
        // range(0.0, 1.0; length=5) first element should be 0.0
        let src = r#"
    xs = range(0.0, 1.0; length=5)
    xs[1]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range first element test");
        match result {
            Value::F64(x) => assert!((x - 0.0).abs() < 1e-10, "Expected 0.0, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_transpose_with_broadcast() {
        // xs' .+ 0 should give a row vector
        let src = r#"
    xs = [1.0, 2.0, 3.0]
    ys = xs' .+ 0.0
    length(ys)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run transpose broadcast test");
        match result {
            Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_transpose_function() {
        // transpose() function should work like ' for real arrays
        let src = r#"
    xs = [1.0, 2.0, 3.0]
    ys = transpose(xs)
    length(ys)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run transpose function test");
        match result {
            Value::I64(x) => assert_eq!(x, 3, "Expected length 3, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_function_definition_simple() {
        // Test function with keyword arguments using defaults
        // Note: explicit return is required for correct behavior
        let src = r#"
    function f(x; y=10)
        return x + y
    end
    f(5)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
        match result {
            Value::I64(v) => assert_eq!(v, 15, "Expected 15, got {}", v),
            Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_function_with_explicit_kwarg() {
        // Test function with keyword argument explicitly provided
        let src = r#"
    function f(x; y=10)
        return x + y
    end
    f(5; y=20)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
        match result {
            Value::I64(v) => assert_eq!(v, 25, "Expected 25, got {}", v),
            Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_function_multiple_kwargs() {
        // Test function with multiple keyword arguments
        let src = r#"
    function f(x; y=1, z=2)
        return x + y + z
    end
    f(10)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
        match result {
            Value::I64(v) => assert_eq!(v, 13, "Expected 13, got {}", v),
            Value::F64(v) => assert!((v - 13.0).abs() < 1e-10, "Expected 13.0, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_function_multiple_kwargs_partial_override() {
        // Test overriding only some keyword arguments
        let src = r#"
    function f(x; y=1, z=2)
        return x + y + z
    end
    f(10; z=100)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg test");
        match result {
            Value::I64(v) => assert_eq!(v, 111, "Expected 111, got {}", v),
            Value::F64(v) => assert!((v - 111.0).abs() < 1e-10, "Expected 111.0, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    /// Test keyword argument with float default value (Issue #1328).
    /// This test verifies that functions with float default kwargs compile and run correctly.
    fn test_kwarg_function_float_default() {
        // Test keyword argument with float default
        let src = r#"
    function f(x; y=1.5)
        return x + y
    end
    f(2.0)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg float default test");
        match result {
            Value::F64(v) => assert!((v - 3.5).abs() < 1e-10, "Expected 3.5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    /// Test keyword argument with float default and explicit override (Issue #1328).
    fn test_kwarg_function_float_default_override() {
        let src = r#"
    function f(x; y=1.5)
        return x + y
    end
    f(2.0; y=0.5)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run kwarg override test");
        match result {
            Value::F64(v) => assert!((v - 2.5).abs() < 1e-10, "Expected 2.5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_range_length() {
        // Test range with length keyword argument (already implemented)
        let src = r#"
    xs = range(0.0, 1.0; length=5)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range test");
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_kwarg_range_length_int() {
        // Test range with Int64 length keyword argument
        // Note: Julia's range(start, stop; length=N) requires Integer for length
        let src = r#"
    n = 5
    xs = range(0.0, 1.0; length=n)
    length(xs)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run range test with Int64 length");
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Short Function Definition ====================

    fn test_short_function_single_arg() {
        // f(x) = x^2
        let src = r#"
    f(x) = x^2
    f(3)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
        match result {
            Value::F64(v) => assert!((v - 9.0).abs() < 1e-10, "Expected 9.0, got {}", v),
            Value::I64(v) => assert_eq!(v, 9, "Expected 9, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_short_function_two_args() {
        // f(x, y) = x + y
        let src = r#"
    add(x, y) = x + y
    add(10, 32)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
        match result {
            Value::F64(v) => assert!((v - 42.0).abs() < 1e-10, "Expected 42.0, got {}", v),
            Value::I64(v) => assert_eq!(v, 42, "Expected 42, got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_short_function_expression_body() {
        // Expression with multiple operations in body
        let src = r#"
    compute(a, b, c) = a + b * c
    compute(1, 2, 3)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run short function test");
        match result {
            Value::F64(v) => assert!((v - 7.0).abs() < 1e-10, "Expected 7.0 (1+2*3), got {}", v),
            Value::I64(v) => assert_eq!(v, 7, "Expected 7 (1+2*3), got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_short_function_with_regular_function() {
        // Short function and regular function in same file
        let src = r#"
    square(x) = x^2

    function cube(x)
        return x^3
    end

    square(3) + cube(2)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run mixed function test");
        match result {
            Value::F64(v) => assert!((v - 17.0).abs() < 1e-10, "Expected 17.0 (9+8), got {}", v),
            Value::I64(v) => assert_eq!(v, 17, "Expected 17 (9+8), got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_short_function_multiple_definitions() {
        // Multiple short function definitions
        let src = r#"
    double(x) = 2 * x
    triple(x) = 3 * x
    quadruple(x) = 4 * x

    double(5) + triple(5) + quadruple(5)
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run multiple short function test");
        match result {
            Value::F64(v) => assert!(
                (v - 45.0).abs() < 1e-10,
                "Expected 45.0 (10+15+20), got {}",
                v
            ),
            Value::I64(v) => assert_eq!(v, 45, "Expected 45 (10+15+20), got {}", v),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_short_function_no_args() {
        // Short function with no arguments
        let src = r#"
    get_pi() = 3.14159
    get_pi()
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run short function no args test");
        match result {
            Value::F64(v) => {
                let expected = 314_159.0 / 100_000.0;
                assert!(
                    (v - expected).abs() < 1e-10,
                    "Expected {}, got {}",
                    expected,
                    v
                );
            }
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Lambda Assignment Tests ====================

    fn test_lambda_assignment_basic() {
        // Test: f = x -> x ^ 3 + 1
        let src = r#"
    f = x -> x ^ 3 + 1
    f(2)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        // 2^3 + 1 = 9
        match result {
            Value::I64(v) => assert_eq!(v, 9),
            Value::F64(v) => assert!((v - 9.0).abs() < 1e-10),
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    fn test_lambda_assignment_multi_param() {
        let src = r#"
    f = (x, y) -> x + y * 2
    f(3, 4)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        // 3 + 4*2 = 11
        match result {
            Value::I64(v) => assert_eq!(v, 11),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_lambda_assignment_simple() {
        let src = r#"
    square = x -> x * x
    square(5)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 25),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    // ==================== Higher-Order Function Tests ====================

    fn test_map_with_lambda() {
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    result = map(x -> x * 2.0, arr)
    result[3]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 6.0).abs() < 1e-10),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_map_with_named_function() {
        let src = r#"
    double(x) = x * 2.0
    arr = [1.0, 2.0, 3.0]
    result = map(double, arr)
    result[2]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_filter_with_lambda() {
        let src = r#"
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = filter(x -> x > 2.5, arr)
    length(result)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 3), // [3.0, 4.0, 5.0]
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_filter_first_element() {
        let src = r#"
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = filter(x -> x > 2.5, arr)
    result[1]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 3.0).abs() < 1e-10),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_reduce_with_lambda() {
        let src = r#"
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    reduce((a, b) -> a + b, arr)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 15.0).abs() < 1e-10), // 1+2+3+4+5 = 15
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_reduce_with_init() {
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    reduce((a, b) -> a + b, arr, 10.0)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 16.0).abs() < 1e-10), // 10+1+2+3 = 16
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_sum_with_lambda() {
        let src = r#"
    arr = [1.0, 2.0, 3.0, 4.0]
    sum(x -> x * x, arr)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 30.0).abs() < 1e-10), // 1+4+9+16 = 30
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_sum_with_implicit_mult_lambda() {
        // Test sum(x -> 2x, [1,2,3]) - implicit multiplication in lambda
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    sum(x -> 2x, arr)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 12.0).abs() < 1e-10), // 2*1 + 2*2 + 2*3 = 12
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_sum_with_implicit_mult_inline_array() {
        // Test sum(x -> 2x, [1,2,3]) with inline integer array
        let src = r#"
    sum(x -> 2x, [1, 2, 3])
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 12.0).abs() < 1e-10), // 2*1 + 2*2 + 2*3 = 12
            Value::I64(v) => assert_eq!(v, 12),
            _ => panic!("Expected F64 or I64, got {:?}", result),
        }
    }

    fn test_map_with_implicit_mult_lambda() {
        // Test map with implicit multiplication: map(x -> 3x, [1,2,3])
        let src = r#"
    result = map(x -> 3x, [1.0, 2.0, 3.0])
    result[2]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 6.0).abs() < 1e-10), // 3*2 = 6
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_sum_with_named_function() {
        let src = r#"
    square(x) = x * x
    arr = [1.0, 2.0, 3.0]
    sum(square, arr)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 14.0).abs() < 1e-10), // 1+4+9 = 14
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_map_empty_array() {
        let src = r#"
    arr = zeros(0)
    result = map(x -> x * 2.0, arr)
    length(result)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_filter_empty_result() {
        let src = r#"
    arr = [1.0, 2.0, 3.0]
    result = filter(x -> x > 10.0, arr)
    length(result)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 0),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_sum_empty_array() {
        let src = r#"
    arr = zeros(0)
    sum(x -> x * x, arr)
    "#;
        let err = run_core_pipeline(src, 0).unwrap_err();
        assert!(
            err.contains("reducing over an empty collection is not allowed"),
            "Unexpected error: {err}"
        );
    }

    // ==================== do Syntax Tests ====================

    fn test_do_syntax_map() {
        let src = r#"
    result = map([1.0, 2.0, 3.0]) do x
        x * 2.0
    end
    result[2]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 4.0).abs() < 1e-10),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_do_syntax_filter() {
        let src = r#"
    result = filter([1.0, 2.0, 3.0, 4.0, 5.0]) do x
        x > 2.5
    end
    length(result)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 3), // [3.0, 4.0, 5.0]
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_do_syntax_reduce() {
        let src = r#"
    reduce([1.0, 2.0, 3.0, 4.0, 5.0]) do a, b
        a + b
    end
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 15.0).abs() < 1e-10), // 1+2+3+4+5 = 15
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_do_syntax_sum() {
        let src = r#"
    sum([1.0, 2.0, 3.0, 4.0]) do x
        x * x
    end
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 30.0).abs() < 1e-10), // 1+4+9+16 = 30
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_do_syntax_multiline() {
        let src = r#"
    result = map([1.0, 2.0, 3.0]) do x
        y = x * 2.0
        y + 1.0
    end
    result[1]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 3.0).abs() < 1e-10), // 1*2+1 = 3
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== Standard Normal Distribution (randn) ====================

    fn test_randn_basic() {
        let src = r#"
    x = randn()
    x
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::F64(v) => {
                // Standard normal values are typically within [-4, 4] (99.99%)
                assert!(
                    v > -10.0 && v < 10.0,
                    "randn() value {} seems out of range",
                    v
                );
            }
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_randn_deterministic() {
        let src = r#"
    randn()
    "#;
        // Same seed should produce same result
        let r1 = run_core_pipeline(src, 42).unwrap();
        let r2 = run_core_pipeline(src, 42).unwrap();
        match (&r1, &r2) {
            (Value::F64(v1), Value::F64(v2)) => {
                assert_eq!(*v1, *v2, "randn() should be deterministic");
            }
            _ => panic!("Expected F64"),
        }

        // Different seed should produce different result
        let r3 = run_core_pipeline(src, 123).unwrap();
        match (&r1, &r3) {
            (Value::F64(v1), Value::F64(v3)) => {
                assert_ne!(*v1, *v3, "Different seeds should produce different results");
            }
            _ => panic!("Expected F64"),
        }
    }

    fn test_randn_array_1d() {
        let src = r#"
    arr = randn(5)
    length(arr)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => {
                assert_eq!(v, 5, "Expected length 5");
            }
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_randn_array_2d() {
        let src = r#"
    mat = randn(3, 4)
    length(mat)
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::I64(v) => {
                assert_eq!(v, 12, "Expected length 12");
            }
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_randn_multiple_calls() {
        let src = r#"
    x1 = randn()
    x2 = randn()
    x3 = randn()
    x1 + x2 + x3
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::F64(_) => {} // Just check it succeeds
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== iOS Sample Tests ====================
    // These tests correspond to the iOS app code samples to ensure they all work correctly.

    // ==================== Higher-Order Functions Samples ====================

    fn test_ios_sample_map_function() {
        let src = r#"
    # map(f, arr) applies function f to each element
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Using lambda (anonymous function)
    doubled = map(x -> x * 2.0, arr)

    # Using named function
    square(x) = x * x
    squared = map(square, arr)

    squared[5]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 25.0).abs() < 1e-10, "Expected 25.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_filter_function() {
        // Note: Using > 5 instead of modulo since % is not supported in core pipeline
        let src = r#"
    arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
    large = filter(x -> x > 5, arr)
    length(large)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, 5, "Expected 5 (values > 5), got {}", v),
            _ => panic!("Expected I64, got {:?}", result),
        }
    }

    fn test_ios_sample_reduce_function() {
        let src = r#"
    # reduce(f, arr) combines elements using binary function f
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Sum all elements
    total = reduce((a, b) -> a + b, arr)

    # Product of all elements
    product = reduce((a, b) -> a * b, arr)

    # With initial value: reduce(f, arr, init)
    total_with_init = reduce((a, b) -> a + b, arr, 100.0)

    product
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 120.0).abs() < 1e-10, "Expected 120.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_do_syntax_map() {
        let src = r#"
    # do...end block creates anonymous function as first argument
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]

    # Using do syntax (equivalent to map(x -> x^2 + 1, arr))
    result = map(arr) do x
        x^2 + 1
    end

    result[5]
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - 26.0).abs() < 1e-10,
                "Expected 26.0 (5^2 + 1), got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_do_syntax_filter_reduce() {
        let src = r#"
    # do syntax works with filter and reduce too
    data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

    # Filter with do block
    filtered = filter(data) do x
        x > 3 && x < 8
    end

    # Reduce with do block (multiple parameters)
    total = reduce(data) do acc, val
        acc + val
    end

    total
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 55.0).abs() < 1e-10, "Expected 55.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_chaining_higher_order() {
        let src = r#"
    # Chain map, filter, reduce for data processing pipelines
    data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]

    # Step 1: Square all numbers
    squared = map(x -> x * x, data)

    # Step 2: Keep only those > 20
    large_squares = filter(x -> x > 20, squared)

    # Step 3: Sum them
    total = reduce((a, b) -> a + b, large_squares)

    total
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        // Squares > 20: 25, 36, 49, 64, 81, 100 -> sum = 355
        match result {
            Value::F64(v) => assert!((v - 355.0).abs() < 1e-10, "Expected 355.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== Structures Samples ====================

    fn test_ios_sample_basic_struct() {
        let src = r#"
    # Define an immutable struct with typed fields
    struct Point
        x::Float64
        y::Float64
    end

    # Create instances
    p1 = Point(3.0, 4.0)

    # Use in calculations
    distance = sqrt(p1.x^2 + p1.y^2)

    distance
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 5.0).abs() < 1e-10, "Expected 5.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_mutable_struct() {
        let src = r#"
    # Mutable structs allow field modification
    mutable struct Counter
        value::Float64
    end

    # Create and modify
    c = Counter(0.0)

    c.value = 10.0
    c.value = c.value + 5.0

    # Use in a loop
    for i in 1:5
        c.value = c.value + 1.0
    end

    c.value
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 20.0).abs() < 1e-10, "Expected 20.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_struct_with_functions() {
        // Note: Struct field access in functions requires main block calculation
        let src = r#"
    struct Rectangle
        width::Float64
        height::Float64
    end
    rect = Rectangle(5.0, 3.0)
    area = rect.width * rect.height
    area
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 15.0).abs() < 1e-10, "Expected 15.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_euclidean_distance() {
        // Note: Using main block for struct field access (same as existing test_struct_euclidean_distance)
        let src = r#"
    struct Point
        x::Float64
        y::Float64
    end
    p1 = Point(0.0, 0.0)
    p2 = Point(3.0, 4.0)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 5.0).abs() < 1e-10, "Expected 5.0, got {}", v),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_particle_simulation() {
        // Note: Using main block for mutable struct field access
        let src = r#"
    mutable struct Particle
        x::Float64
        y::Float64
        vx::Float64
        vy::Float64
    end
    particle = Particle(0.0, 0.0, 1.0, 0.5)
    dt = 0.1
    for t in 1:10
        particle.x = particle.x + particle.vx * dt
        particle.y = particle.y + particle.vy * dt
    end
    sqrt(particle.x^2 + particle.y^2)
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        // After 10 steps: x = 1.0, y = 0.5, distance = sqrt(1.25) ≈ 1.118
        match result {
            Value::F64(v) => assert!(
                (v - 1.118033988749895).abs() < 1e-10,
                "Expected ~1.118, got {}",
                v
            ),
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    // ==================== Error Handling Samples ====================

    fn test_ios_sample_try_catch_basics() {
        let src = r#"
    # try/catch handles runtime errors gracefully
    x = 0

    try
        # Use integer division to trigger error (float division returns Inf per IEEE 754)
        y = div(1, 0)
        x = 999  # This won't execute
    catch e
        # e contains the error message
        x = -1
    end

    x
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::I64(v) => assert_eq!(v, -1, "Expected -1 (caught error), got {}", v),
            Value::F64(v) => assert!((v - (-1.0)).abs() < 1e-10, "Expected -1.0, got {}", v),
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    // Note: test_ios_sample_try_catch_finally skipped - try block assignment without error
    // has known issues. Use test_ios_sample_try_catch_basics for error catching tests.

    fn test_ios_sample_error_recovery() {
        let src = r#"
    result = 0.0
    try
        result = 100 / 5
    catch e
        result = 0.0
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!((v - 20.0).abs() < 1e-10, "Expected 20.0, got {}", v),
            Value::I64(v) => assert_eq!(v, 20, "Expected 20, got {}", v),
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    fn test_ios_sample_error_recovery_with_error() {
        let src = r#"
    result = 0.0
    try
        # Use integer division to trigger error (float division returns Inf per IEEE 754)
        result = Float64(div(10, 0))
    catch e
        result = 0.0
    end
    result
    "#;
        let result = run_core_pipeline(src, 0).unwrap();
        match result {
            Value::F64(v) => assert!(
                (v - 0.0).abs() < 1e-10,
                "Expected 0.0 (error recovery), got {}",
                v
            ),
            Value::I64(v) => assert_eq!(v, 0, "Expected 0 (error recovery), got {}", v),
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    // ==================== Monte Carlo randn Samples ====================

    fn test_ios_sample_normal_distribution() {
        let src = r#"
    arr = randn(10)
    sum = 0.0
    for i in 1:length(arr)
        sum = sum + arr[i]
    end
    mean = sum / length(arr)
    mean
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::F64(v) => {
                // Mean should be reasonably close to 0 for 10 samples
                assert!(v.abs() < 2.0, "Mean {} seems too far from 0", v);
            }
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_normal_distribution_matrix() {
        let src = r#"
    mat = randn(3, 4)
    sum = 0.0
    sum_sq = 0.0
    n = 12
    for i in 1:3
        for j in 1:4
            v = mat[i, j]
            sum = sum + v
            sum_sq = sum_sq + v * v
        end
    end
    mean = sum / n
    variance = sum_sq / n - mean * mean
    std = sqrt(variance)
    std
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::F64(v) => {
                // Standard deviation should be somewhat close to 1
                assert!(v > 0.0, "Std should be positive, got {}", v);
            }
            _ => panic!("Expected F64, got {:?}", result),
        }
    }

    fn test_ios_sample_histogram_visualization() {
        // Simplified test: count values in [-1, 1] range
        let src = r#"
    n = 100
    samples = randn(n)
    count = 0
    for i in 1:n
        x = samples[i]
        if x >= -1 && x < 1
            count = count + 1
        end
    end
    count
    "#;
        let result = run_core_pipeline(src, 42).unwrap();
        match result {
            Value::F64(v) => {
                // About 68% of values should be within [-1, 1]
                assert!(
                    v > 40.0 && v < 95.0,
                    "Expected ~68 values in [-1,1], got {}",
                    v
                );
            }
            Value::I64(v) => {
                assert!(v > 40 && v < 95, "Expected ~68 values in [-1,1], got {}", v);
            }
            _ => panic!("Expected numeric result, got {:?}", result),
        }
    }

    // ==================== Broadcast Compound Assignment ====================
    // These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

    fn test_broadcast_add_assign() {
        // Test .+= broadcast compound assignment
        let src = r#"
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    a .+= b
    # a should be [11, 22, 33]
    a[1] + a[2] + a[3]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run broadcast add assign test");
        match result {
            Value::F64(x) => assert!((x - 66.0).abs() < 1e-10, "Expected 66.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 66, "Expected 66, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_broadcast_mul_assign() {
        // Test .*= broadcast compound assignment
        let src = r#"
    a = [1.0, 2.0, 3.0]
    b = [2.0, 3.0, 4.0]
    a .*= b
    # a should be [2, 6, 12]
    a[1] + a[2] + a[3]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run broadcast mul assign test");
        match result {
            Value::F64(x) => assert!((x - 20.0).abs() < 1e-10, "Expected 20.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 20, "Expected 20, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_broadcast_sub_assign() {
        // Test .-= broadcast compound assignment
        let src = r#"
    a = [10.0, 20.0, 30.0]
    b = [1.0, 2.0, 3.0]
    a .-= b
    # a should be [9, 18, 27]
    a[1] + a[2] + a[3]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run broadcast sub assign test");
        match result {
            Value::F64(x) => assert!((x - 54.0).abs() < 1e-10, "Expected 54.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 54, "Expected 54, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_broadcast_and_assign() {
        // Test .&= broadcast compound assignment
        // Bitwise & is only defined for integer types, not Float64 (Issue #2704)
        let src = r#"
    a = [1, 1, 0, 0]
    b = [1, 0, 1, 0]
    a .&= b
    # a should be [1, 0, 0, 0]
    a[1] + a[2] + a[3] + a[4]
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run broadcast and assign test");
        match result {
            Value::I64(x) => assert_eq!(x, 1, "Expected 1, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Regular Compound Assignment ====================
    // These tests use the Core IR pipeline (tree-sitter → lowering → compile_core)

    fn test_minus_assign_new() {
        // Test -= compound assignment
        let src = r#"
    a = 10
    a -= 3
    a
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run minus assign test");
        match result {
            Value::F64(x) => assert!((x - 7.0).abs() < 1e-10, "Expected 7.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 7, "Expected 7, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_mul_assign_new() {
        // Test *= compound assignment (using core pipeline)
        let src = r#"
    a = 5
    a *= 4
    a
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run mul assign test");
        match result {
            Value::F64(x) => assert!((x - 20.0).abs() < 1e-10, "Expected 20.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 20, "Expected 20, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_div_assign_new() {
        // Test /= compound assignment
        let src = r#"
    a = 20.0
    a /= 4.0
    a
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run div assign test");
        match result {
            Value::F64(x) => assert!((x - 5.0).abs() < 1e-10, "Expected 5.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 5, "Expected 5, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    fn test_pow_assign_new() {
        // Test ^= compound assignment
        let src = r#"
    a = 2.0
    a ^= 3.0
    a
    "#;
        let result = run_core_pipeline(src, 0).expect("Failed to run pow assign test");
        match result {
            Value::F64(x) => assert!((x - 8.0).abs() < 1e-10, "Expected 8.0, got {}", x),
            Value::I64(x) => assert_eq!(x, 8, "Expected 8, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // ==================== Struct-in-nested-scope Rejection (Issue #10402) ====================

    fn test_struct_nested_in_let_inside_macro_call_function_rejected_10402() {
        // Issue #10402: a `struct` nested inside a `let`/`begin` block that is
        // itself nested inside a FUNCTION body must be rejected, even when
        // that function is reached only through an enclosing top-level `let`
        // and the function's own body happens to contain a macro call
        // (`@show`) — the precondition that routes its body through the
        // ctx-aware lowering path that also handles `let`-nested structs
        // (Issue #10194's `lower_transparent_block_stmts`). Upstream Julia
        // rejects ANY `struct` not lexically at top level with `ERROR:
        // LoadError: syntax: "struct" expression not at top level`. Before
        // this fix sjulia silently accepted and ran the program (the struct
        // leaked into Program metadata via the #10194 macro-expanded-struct
        // queue with no nested-function guard in `lower_stmt_impl`).
        let src = r#"
    let
        function f()
            @show 1
            let
                struct BadNestedStruct10194
                    x::Int
                end
            end
            BadNestedStruct10194(1).x
        end
        f()
    end
    "#;
        let result = run_core_pipeline(src, 0);
        assert!(
            result.is_err(),
            "struct nested in let inside a nested function must be rejected (Issue #10402), got {:?}",
            result
        );
    }

    fn test_struct_sibling_of_macro_call_function_in_let_not_regressed_10402() {
        // Guard against a delta-unsafe reject (see the fix in
        // `lowering::reject_macro_expanded_structs_added_since`): a struct
        // that is a legitimate SIBLING of a function (not nested inside it)
        // within the same enclosing top-level `let` must still be defined,
        // even though the function's own body contains a macro call and so
        // is lowered through the same ctx-aware path used to detect Issue
        // #10402's illegal nesting above. A naive fix that rejects on the
        // *whole* macro-expanded-struct queue (rather than only what the
        // function itself added) would incorrectly reject this legal
        // program and silently drop `GoodSibling10402`.
        let src = r#"
    let
        struct GoodSibling10402
            x::Int
        end
        function f()
            @show 1
        end
        f()
        GoodSibling10402(3).x
    end
    "#;
        let result = run_core_pipeline(src, 0).expect("legal sibling struct must lower and run");
        match result {
            Value::I64(x) => assert_eq!(x, 3, "Expected 3, got {}", x),
            _ => panic!("Unexpected result type: {:?}", result),
        }
    }

    // Generated aggregate chunks for nextest process amortization.
    #[test]
    fn chunk_000() {
        test_struct_basic_immutable();
        test_struct_field_access();
        test_mutable_struct_field_assignment();
        test_struct_in_expression();
        test_struct_euclidean_distance();
        test_parametric_struct_explicit_type();
        test_parametric_struct_type_inference();
        test_parametric_struct_int_type();
        test_parametric_struct_multiple_params();
        test_parametric_struct_with_bound();
        test_parametric_nested_array_field();
        test_parametric_nested_struct_type();
        test_parametric_double_nested();
        test_while_break();
        test_for_break();
        test_while_continue();
    }

    #[test]
    fn chunk_001() {
        test_for_continue();
        test_nested_loops_break();
        test_try_catch_finally_with_message();
        test_try_else_finally_no_error();
        test_try_catch_finally_no_error_no_else();
        test_slice_range_1d();
        test_slice_full_matrix();
        test_transpose_1d_array();
        test_transpose_2d_basic();
        test_im_literal();
        test_im_in_expression();
        test_range_with_length();
        test_range_length_first_element();
        test_transpose_with_broadcast();
        test_transpose_function();
        test_kwarg_function_definition_simple();
    }

    #[test]
    fn chunk_002() {
        test_kwarg_function_with_explicit_kwarg();
        test_kwarg_function_multiple_kwargs();
        test_kwarg_function_multiple_kwargs_partial_override();
        test_kwarg_function_float_default();
        test_kwarg_function_float_default_override();
        test_kwarg_range_length();
        test_kwarg_range_length_int();
        test_short_function_single_arg();
        test_short_function_two_args();
        test_short_function_expression_body();
        test_short_function_with_regular_function();
        test_short_function_multiple_definitions();
        test_short_function_no_args();
        test_lambda_assignment_basic();
        test_lambda_assignment_multi_param();
        test_lambda_assignment_simple();
    }

    #[test]
    fn chunk_003() {
        test_map_with_lambda();
        test_map_with_named_function();
        test_filter_with_lambda();
        test_filter_first_element();
        test_reduce_with_lambda();
        test_reduce_with_init();
        test_sum_with_lambda();
        test_sum_with_implicit_mult_lambda();
        test_sum_with_implicit_mult_inline_array();
        test_map_with_implicit_mult_lambda();
        test_sum_with_named_function();
        test_map_empty_array();
        test_filter_empty_result();
        test_sum_empty_array();
        test_do_syntax_map();
        test_do_syntax_filter();
    }

    #[test]
    fn chunk_004() {
        test_do_syntax_reduce();
        test_do_syntax_sum();
        test_do_syntax_multiline();
        test_randn_basic();
        test_randn_deterministic();
        test_randn_array_1d();
        test_randn_array_2d();
        test_randn_multiple_calls();
        test_ios_sample_map_function();
        test_ios_sample_filter_function();
        test_ios_sample_reduce_function();
        test_ios_sample_do_syntax_map();
        test_ios_sample_do_syntax_filter_reduce();
        test_ios_sample_chaining_higher_order();
        test_ios_sample_basic_struct();
        test_ios_sample_mutable_struct();
    }

    #[test]
    fn chunk_005() {
        test_ios_sample_struct_with_functions();
        test_ios_sample_euclidean_distance();
        test_ios_sample_particle_simulation();
        test_ios_sample_try_catch_basics();
        test_ios_sample_error_recovery();
        test_ios_sample_error_recovery_with_error();
        test_ios_sample_normal_distribution();
        test_ios_sample_normal_distribution_matrix();
        test_ios_sample_histogram_visualization();
        test_broadcast_add_assign();
        test_broadcast_mul_assign();
        test_broadcast_sub_assign();
        test_broadcast_and_assign();
        test_minus_assign_new();
        test_mul_assign_new();
        test_div_assign_new();
    }

    #[test]
    fn chunk_006() {
        test_pow_assign_new();
        test_struct_nested_in_let_inside_macro_call_function_rejected_10402();
        test_struct_sibling_of_macro_call_function_in_let_not_regressed_10402();
    }
}

/// Issue #11146 (#10813 Phase 2a): the exception-type taxonomy funnel.
///
/// `vm_error_to_exception_value` builds the exception a `catch` binds by looking
/// the funnel's class name up in the VM's `struct_defs`:
///
/// ```ignore
/// let name = err.exception_class().julia_name()?;                  // the funnel
/// let type_id = self.struct_defs.iter().position(|d| d.name == name)?;
/// ```
///
/// Both `?`s fall back to `Value::str_new(err.to_string())` — a raw `String`. So
/// a class whose `julia_name()` does not name a struct that actually exists in
/// Base degrades SILENTLY: `typeof(e)` becomes `String`, not even an `Exception`
/// subtype. That is precisely the defect Issue #11146 exists to remove, and a
/// typo or a renamed Base struct would reintroduce it without any compile error.
///
/// This test compiles the real Base and requires every Julia-exception class in
/// the funnel to resolve.
mod integration_exception_taxonomy_funnel_tests {
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, ExceptionClass, VmError};

    fn compile_base_only() -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program: Program = prelude_lowering.lower(prelude_parsed).expect("lower base");
        compile_core_program(&prelude_program).expect("compile base")
    }

    #[test]
    fn exception_class_julia_names_resolve_in_base_11146() {
        let compiled = compile_base_only();
        let mut missing = Vec::new();
        for class in ExceptionClass::JULIA_CLASSES {
            let name = class
                .julia_name()
                .expect("JULIA_CLASSES must not contain VmInternal");
            let found = compiled.struct_defs.iter().any(|d| d.name == name);
            if !found {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "these exception classes in the Issue #11146 funnel do NOT resolve to a struct in \
             Base, so a `catch` would silently bind a raw String instead of the exception \
             object: {missing:?}. Define the struct in subset_julia_vm/src/julia/base/error.jl \
             or correct ExceptionClass::julia_name()."
        );
    }

    #[test]
    fn parser_detail_structs_resolve_in_base_11572() {
        let compiled = compile_base_only();
        for name in [
            "JuliaSyntax.SourceFile",
            "JuliaSyntax.Diagnostic",
            "JuliaSyntax.ParseError",
        ] {
            assert!(
                compiled
                    .struct_defs
                    .iter()
                    .any(|definition| definition.name == name),
                "parser detail type {name} must retain its JuliaSyntax-qualified name in Base"
            );
        }
    }

    /// The funnel's promise, end to end: every catchable `VmError` maps to a
    /// class, and every class names a real Base struct — so there is no
    /// catchable error left that can bind a non-`Exception` value.
    #[test]
    fn every_catchable_vm_error_has_a_base_exception_struct_11146() {
        let compiled = compile_base_only();
        let samples = [
            VmError::MethodError("m".to_string()),
            VmError::ArgumentError("a".to_string()),
            VmError::TypeError("t".to_string()),
            VmError::UndefVarError("v".to_string()),
            VmError::IndexOutOfBounds {
                indices: vec![9],
                shape: vec![3],
            },
            VmError::DimensionMismatchMsg("d".to_string()),
            VmError::ParseError("p".to_string()),
            // Issue #11146 moved NotImplemented out of the uncatchable set: an
            // unimplemented feature is user-reachable, so it must surface as a
            // real (ErrorException) exception rather than a bare String.
            VmError::NotImplemented("gap".to_string()),
        ];
        for err in samples {
            assert!(
                err.is_catchable(),
                "{err:?} must be catchable through the funnel"
            );
            let name = err
                .exception_class()
                .julia_name()
                .expect("a catchable error must have a Julia exception class");
            assert!(
                compiled.struct_defs.iter().any(|d| d.name == name),
                "{err:?} maps to class {name}, which has no struct in Base -- a catch would \
                 bind a raw String (Issue #11146)"
            );
        }
    }
}
