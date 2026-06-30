use subset_julia_vm::compile_and_run_str;

fn main() {
    // Test the core issue: c has Any type (from array element)
    let test1 = r#"
function test1(c)
    z = 0.0 + 0.0im
    r = z^2 + c
    return 1
end
test1(1.0 + 0.0im)
"#;
    println!("Test 1: z^2 + c where c has no type annotation (Any)");
    let result = compile_and_run_str(test1, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test with explicit type
    let test2 = r#"
function test2(c::Complex)
    z = 0.0 + 0.0im
    r = z^2 + c
    return 1
end
test2(1.0 + 0.0im)
"#;
    println!("\nTest 2: z^2 + c where c::Complex");
    let result = compile_and_run_str(test2, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test simple c + z without power
    let test3 = r#"
function test3(c)
    z = 1.0 + 0.0im
    r = c + z
    return 1
end
test3(1.0 + 0.0im)
"#;
    println!("\nTest 3: c + z where c has Any type");
    let result = compile_and_run_str(test3, 0);
    println!("  Result: {} (NaN means error)", result);

    // Test z + c (Complex + Any)
    let test4 = r#"
function test4(c)
    z = 1.0 + 0.0im
    r = z + c
    return 1
end
test4(1.0 + 0.0im)
"#;
    println!("\nTest 4: z + c where z is Complex literal, c has Any type");
    let result = compile_and_run_str(test4, 0);
    println!("  Result: {} (NaN means error)", result);
}
