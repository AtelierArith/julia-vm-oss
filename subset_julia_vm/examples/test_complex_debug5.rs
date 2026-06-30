use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test 1: Two local complex vars added
    let test1 = r#"
function test1()
    z = 1.0 + 0.0im
    w = 2.0 + 0.0im
    r = z + w
    return real(r)
end
test1()
"#;
    println!("Test 1: z + w (both local)");
    let result = compile_and_run_str(test1, 0);
    println!("  Result: {} (expected 3.0, NaN means error)", result);

    // Test 2: z^2 assigned to new var, then add
    let test2 = r#"
function test2()
    z = 1.0 + 0.0im
    w = 2.0 + 0.0im
    zz = z^2
    r = zz + w
    return real(r)
end
test2()
"#;
    println!("\nTest 2: zz = z^2; r = zz + w");
    let result = compile_and_run_str(test2, 0);
    println!("  Result: {} (expected 3.0, NaN means error)", result);

    // Test 3: Inline z^2 + w
    let test3 = r#"
function test3()
    z = 1.0 + 0.0im
    w = 2.0 + 0.0im
    r = z^2 + w
    return real(r)
end
test3()
"#;
    println!("\nTest 3: r = z^2 + w (inline)");
    let result = compile_and_run_str(test3, 0);
    println!("  Result: {} (expected 3.0, NaN means error)", result);

    // Test 4: Check what type z^2 is returning
    let test4 = r#"
function test4()
    z = 1.0 + 0.0im
    zz = z^2
    println("typeof zz: ", typeof(zz))
    return real(zz)
end
test4()
"#;
    println!("\nTest 4: Check type of z^2");
    let result = compile_and_run_str(test4, 0);
    println!("  Result: {} (expected 1.0, NaN means error)", result);
}
