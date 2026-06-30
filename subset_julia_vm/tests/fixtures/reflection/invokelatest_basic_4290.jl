using Test

invokelatest_inc_4290(x) = x + 1
invokelatest_count_4290(args...) = length(args)
invokelatest_kw_4290(; x=1) = x + 1
invokelatest_pos_kw_4290(x; y=1) = x + y
invokelatest_macro_kw_4290(; x=1) = x + 2
invokelatest_macro_pos_kw_4290(x; y=1) = x + y
invokelatest_macro_property_get_4290(x) = @invokelatest x.f
function invokelatest_macro_property_set_4290(x, v)
    @invokelatest x.f = v
    x.f
end
invokelatest_macro_index_get_4290(xs) = @invokelatest xs[2]
function invokelatest_macro_index_set_4290(xs, v)
    @invokelatest xs[2] = v
    xs[2]
end
invoke_world_count_4290(args...) = length(args)
invoke_world_typed_4290(x::Int64) = x + 1
world_macro_length_4290(xs) = (@world length Base.get_world_counter())(xs)
world_macro_base_length_4290(xs) = (@world Base.length Base.get_world_counter())(xs)

function invoke_in_old_world_rejected_4290()
    try
        Base.invoke_in_world(UInt64(0), invokelatest_inc_4290, 2)
    catch err
        return true
    end
    return false
end

struct InvokeProperty4290
    f::Int64
end

mutable struct InvokeMutableProperty4290
    f::Int64
end

invoke_pick_4290(x::Integer) = x + 1
invoke_pick_4290(x::Number) = x + 10
invoke_kw_pick_4290(x::Integer; y=1) = x + y + 1
invoke_kw_pick_4290(x::Number; y=1) = x + y + 10
invoke_pair_4290(x::Integer, y::Number) = x + y + 1
invoke_pair_4290(x::Number, y::Number) = x + y + 10
invoke_vararg_4290(xs::Vararg{Int64}) = length(xs)
invoke_alias_4290 = invoke_pick_4290
invoke_sig_4290 = Tuple{Number}
invoke_kw_splat_4290 = (y=5,)
invoke_function_value_4290(f, x) = invoke(f, Tuple{Number}, x)
invoke_function_value_kw_4290(f, x; y=1) = invoke(f, Tuple{Number}, x; y=y)
invoke_function_value_kw_splat_4290(f, x, kw) = invoke(f, Tuple{Number}, x; kw...)
invoke_runtime_signature_4290(sig, x) = invoke(invoke_pick_4290, sig, x)
invoke_function_value_runtime_signature_4290(f, sig, x) = invoke(f, sig, x)
invoke_runtime_signature_kw_4290(sig, x; y=1) = invoke(invoke_kw_pick_4290, sig, x; y=y)
invoke_function_value_runtime_signature_kw_4290(f, sig, x; y=1) = invoke(f, sig, x; y=y)
invoke_runtime_signature_kw_splat_4290(sig, x, kw) = invoke(invoke_kw_pick_4290, sig, x; kw...)
invoke_function_value_runtime_signature_kw_splat_4290(f, sig, x, kw) = invoke(f, sig, x; kw...)
invoke_macro_untyped_4290(x) = @invoke invoke_pick_4290(x)
invoke_macro_mixed_untyped_4290(y) = @invoke invoke_pair_4290(2::Number, y)
invoke_macro_kw_4290(x; y=1) = @invoke invoke_kw_pick_4290(x::Number; y=y)
invoke_macro_kw_splat_4290(x, kw) = @invoke invoke_kw_pick_4290(x::Number; kw...)
invoke_macro_property_get_4290(x) = @invoke (x::InvokeProperty4290).f
function invoke_macro_property_set_4290(x, v)
    @invoke (x::InvokeMutableProperty4290).f = v::Int64
    x.f
end
invoke_macro_index_get_4290(xs) = @invoke (xs::Vector{Int64})[2::Int64]
function invoke_macro_index_set_4290(xs, v)
    @invoke (xs::Vector{Int64})[2::Int64] = v::Int64
    xs[2]
end
invoke_macro_operator_4290() = @invoke 420::Integer % Unsigned

