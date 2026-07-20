# Regression fixture for Issues #11198, #11215, #11487.
#
# The runtime specializer's `compile_call` (and `compile_convert_call`) had an
# unconditional name-keyed "Int"/"Int64" fast path that, once a capture-free
# lambda was runtime-specialized (reached through an untyped function
# parameter / HOF), emitted a raw primitive conversion instruction without:
#   (a) InexactError checking on a lossy Float64 source (#11198), or
#   (b) consulting the constructor's method table for a struct operand with a
#       user-defined `Int64(::T)` method (#11215).
# The `convert(Int64, x)` sibling shared the same unchecked truncation bug for
# a lossy Float64 source (#11487).
#
# Verified against upstream `julia --startup-file=no --history-file=no` 1.12.6.

using Test

apply(f, x) = f(x)
call_it(f) = f()

@testset "HOF-truncation shape raises InexactError (Issues #11198, #11487)" begin
    # Int(::Float64) / convert(Int64, ::Float64) inside a lambda invoked
    # through an untyped function parameter must raise InexactError for a
    # non-integral value, matching the direct-call path. Before the fix, the
    # specializer's unconditional fast path emitted a raw `ToI64` truncation
    # here instead, so these silently returned `1` with no error.
    @test_throws InexactError call_it(() -> Int(1.5))
    @test_throws InexactError call_it(() -> convert(Int64, 1.5))

    # Exact conversions through the same HOF path must still take the fast
    # path and succeed (no regression on the common case).
    @test call_it(() -> Int(3.0)) == 3
    @test call_it(() -> convert(Int64, 3.0)) == 3
end

# (b) custom-constructor-method shape: a capture-free lambda calling
# `Int64(x)` on a struct with a user-defined `Base.Int64(::T)` method must
# dispatch through the method table, not bypass it with a primitive
# conversion, when reached through an untyped function parameter.
struct IntCtorFastpathBox11215
    x::Int64
end

Base.Int64(b::IntCtorFastpathBox11215) = b.x + 100
Base.convert(::Type{Int64}, b::IntCtorFastpathBox11215) = b.x + 200

@testset "custom-constructor-method shape dispatches through the method table (Issue #11215)" begin
    b = IntCtorFastpathBox11215(1)

    @test Int64(b) == 101
    @test convert(Int64, b) == 201
    @test apply(y -> Int64(y), b) == 101
    @test apply(y -> convert(Int64, y), b) == 201
end

true
