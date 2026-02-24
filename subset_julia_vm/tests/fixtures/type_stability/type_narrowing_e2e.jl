# End-to-end type narrowing test (Issue #1750)
#
# Verifies the full pipeline from abstract interpretation through to runtime:
#   type narrowing analysis → compiled code → correct runtime behavior
#
# Covers compound boolean patterns (&&, ||, !), nested narrowing,
# mixed field/variable narrowing, and early return narrowing.

using Test

# --- Struct definitions (OUTSIDE @testset per project guidelines) ---

struct MaybeInt
    val::Union{Int64, Nothing}
end

struct Pair
    first::Union{Int64, Nothing}
    second::Union{Int64, Nothing}
end

# --- Helper functions (OUTSIDE @testset) ---

# Compound &&: both arguments must be Int64
function add_if_both_int(x, y)
    if isa(x, Int64) && isa(y, Int64)
        return x + y
    end
    return 0
end

# Compound ||: accept Int64 or Float64
function increment_numeric(x)
    if isa(x, Int64) || isa(x, Float64)
        return x + 1
    end
    return 0
end

# Negation: !isa
function process_if_not_nothing(x)
    if !isa(x, Nothing)
        return x + 1
    end
    return 0
end

# Compound && with field access narrowing
function sum_if_both_present(p::Pair)
    if p.first !== nothing && p.second !== nothing
        return p.first + p.second
    end
    return -1
end

# Mixed field + variable narrowing with &&
function multiply_if_valid(c::MaybeInt, x)
    if c.val !== nothing && isa(x, Int64)
        return c.val * x
    end
    return -1
end

# Nested conditional narrowing (narrowing in multiple levels)
function nested_field_narrowing(a::MaybeInt, b::MaybeInt)
    if a.val !== nothing
        if b.val !== nothing
            return a.val + b.val
        else
            return a.val
        end
    end
    return 0
end

# Early return pattern: narrowing takes effect after guard clause
function early_return_guard(c::MaybeInt)
    if c.val === nothing
        return 0
    end
    # After the early return, c.val should be narrowed to Int64
    return c.val * 2
end

# Else-branch narrowing: === nothing narrows in then, excludes in else
function else_branch_narrowing(c::MaybeInt)
    if c.val === nothing
        return "absent"
    else
        return c.val + 100
    end
end

@testset "Type Narrowing End-to-End (Issue #1750)" begin
    @testset "compound && with isa" begin
        @test add_if_both_int(1, 2) == 3
        @test add_if_both_int(10, 20) == 30
        @test add_if_both_int(1, "hello") == 0
        @test add_if_both_int("a", "b") == 0
    end

    @testset "compound || with isa" begin
        @test increment_numeric(5) == 6
        @test increment_numeric(2.5) == 3.5
        @test increment_numeric("hi") == 0
    end

    @testset "negation !isa" begin
        @test process_if_not_nothing(5) == 6
        @test process_if_not_nothing(nothing) == 0
    end

    @testset "compound && with field access" begin
        @test sum_if_both_present(Pair(10, 20)) == 30
        @test sum_if_both_present(Pair(10, nothing)) == -1
        @test sum_if_both_present(Pair(nothing, 20)) == -1
        @test sum_if_both_present(Pair(nothing, nothing)) == -1
    end

    @testset "mixed field + variable narrowing" begin
        @test multiply_if_valid(MaybeInt(5), 3) == 15
        @test multiply_if_valid(MaybeInt(nothing), 3) == -1
        @test multiply_if_valid(MaybeInt(5), "hello") == -1
    end

    @testset "nested conditional narrowing" begin
        @test nested_field_narrowing(MaybeInt(10), MaybeInt(20)) == 30
        @test nested_field_narrowing(MaybeInt(10), MaybeInt(nothing)) == 10
        @test nested_field_narrowing(MaybeInt(nothing), MaybeInt(20)) == 0
    end

    @testset "early return guard pattern" begin
        @test early_return_guard(MaybeInt(7)) == 14
        @test early_return_guard(MaybeInt(nothing)) == 0
    end

    @testset "else-branch narrowing" begin
        @test else_branch_narrowing(MaybeInt(nothing)) == "absent"
        @test else_branch_narrowing(MaybeInt(42)) == 142
    end
end

true
