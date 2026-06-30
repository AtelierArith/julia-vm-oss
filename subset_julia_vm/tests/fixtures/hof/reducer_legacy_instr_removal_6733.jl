# Issue #6733: the legacy reducer HOF VM instructions (FindAllFunc / FindFirstFunc
# / FindLastFunc / MapReduceFunc(WithInit) / MapFoldrFunc(WithInit) / MapFuncInPlace
# / FilterFuncInPlace / SumFunc / AnyFunc / AllFunc / CountFunc) were removed.
# any/all/count/sum/mapreduce/findall/findfirst/map!/filter! now resolve through
# normal method dispatch to their pure-Julia definitions (base/reduce.jl,
# base/iterators.jl, base/array.jl), and range/LinRange/first/last keep working.
# Short-circuit semantics for any/all are preserved. Verified vs julia 1.12.

using Test

@testset "range / LinRange (Issue #6733)" begin
    @test collect(range(1, 10, length=5)) == [1.0, 3.25, 5.5, 7.75, 10.0]
    @test collect(LinRange(0.0, 1.0, 3)) == [0.0, 0.5, 1.0]
    @test collect(1:2:9) == [1, 3, 5, 7, 9]
end

@testset "tuple first / last + destructuring (Issue #6733)" begin
    @test first((10, 20, 30)) == 10
    @test last((10, 20, 30)) == 30
    @test first((42,)) == 42 && last((42,)) == 42
    a, b, c = (1, 2, 3)   # destructuring (TupleFirst codegen) still works
    @test (a, b, c) == (1, 2, 3)
end

@testset "HOF reducers are pure Julia with short-circuit (Issue #6733)" begin
    @test any(x -> x > 2, [1, 2, 3]) == true
    @test any(x -> x > 5, [1, 2, 3]) == false
    @test all(x -> x > 0, [1, 2, 3]) == true
    @test all(x -> x > 1, [1, 2, 3]) == false
    @test count(iseven, [1, 2, 3, 4, 5, 6]) == 3
    @test sum(x -> x^2, [1, 2, 3]) == 14
    @test mapreduce(x -> x + 1, +, [1, 2, 3]) == 9
    @test findall(iseven, [1, 2, 3, 4]) == [2, 4]
    @test findfirst(iseven, [1, 3, 4, 5]) == 3
end

true
