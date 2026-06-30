use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test 1: Direct z^2 in top-level
    let test1 = r#"
z = 1.0 + 2.0im
w = z^2
real(w)
"#;
    println!("Test 1: z^2 at top level");
    let result = compile_and_run_str(test1, 0);
    println!("  Result: {} (expected -3.0, NaN means error)", result);

    // Test 2: z^2 in function with local z
    let test2 = r#"
function test2()
    z = 1.0 + 2.0im
    w = z^2
    return real(w)
end
test2()
"#;
    println!("\nTest 2: z^2 in function with local z");
    let result = compile_and_run_str(test2, 0);
    println!("  Result: {} (expected -3.0, NaN means error)", result);

    // Test 3: z^2 + literal complex
    let test3 = r#"
function test3()
    z = 1.0 + 0.0im
    w = z^2 + (1.0 + 0.0im)
    return real(w)
end
test3()
"#;
    println!("\nTest 3: z^2 + literal complex");
    let result = compile_and_run_str(test3, 0);
    println!("  Result: {} (expected 2.0, NaN means error)", result);

    // Test 4: What is the type of z^2?
    let test4 = r#"
function test4()
    z = 1.0 + 0.0im
    w = z^2
    return abs2(w)
end
test4()
"#;
    println!("\nTest 4: abs2(z^2)");
    let result = compile_and_run_str(test4, 0);
    println!("  Result: {} (expected 1.0, NaN means error)", result);

    // Test 5: z^2 where z has parameter type
    let test5 = r#"
function test5(c::Complex)
    w = c^2
    return real(w)
end
test5(1.0 + 2.0im)
"#;
    println!("\nTest 5: c^2 where c::Complex is param");
    let result = compile_and_run_str(test5, 0);
    println!("  Result: {} (expected -3.0, NaN means error)", result);

    // Test 6: z^2 + c where c is param, z is local
    let test6 = r#"
function test6(c::Complex)
    z = 1.0 + 0.0im
    w = z^2 + c
    return real(w)
end
test6(1.0 + 0.0im)
"#;
    println!("\nTest 6: z^2 + c where c::Complex is param");
    let result = compile_and_run_str(test6, 0);
    println!("  Result: {} (expected 2.0, NaN means error)", result);
}
