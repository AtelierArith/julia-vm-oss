use subset_julia_vm::compile_and_run_str;

#[test]
fn test_simple_if_else() {
    let src = r#"
x = -0.5
result = 0
if x < 0
    result = 1
else
    result = 2
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!("test_simple_if_else: Result for x=-0.5: {}", result);
    // x=-0.5 is < 0, so result = 1
    assert!((result - 1.0).abs() < 1e-10, "Expected 1, got {}", result);
}

#[test]
fn test_if_elseif_else() {
    let src = r#"
x = -0.5
result = 0
if x < -1
    result = 1
elseif x < 0
    result = 2
else
    result = 3
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!("test_if_elseif_else: Result for x=-0.5: {}", result);
    // x=-0.5 is < 0 but not < -1, so result = 2
    assert!((result - 2.0).abs() < 1e-10, "Expected 2, got {}", result);
}

#[test]
fn test_if_two_elseif_no_else() {
    let src = r#"
x = -0.5
result = 0
if x < -2
    result = 1
elseif x < -1
    result = 2
elseif x < 0
    result = 3
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!("test_if_two_elseif_no_else: Result for x=-0.5: {}", result);
    // x=-0.5: not < -2, not < -1, but < 0 => result = 3
    assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
}

#[test]
fn test_if_two_elseif_with_else() {
    let src = r#"
x = -0.5
result = 0
if x < -2
    result = 1
elseif x < -1
    result = 2
elseif x < 0
    result = 3
else
    result = 4
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!(
        "test_if_two_elseif_with_else: Result for x=-0.5: {}",
        result
    );
    // x=-0.5: not < -2, not < -1, but < 0 => result = 3
    assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
}

#[test]
fn test_if_three_elseif_with_else() {
    let src = r#"
x = 0.5
result = 0
if x < -1
    result = 1
elseif x < 0
    result = 2
elseif x < 1
    result = 3
elseif x < 2
    result = 4
else
    result = 5
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!(
        "test_if_three_elseif_with_else: Result for x=0.5: {}",
        result
    );
    // x=0.5: not < -1, not < 0, but < 1 => result = 3
    assert!((result - 3.0).abs() < 1e-10, "Expected 3, got {}", result);
}

#[test]
fn test_if_three_elseif_else_path() {
    let src = r#"
x = 10.0
result = 0
if x < -1
    result = 1
elseif x < 0
    result = 2
elseif x < 1
    result = 3
elseif x < 2
    result = 4
else
    result = 5
end
result
"#;
    let result = compile_and_run_str(src, 0);
    println!(
        "test_if_three_elseif_else_path: Result for x=10.0: {}",
        result
    );
    // x=10.0: not < -1, not < 0, not < 1, not < 2 => result = 5
    assert!((result - 5.0).abs() < 1e-10, "Expected 5, got {}", result);
}
