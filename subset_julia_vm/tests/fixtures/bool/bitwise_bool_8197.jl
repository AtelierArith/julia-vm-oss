# Test Bool-Bool bitwise operators &, |, ⊻ (and xor)
# Issue #8197: `&(::Bool,::Bool)`, `|(::Bool,::Bool)`, `⊻(::Bool,::Bool)` threw
#   MethodError; upstream Julia defines them in base/bool.jl as
#     (&)(x::Bool, y::Bool) = and_int(x, y)
#     (|)(x::Bool, y::Bool) = or_int(x, y)
#     xor(x::Bool, y::Bool) = (x != y)   # ⊻ is the infix synonym for xor

using Test

@testset "Bool bitwise operators (Issue #8197)" begin
    @testset "& (bitwise AND)" begin
        @test (true & false) === false
        @test (true & true) === true
        @test (false & false) === false
        @test (false & true) === false
        @test typeof(true & false) === Bool
    end

    @testset "| (bitwise OR)" begin
        @test (true | false) === true
        @test (true | true) === true
        @test (false | false) === false
        @test (false | true) === true
        @test typeof(true | false) === Bool
    end

    @testset "⊻ / xor (bitwise XOR)" begin
        @test (true ⊻ true) === false
        @test (true ⊻ false) === true
        @test (false ⊻ false) === false
        @test (false ⊻ true) === true
        @test xor(true, false) === true
        @test xor(true, true) === false
        @test typeof(true ⊻ true) === Bool
        @test typeof(xor(true, false)) === Bool
    end

    @testset "branch-free style composition" begin
        # Mirrors upstream base/float.jl branch-free comparisons (Issue #8187)
        x = true
        y = false
        @test (x & !y) | (y & !x) === true
        @test ((true & false) | (true ⊻ false)) === true
    end

    # Regression guard: adding the Bool methods must NOT break mixed-type or
    # same-type integer bitwise dispatch. The Bool methods are registered AFTER
    # the Int64 methods (base/int.jl) so the type-safe Int64 method stays the
    # runtime fallback for a mixed call with no exact same-type method; a Bool
    # method registered first made a Bool result slot receive a widened Int64
    # at runtime (InternalError: LoadSlotBool). Each value below matches upstream.
    @testset "mixed-type bitwise still dispatches (no LoadSlotBool)" begin
        and_op(a, b) = a & b
        or_op(a, b) = a | b
        xor_op(a, b) = a ⊻ b
        # mixed types promote to Int64 (upstream: 0x05 & 5 === 5, true & 5 === 1)
        @test and_op(0x05, 5) === 5
        @test and_op(true, 5) === 1
        @test or_op(true, 5) === 5
        @test and_op(UInt16(5), 5) === 5
        @test typeof(and_op(0x05, 5)) === Int64
        # same-type calls keep their type (exact methods win over the fallback)
        @test and_op(0x0f, 0x05) === 0x05
        @test typeof(and_op(0x0f, 0x05)) === UInt8
        @test or_op(0x05, 0x02) === 0x07
        @test xor_op(5, 3) === 6
        @test and_op(true, false) === false
        @test typeof(and_op(true, false)) === Bool
    end
end

true