@testset "Base.invokelatest compatibility (Issue #4290)" begin
    @test Core.invokelatest(+, 1, 2) == 3
    @test Base.invokelatest(invokelatest_inc_4290, 2) == 3
    @test Base.invokelatest(invokelatest_count_4290, 1, 2, 3) == 3
    @test Base.invokelatest(invokelatest_kw_4290; x=4) == 5
    @test Base.invokelatest(invokelatest_pos_kw_4290, 3; y=5) == 8
    @test (@invokelatest invokelatest_inc_4290(2)) == 3
    @test (@invokelatest invokelatest_macro_kw_4290(x=4)) == 6
    @test (@invokelatest invokelatest_macro_pos_kw_4290(3; y=5)) == 8
    @test invokelatest_macro_property_get_4290(InvokeProperty4290(7)) == 7
    @test invokelatest_macro_property_set_4290(InvokeMutableProperty4290(1), 9) == 9
    @test invokelatest_macro_index_get_4290([10, 20]) == 20
    @test invokelatest_macro_index_set_4290([10, 20], 99) == 99
    world = Base.get_world_counter()
    @test typeof(world) == UInt64
    @test Base.tls_world_age() == world
    @test Base.invoke_in_world(world, invokelatest_inc_4290, 2) == 3
    @test invoke_in_old_world_rejected_4290()
    @test Base.invoke_in_world(world, invoke_world_count_4290, 1, 2, 3) == 3
    @test Base.invoke_in_world(world, invokelatest_pos_kw_4290, 3; y=5) == 8
    @test hasmethod(invoke_world_typed_4290, Tuple{Int64}; world=world)
    @test !hasmethod(invoke_world_typed_4290, Tuple{Int64}; world=typemax(UInt64))
    hasmethod_world_kw_rejected_4290 = false
    try
        hasmethod(invokelatest_kw_4290, Tuple{}, (:x,); world=typemax(UInt64))
    catch err
        hasmethod_world_kw_rejected_4290 = true
    end
    @test hasmethod_world_kw_rejected_4290
    @test Base.return_types(invoke_world_typed_4290, Tuple{Int64}; world=world)[1] === Int64
    @test Base.code_typed(invoke_world_typed_4290, Tuple{Int64}; world=world)[1][2] === Int64
    code_lowered_world_rejected_4290 = false
    try
        Base.code_lowered(invoke_world_typed_4290, Tuple{Int64}; world=world)
    catch err
        code_lowered_world_rejected_4290 = true
    end
    @test code_lowered_world_rejected_4290
    @test Core.Compiler.return_type(invoke_world_typed_4290, Tuple{Int64}, world) === Int64
    @test Core.Compiler.return_type(Tuple{typeof(invoke_world_typed_4290), Int64}, world) === Int64
    @test world_macro_length_4290([1, 2, 3]) == 3
    @test world_macro_base_length_4290([1, 2, 3]) == 3
end

@testset "invoke signature-selected dispatch (Issue #4290)" begin
    @test Core.invoke(+, Tuple{Int64, Int64}, 1, 2) == 3
    @test invoke(invoke_pick_4290, Tuple{Integer}, 2) == 3
    @test invoke(invoke_pick_4290, Tuple{Number}, 2) == 12
    @test Base.invoke(invoke_pair_4290, Tuple{Integer, Number}, 2, 3) == 6
    @test Base.invoke(invoke_pair_4290, Tuple{Number, Number}, 2, 3) == 15
    @test invoke(invoke_vararg_4290, Tuple{Vararg{Int64}}, 1, 2) == 2
    @test invoke(invoke_vararg_4290, Tuple{Vararg{Int64}}, 1, 2, 3) == 3
    @test (@invoke invoke_pick_4290(2::Number)) == 12
    @test (@invoke invoke_pair_4290(2::Number, 3::Number)) == 15
    @test invoke(invoke_alias_4290, Tuple{Number}, 2) == 12
    @test invoke(invoke_pick_4290, invoke_sig_4290, 2) == 12
    @test invoke(invoke_kw_pick_4290, Tuple{Number}, 2; y=5) == 17
    @test invoke(invoke_kw_pick_4290, Tuple{Number}, 2; invoke_kw_splat_4290...) == 17
    @test invoke_function_value_4290(invoke_pick_4290, 2) == 12
    @test invoke_function_value_kw_4290(invoke_kw_pick_4290, 2; y=5) == 17
    @test invoke_function_value_kw_splat_4290(invoke_kw_pick_4290, 2, invoke_kw_splat_4290) == 17
    @test invoke_runtime_signature_4290(Tuple{Number}, 2) == 12
    @test invoke_function_value_runtime_signature_4290(invoke_pick_4290, Tuple{Number}, 2) == 12
    @test invoke_runtime_signature_kw_4290(Tuple{Number}, 2; y=5) == 17
    @test invoke_function_value_runtime_signature_kw_4290(invoke_kw_pick_4290, Tuple{Number}, 2; y=5) == 17
    @test invoke_runtime_signature_kw_splat_4290(Tuple{Number}, 2, invoke_kw_splat_4290) == 17
    @test invoke_function_value_runtime_signature_kw_splat_4290(invoke_kw_pick_4290, Tuple{Number}, 2, invoke_kw_splat_4290) == 17
    @test invoke_macro_untyped_4290(2) == 3
    @test invoke_macro_mixed_untyped_4290(3) == 15
    @test invoke_macro_kw_4290(2; y=5) == 17
    @test invoke_macro_kw_splat_4290(2, invoke_kw_splat_4290) == 17
    @test invoke_macro_property_get_4290(InvokeProperty4290(7)) == 7
    @test invoke_macro_property_set_4290(InvokeMutableProperty4290(1), 9) == 9
    @test invoke_macro_index_get_4290([10, 20]) == 20
    @test invoke_macro_index_set_4290([10, 20], 99) == 99
    @test invoke_macro_operator_4290() == UInt64(420)
    @test typeof(invoke_macro_operator_4290()) == UInt64
end

true
