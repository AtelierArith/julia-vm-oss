use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test 1: z + c where c is Complex parameter
    let test1 = r#"
function test1(c::Complex)
    z = 0.0 + 0.0im
    z = z + c
    return 1
end
test1(1.0 + 0.0im)
"#;
    println!("Test 1: z + c (c is ::Complex param)");
    let result = compile_and_run_str(test1, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test 2: Same without type annotation
    let test2 = r#"
function test2(c)
    z = 0.0 + 0.0im
    z = z + c
    return 1
end
test2(1.0 + 0.0im)
"#;
    println!("\nTest 2: z + c (c has no type annotation)");
    let result = compile_and_run_str(test2, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test 3: c + z (reversed order)
    let test3 = r#"
function test3(c::Complex)
    z = 0.0 + 0.0im
    z = c + z
    return 1
end
test3(1.0 + 0.0im)
"#;
    println!("\nTest 3: c + z (c is ::Complex param, reversed)");
    let result = compile_and_run_str(test3, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test 4: c used directly in expression
    let test4 = r#"
function test4(c::Complex)
    return real(c)
end
test4(1.0 + 2.0im)
"#;
    println!("\nTest 4: real(c) where c is ::Complex param");
    let result = compile_and_run_str(test4, 0);
    println!("  Result: {} (expected 1.0, NaN means error)", result);

    // Test 5: Just return c
    let test5 = r#"
function test5(c::Complex)
    return c
end
z = test5(1.0 + 2.0im)
real(z)
"#;
    println!("\nTest 5: Just return c (::Complex param)");
    let result = compile_and_run_str(test5, 0);
    println!("  Result: {} (expected 1.0, NaN means error)", result);
}
