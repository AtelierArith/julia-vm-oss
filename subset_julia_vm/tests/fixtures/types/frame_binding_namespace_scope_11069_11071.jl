using Test

callee_defined_11069() = @isdefined QQQ11069
caller_defined_11069(x::QQQ11069) where QQQ11069 = callee_defined_11069()

callee_read_11069() = QQQ11069
caller_read_11069(x::QQQ11069) where QQQ11069 = callee_read_11069()

function caller_read_throws_undef_11069()
    try
        caller_read_11069(1)
        false
    catch err
        err isa UndefVarError
    end
end

function outer_shadow_11070(x::T) where T
    function inner_shadow_11070(y::T) where T
        (T, (() -> T)())
    end
    inner_shadow_11070(1.0)
end

global_visible_to_eval_11071 = 7

function eval_local_throws_undef_11071()
    local_only_11071 = 1
    try
        eval(:local_only_11071)
        false
    catch err
        err isa UndefVarError
    end
end

function make_eval_capture_probe_11071()
    captured_only_11071 = 2
    () -> begin
        captured_only_11071
        try
            eval(:captured_only_11071)
            false
        catch err
            err isa UndefVarError
        end
    end
end

function eval_where_throws_undef_11071(x::T) where T
    T
    try
        eval(:T)
        false
    catch err
        err isa UndefVarError
    end
end

eval_global_from_function_11071() = eval(:global_visible_to_eval_11071)

eval(:(eval_defined_local_11071() = begin
    local_state_11071 = 5
    local_state_11071 + 1
end))

eval(:(eval_defined_let_arg_11071(x) = let y_11071 = 1
    x + y_11071
end))

eval(:(eval_defined_catch_arg_11071(x) = try
    error("boom")
catch
    x
end))

function eval_nested_scope_updates_nearest_11071()
    eval(:(let x_11071 = 1
        try
            error("boom")
        catch
            x_11071 = 2
        end
        let y_11071 = 0
            x_11071 = 3
        end
        x_11071
    end))
end

function eval_catch_reads_outer_11071()
    eval(:(let x_11071 = 4
        try
            error("boom")
        catch
            x_11071
        end
    end))
end

@testset "frame binding namespaces remain lexical (Issues #11069/#11070/#11071)" begin
    @test caller_defined_11069(1) === false
    @test caller_read_throws_undef_11069()

    @test outer_shadow_11070(1) === (Float64, Float64)

    @test eval_local_throws_undef_11071()
    @test make_eval_capture_probe_11071()()
    @test eval_where_throws_undef_11071(1)
    @test eval_global_from_function_11071() == 7
    @test Base.invokelatest(eval_defined_local_11071) == 6
    @test Base.invokelatest(eval_defined_let_arg_11071, 2) == 3
    @test Base.invokelatest(eval_defined_catch_arg_11071, 3) == 3
    @test eval_nested_scope_updates_nearest_11071() == 3
    @test eval_catch_reads_outer_11071() == 4
end

true
