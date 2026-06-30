# Regression coverage for boxing large Instr variants (Issue #5176).
#
# Boxing PushI128 / PushModule / MakeGenerator / the dict-mutating typed-dispatch
# StoreDict variants / the kwargs-splat call variants is a representation-only
# change: behaviour must be byte-for-byte identical to upstream Julia. This
# fixture drives each affected instruction so any operand-extraction mistake in
# the boxed paths surfaces as a value mismatch.

using Test

@testset "boxed Instr operands behave identically" begin
    # PushI128: Int128 literal beyond the Int64 range.
    big = 9223372036854775808           # i64::MAX + 1
    @assert typeof(big) == Int128
    @assert big - 1 == 9223372036854775807
    @assert big + big == 18446744073709551616

    # MakeGenerator: lazy generator expression with a function body + filter.
    squares = (x * x for x in 1:5)
    @assert sum(squares) == 55
    evens = (x for x in 1:10 if x % 2 == 0)
    @assert collect(evens) == [2, 4, 6, 8, 10]

    # Dict-mutating typed-dispatch StoreDict variants: get!, pop!, delete!,
    # merge!, empty! all write the mutated Dict back to the bound local.
    d = Dict{String,Int}("a" => 1, "b" => 2)
    @assert get!(d, "c", 3) == 3
    @assert d["c"] == 3
    @assert pop!(d, "a") == 1
    @assert !haskey(d, "a")
    delete!(d, "b")
    @assert !haskey(d, "b")
    merge!(d, Dict("x" => 9))
    @assert d["x"] == 9
    empty!(d)
    @assert length(d) == 0

    # kwargs-splat through a function value: positional splat + kwargs splat.
    function describe(args...; kwargs...)
        length(args) + length(kwargs)
    end
    pos = (1, 2, 3)
    opts = (a = 1, b = 2)
    f = describe
    @assert f(pos...; opts...) == 5

    @test true
end

true
