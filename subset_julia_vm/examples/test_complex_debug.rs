use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test 1: Simple complex creation and abs2
    let test1 = r#"
z = 0.0 + 0.0im
abs2(z)
"#;
    println!("Test 1: Simple abs2 on complex literal");
    let result1 = compile_and_run_str(test1, 0);
    println!("  Result: {} (NaN means error)", result1);

    // Test 2: abs2 comparison
    let test2 = r#"
z = 1.0 + 2.0im
abs2(z) > 4.0
"#;
    println!("\nTest 2: abs2 comparison");
    let result2 = compile_and_run_str(test2, 0);
    println!("  Result: {} (NaN means error)", result2);

    // Test 3: Function with complex parameter
    let test3 = r#"
function test_abs2(z::Complex)
    return abs2(z)
end
test_abs2(1.0 + 2.0im)
"#;
    println!("\nTest 3: Function with ::Complex parameter");
    let result3 = compile_and_run_str(test3, 0);
    println!("  Result: {} (NaN means error)", result3);

    // Test 4: Function with abs2 in if condition
    let test4 = r#"
function test_if(z::Complex)
    if abs2(z) > 4.0
        return 1
    end
    return 0
end
test_if(3.0 + 0.0im)
"#;
    println!("\nTest 4: abs2 in if condition");
    let result4 = compile_and_run_str(test4, 0);
    println!("  Result: {} (NaN means error)", result4);

    // Test 5: The actual mandelbrot pattern
    let test5 = r#"
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
mandelbrot_escape(2.0 + 0.0im, 50)
"#;
    println!("\nTest 5: Full mandelbrot function");
    let result5 = compile_and_run_str(test5, 0);
    println!("  Result: {} (NaN means error)", result5);

    // Test 6: Broadcast
    let test6 = r#"
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
C = [2.0 + 0.0im, -1.0 + 0.0im, 0.0 + 0.0im]
result = mandelbrot_escape.(C, Ref(50))
sum(result)
"#;
    println!("\nTest 6: Broadcast with Ref");
    let result6 = compile_and_run_str(test6, 0);
    println!("  Result: {} (expected 103, NaN means error)", result6);
}
