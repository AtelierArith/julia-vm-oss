# Verify predicate-HOF reducer forms dispatch to Pure Julia (Issue #3728)
#
# Before #3728:
#   - `compile/expr/builtin_hof.rs` routed `findall(f, arr)`,
#     `findfirst(f, arr)`, `findlast(f, arr)`, `count(f, arr)`,
#     `sum(f, arr)`, `any(f, arr)`, `all(f, arr)` to dedicated VM HOF
#     instructions (`FindAllFunc`, `FindFirstFunc`, `FindLastFunc`,
#     `CountFunc`, `SumFunc`, `AnyFunc`, `AllFunc`).
#
# After #3728:
#   - Those names dispatch to Pure Julia methods:
#       - findfirst(f::Function, arr::Array)  → base/array.jl
#       - findlast(f::Function, arr::Array)   → base/array.jl
#       - findall(f::Function, arr::Array)    → base/reduce.jl (new)
#       - any(f::Function, arr)               → base/reduce.jl (new)
#       - all(f::Function, arr)               → base/reduce.jl (new)
#       - count(f::Function, arr::Array)      → base/reduce.jl (new)
#       - sum(f::Function, arr::Array)        → base/reduce.jl (new)
#
# All assertions verified against official Julia 1.12.

using Test

@testset "any(f, arr) Pure Julia dispatch" begin
    @test any(isodd, [1, 2, 4]) == true
    @test any(isodd, [2, 4, 6]) == false
    @test any(x -> x > 10, [1, 2, 3]) == false
    @test any(x -> x > 2, [1, 2, 3]) == true
    @test any(x -> x > 0, []) == false
end

@testset "all(f, arr) Pure Julia dispatch" begin
    @test all(x -> x > 0, [1, 2, 3]) == true
    @test all(x -> x > 1, [1, 2, 3]) == false
    @test all(iseven, [2, 4, 6]) == true
    @test all(iseven, [2, 3, 4]) == false
    @test all(x -> x > 0, []) == true
end

@testset "count(f, arr) Pure Julia dispatch" begin
    @test count(x -> x > 1, [1, 2, 3]) == 2
    @test count(isodd, [1, 2, 3, 4, 5]) == 3
    @test count(x -> x > 100, [1, 2, 3]) == 0
end

@testset "findall(f, arr) Pure Julia dispatch" begin
    @test findall(x -> x > 1, [1, 2, 3]) == [2, 3]
    @test findall(isodd, [1, 2, 3, 4, 5]) == [1, 3, 5]
    @test isempty(findall(x -> x > 100, [1, 2, 3]))
end

@testset "findfirst(f, arr) / findlast(f, arr) Pure Julia dispatch" begin
    @test findfirst(x -> x == 2, [1, 2, 3]) == 2
    @test findfirst(x -> x > 100, [1, 2, 3]) === nothing
    @test findlast(x -> x > 1, [1, 2, 3]) == 3
    @test findlast(x -> x > 100, [1, 2, 3]) === nothing
    @test findfirst(iseven, [1, 3, 5, 6, 7]) == 4
end

@testset "sum(f, arr) Pure Julia dispatch" begin
    # Pure Julia preserves Int when the predicate maps Ints to Ints
    @test sum(x -> x * x, [1, 2, 3]) == 14
    @test sum(x -> x * 2, [1, 2, 3]) == 12
    # Float input stays Float
    @test sum(x -> x * 2.0, [1, 2, 3]) == 12.0
end

@testset "Closure capture inside predicate HOFs" begin
    threshold = 2
    @test count(x -> x > threshold, [1, 2, 3, 4, 5]) == 3
    @test findall(x -> x > threshold, [1, 2, 3, 4, 5]) == [3, 4, 5]
    @test sum(x -> x + threshold, [1, 2, 3]) == 12  # 3 + 4 + 5
end

true
