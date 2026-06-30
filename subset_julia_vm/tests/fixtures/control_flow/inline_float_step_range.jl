# Test: inline StepRangeLen{Float64} literal in for-head (Issue #3551).
# Previously, `for x in 1.0:0.5:2.0; ...; end` silently iterated zero times
# because the integer fast path in `Stmt::For` codegen pinned every component
# to ValueType::I64, truncating the float bounds and producing an empty loop.
# The fix is in the lowering: when any of start/end/step is a non-integer
# literal, fall back to `Stmt::ForEach` over `Expr::Range`, which goes
# through the generic Pure Julia iterate(::StepRangeLen) path.

using Test

# Inline float-stepped range (regression case).
function inline_float_step()
    xs = Float64[]
    for x in 1.0:0.5:2.0
        push!(xs, x)
    end
    xs
end

# Inline float unit range (start:end with no step).
function inline_float_unit()
    xs = Float64[]
    for x in 1.0:3.0
        push!(xs, x)
    end
    xs
end

# Indirect via variable — should still work the same as before.
function indirect_float_step()
    xs = Float64[]
    r = 1.0:0.5:2.0
    for x in r
        push!(xs, x)
    end
    xs
end

# Negative float step.
function inline_float_step_neg()
    xs = Float64[]
    for x in 2.0:-0.5:0.5
        push!(xs, x)
    end
    xs
end

# Integer fast path must continue to work after the fix.
function inline_int_step()
    xs = Int[]
    for i in 1:2:7
        push!(xs, i)
    end
    xs
end

@testset "Inline float-step ranges in for-head (Issue #3551)" begin
    @test inline_float_step() == [1.0, 1.5, 2.0]
    @test inline_float_unit() == [1.0, 2.0, 3.0]
    @test indirect_float_step() == [1.0, 1.5, 2.0]
    @test inline_float_step_neg() == [2.0, 1.5, 1.0, 0.5]
    @test inline_int_step() == [1, 3, 5, 7]
end

true
