using Test

function closures_hof_map_capture_4289(a)
    f(x) = x + a
    map(f, [1, 2, 3])
end

function closures_anonymous_hof_map_capture_4289(a)
    map(x -> x + a, [1, 2, 3])
end

function closures_assigned_anonymous_hof_map_capture_4289(a)
    f = x -> x + a
    map(f, [1, 2, 3])
end

function closures_anonymous_generator_capture_4289(a)
    collect(Base.Generator(x -> x + a, 1:3))
end

function closures_generator_capture_4289(a)
    f(x) = x + a
    collect(Base.Generator(f, 1:3))
end

function closures_typed_local_capture_call_4289(a)
    f(x::Int) = x + a
    f(2)
end

function closures_generator_vararg_capture_4289(a)
    f(x, y) = x + y + a
    collect(Base.Generator(f, [1, 2], [10, 20]))
end

function closures_filtered_generator_capture_4289(a)
    f(x) = x + a
    p(x) = x > 1
    collect(f(x) for x in [1, 2, 3] if p(x))
end

function closures_filtered_generator_range_capture_4289(a)
    f(x) = x + a
    p(x) = x > 1
    collect(f(x) for x in 1:3 if p(x))
end

function closures_filtered_comprehension_capture_4289(a)
    f(x) = x + a
    p(x) = x > 1
    [f(x) for x in [1, 2, 3] if p(x)]
end

function closures_filtered_generator_empty_capture_4289(a)
    f(x) = x + a
    p(x) = false
    collect(f(x) for x in [1, 2, 3] if p(x))
end

function closures_generator_expr_capture_4289(a)
    f(x) = x + a
    collect(f(x) for x in 1:3)
end

function closures_make_adder_returned_4289(a)
    x -> x + a
end

function closures_returned_arrow_direct_call_4289(a, x)
    closures_make_adder_returned_4289(a)(x)
end

function closures_returned_arrow_local_call_4289(a, x)
    f = closures_make_adder_returned_4289(a)
    f(x)
end

function closures_returned_arrow_renamed_capture_call_4289(offset, x)
    f = closures_make_adder_returned_4289(offset)
    f(x)
end

function closures_make_multiplier_returned_4289(a)
    mul(x) = x * a
    mul
end

function closures_returned_named_local_call_4289(a, x)
    f = closures_make_multiplier_returned_4289(a)
    f(x)
end

function closures_returned_named_renamed_capture_call_4289(factor, x)
    f = closures_make_multiplier_returned_4289(factor)
    f(x)
end

closures_compose_inc_4289(x) = x + 1
closures_compose_double_4289(x) = x * 2

function closures_composed_call_4289(a)
    h = closures_compose_inc_4289 ∘ closures_compose_double_4289
    h(a)
end

function closures_make_composed_adder_returned_4289(a)
    add = x -> x + a
    add ∘ closures_compose_double_4289
end

function closures_returned_composed_closure_call_4289(offset, x)
    h = closures_make_composed_adder_returned_4289(offset)
    h(x)
end

function closures_reflect_closure_return_type_4289(a)
    f = x -> x + a
    Base.return_types(f, Tuple{Int64})[1]
end

function closures_reflect_closure_code_typed_4289(a)
    f = x -> x + a
    Base.code_typed(f, Tuple{Int64})[1][2]
end

