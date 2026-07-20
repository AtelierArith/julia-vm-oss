#[test]
fn function_visibility_distinguishes_unactivated_from_future_world_8452_11320() {
    let mut vm = Vm::new(Vec::new(), StableRng::new(0));

    let mut unactivated = dispatch_test_function("unactivated_11320", vec![], vec![]);
    unactivated.min_world = u64::MAX;
    vm.functions.push(Rc::new(unactivated));
    vm.function_name_index
        .insert("unactivated_11320".to_string(), vec![0]);
    assert!(vm.function_name_exists_only_as_unactivated("unactivated_11320"));

    let mut future_world = dispatch_test_function("future_world_8452", vec![], vec![]);
    future_world.min_world = vm.current_world + 1;
    vm.functions.push(Rc::new(future_world));
    vm.function_name_index
        .insert("future_world_8452".to_string(), vec![1]);
    assert!(
        !vm.function_name_exists_only_as_unactivated("future_world_8452"),
        "a finite future-world method means the generic binding exists; an old-world call must fall through to MethodError"
    );
}
