# Verify same-name HOFs dispatch to Pure Julia methods (Issue #3731)
#
# Before #3731:
#   - `compile/expr/builtin_hof.rs` routed `map!`, `filter!`, `mapreduce`,
#     `mapfoldl`, `mapfoldr` directly to dedicated VM HOF instructions
#     (`MapFuncInPlace`, `FilterFuncInPlace`, `MapReduceFunc`, `MapFoldrFunc`)
#     even when same-name Pure Julia methods existed.
#
# After #3731:
#   - The compile_builtin_hof handlers for those names return `Ok(None)` so
#     calls fall through to method dispatch and resolve to:
#       - `map!(f, a::Array)`              → base/array.jl
#       - `map!(f, dest::Array, src::Array)` → base/array.jl
#       - `filter!(f, a::Array)`           → base/array.jl
#       - `mapfoldl(f, op, itr [, init])`  → base/iterators.jl
#       - `mapfoldr(f, op, itr [, init])`  → base/iterators.jl
#       - `mapreduce(f, op, itr [, init])` → base/iterators.jl
#
# Each of the assertions below was verified against official Julia 1.12.

using Test

@testset "map!(f, a::Array) Pure Julia dispatch" begin
    a = [1, 2, 3]
    map!(x -> x * 2, a)
    @test a == [2, 4, 6]
    # Lambda referencing closure
    b = [1.0, 2.0, 3.0]
    k = 10
    map!(x -> x + k, b)
    @test b == [11.0, 12.0, 13.0]
end

@testset "map!(f, dest, src) Pure Julia dispatch" begin
    src = [1, 2, 3, 4]
    dest = [0, 0, 0, 0]
    map!(x -> x * x, dest, src)
    @test dest == [1, 4, 9, 16]
    # Different sized destination uses min(length(dest), length(src))
    src2 = [10, 20, 30]
    dest2 = [0, 0, 0, 0, 0]
    map!(x -> x + 1, dest2, src2)
    @test dest2[1:3] == [11, 21, 31]
end

@testset "filter!(f, a::Array) Pure Julia dispatch" begin
    a = [1, 2, 3, 4, 5]
    filter!(x -> x > 2, a)
    @test a == [3, 4, 5]
    # Predicate that filters everything
    b = [1, 2, 3]
    filter!(x -> x > 100, b)
    @test isempty(b)
    # Predicate that keeps everything
    c = [1, 2, 3]
    filter!(x -> true, c)
    @test c == [1, 2, 3]
end

@testset "mapfoldl Pure Julia dispatch" begin
    @test mapfoldl(x -> x * x, +, [1, 2, 3]) == 14   # 1 + 4 + 9
    @test mapfoldl(x -> x * x, -, [1, 2, 3]) == -12  # (1 - 4) - 9
    # init keyword form (compiler rewrites to positional, then method dispatch)
    @test mapfoldl(x -> x + 1, +, [1, 2, 3]; init=0) == 9
end

@testset "mapfoldr Pure Julia dispatch" begin
    @test mapfoldr(x -> x * x, -, [1, 2, 3]) == 6    # 1 - (4 - 9)
    @test mapfoldr(x -> x + 1, +, [1, 2, 3]) == 9    # 2 + (3 + 4)
    @test mapfoldr(x -> x * 2, +, [1, 2, 3]; init=0) == 12
end

@testset "mapreduce Pure Julia dispatch" begin
    @test mapreduce(x -> x * x, +, [1, 2, 3]) == 14
    @test mapreduce(x -> x * 2, +, [1, 2, 3]; init=100) == 112
end

@testset "Closure capture inside HOFs" begin
    factor = 3
    a = [1, 2, 3]
    map!(x -> x * factor, a)
    @test a == [3, 6, 9]
    @test mapreduce(x -> x * factor, +, [1, 2, 3]) == 18
end

true