@testset "closure capture through HOF and Generator (Issue #4289)" begin
    mapped = closures_hof_map_capture_4289(10)
    anonymous_mapped = closures_anonymous_hof_map_capture_4289(10)
    assigned_anonymous_mapped = closures_assigned_anonymous_hof_map_capture_4289(10)
    anonymous_generated = closures_anonymous_generator_capture_4289(10)
    generated = closures_generator_capture_4289(10)
    typed_local = closures_typed_local_capture_call_4289(10)
    generated_vararg = closures_generator_vararg_capture_4289(10)
    filtered_generated = closures_filtered_generator_capture_4289(10)
    filtered_range_generated = closures_filtered_generator_range_capture_4289(10)
    filtered_comprehension = closures_filtered_comprehension_capture_4289(10)
    filtered_empty = closures_filtered_generator_empty_capture_4289(10)
    generator_expr = closures_generator_expr_capture_4289(10)
    returned_arrow_direct = closures_returned_arrow_direct_call_4289(10, 3)
    returned_arrow_local = closures_returned_arrow_local_call_4289(10, 3)
    returned_arrow_renamed = closures_returned_arrow_renamed_capture_call_4289(10, 3)
    returned_named_local = closures_returned_named_local_call_4289(10, 3)
    returned_named_renamed = closures_returned_named_renamed_capture_call_4289(10, 3)
    composed = closures_composed_call_4289(4)
    returned_composed = closures_returned_composed_closure_call_4289(3, 5)

    @test mapped == [11, 12, 13]
    @test typeof(mapped) == Vector{Int64}
    @test anonymous_mapped == [11, 12, 13]
    @test typeof(anonymous_mapped) == Vector{Int64}
    @test assigned_anonymous_mapped == [11, 12, 13]
    @test typeof(assigned_anonymous_mapped) == Vector{Int64}
    @test anonymous_generated == [11, 12, 13]
    @test typeof(anonymous_generated) == Vector{Int64}
    @test generated == [11, 12, 13]
    @test typeof(generated) == Vector{Int64}
    @test typed_local == 12
    @test typeof(typed_local) == Int64
    @test generated_vararg == [21, 32]
    @test typeof(generated_vararg) == Vector{Int64}
    @test filtered_generated == [12, 13]
    @test typeof(filtered_generated) == Vector{Int64}
    @test filtered_range_generated == [12, 13]
    @test typeof(filtered_range_generated) == Vector{Int64}
    @test filtered_comprehension == [12, 13]
    @test typeof(filtered_comprehension) == Vector{Int64}
    @test filtered_empty == []
    @test typeof(filtered_empty) == Vector{Union{}}
    @test generator_expr == [11, 12, 13]
    @test typeof(generator_expr) == Vector{Int64}
    @test Base.return_types(closures_generator_expr_capture_4289, Tuple{Int64})[1] == Vector{Int64}
    @test Base.infer_return_type(closures_generator_expr_capture_4289, Tuple{Int64}) == Vector{Int64}
    @test Base.return_types(closures_typed_local_capture_call_4289, Tuple{Int64})[1] == Int64
    @test Base.infer_return_type(closures_typed_local_capture_call_4289, Tuple{Int64}) == Int64
    @test returned_arrow_direct == 13
    @test typeof(returned_arrow_direct) == Int64
    @test returned_arrow_local == 13
    @test typeof(returned_arrow_local) == Int64
    @test returned_arrow_renamed == 13
    @test typeof(returned_arrow_renamed) == Int64
    @test returned_named_local == 30
    @test typeof(returned_named_local) == Int64
    @test returned_named_renamed == 30
    @test typeof(returned_named_renamed) == Int64
    @test composed == 9
    @test typeof(composed) == Int64
    @test returned_composed == 13
    @test typeof(returned_composed) == Int64
    @test Base.return_types(closures_returned_arrow_direct_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_arrow_direct_call_4289, Tuple{Int64, Int64}) == Int64
    @test Base.return_types(closures_returned_arrow_local_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_arrow_local_call_4289, Tuple{Int64, Int64}) == Int64
    @test Base.return_types(closures_returned_arrow_renamed_capture_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_arrow_renamed_capture_call_4289, Tuple{Int64, Int64}) == Int64
    @test Base.return_types(closures_returned_named_local_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_named_local_call_4289, Tuple{Int64, Int64}) == Int64
    @test Base.return_types(closures_returned_named_renamed_capture_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_named_renamed_capture_call_4289, Tuple{Int64, Int64}) == Int64
    @test Base.return_types(closures_composed_call_4289, Tuple{Int64})[1] == Int64
    @test Base.infer_return_type(closures_composed_call_4289, Tuple{Int64}) == Int64
    @test Base.return_types(closures_returned_composed_closure_call_4289, Tuple{Int64, Int64})[1] == Int64
    @test Base.infer_return_type(closures_returned_composed_closure_call_4289, Tuple{Int64, Int64}) == Int64
    @test closures_reflect_closure_return_type_4289(10) == Int64
    @test closures_reflect_closure_code_typed_4289(10) == Int64
end

true
