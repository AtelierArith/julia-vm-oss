# Statically-known dispatch misses inside try/catch are runtime MethodErrors.

using Test

dpkw9567_gap_9709(x::Int; scale = 1) = :ok
dparity9567_gap_9710(x::Int, y::Int) = :ok

side_effects_9709_9710 = 0

function bump_arg_9709()
    global side_effects_9709_9710 += 1
    return "s"
end

function bump_kw_9709()
    global side_effects_9709_9710 += 10
    return 2
end

function keyword_no_method_caught_9709()
    try
        dpkw9567_gap_9709(bump_arg_9709(); scale = bump_kw_9709())
        return false
    catch err
        return err isa MethodError
    end
end

function arity_no_method_caught_9710()
    try
        dparity9567_gap_9710(1)
        return false
    catch err
        return err isa MethodError
    end
end

@testset "try/catch dispatch MethodError parity (Issues #9709/#9710)" begin
    @test keyword_no_method_caught_9709()
    @test side_effects_9709_9710 == 11
    @test arity_no_method_caught_9710()
end

true
