# Extends the #10819 branch-widening matrix (Issue #10820 prevention) with
# two control-flow shapes not covered by nothing_initialized_branch_widen_10819.jl:
# a `try`/`catch` where only the try path assigns, and a loop that may execute
# zero times where only the loop body assigns. Both start from a local
# initialized to `nothing` (a storage-elision-eligible singleton) and widen it
# to a dynamic value on only one path; the non-assigning path must still
# observe `nothing` instead of raising UndefVarError.

using Test

function trycatch_widen_10820(should_throw, value)
    result = nothing
    try
        if should_throw
            error("boom")
        end
        result = value
    catch
        # non-assigning path: `result` must still read back as `nothing`
    end
    result
end

function trycatch_catch_widen_10820(should_throw, value)
    result = nothing
    try
        if should_throw
            error("boom")
        end
    catch
        result = value
    end
    result
end

function loop_zero_widen_10820(iterations, value)
    result = nothing
    for _ in 1:iterations
        result = value
    end
    result
end

function while_zero_widen_10820(take_branch, value)
    result = nothing
    i = 0
    while take_branch && i == 0
        result = value
        i += 1
    end
    result
end

@testset "Nothing local survives try/catch and zero-iteration loop widening (Issue #10820)" begin
    @test trycatch_widen_10820(false, 42) === 42
    @test trycatch_widen_10820(true, 42) === nothing
    @test trycatch_catch_widen_10820(false, 42) === nothing
    @test trycatch_catch_widen_10820(true, 42) === 42
    @test loop_zero_widen_10820(0, 99) === nothing
    @test loop_zero_widen_10820(3, 99) === 99
    @test while_zero_widen_10820(false, 7) === nothing
    @test while_zero_widen_10820(true, 7) === 7
end

true
