# Issue #6732: the Value::Set Rust carrier (and its HashSet ops/_set_* intrinsics)
# was removed; every Set is a pure-Julia Set{T} struct over Dict{T,Nothing}
# (base/set.jl) and the public API dispatches through its methods. Verified vs
# julia 1.12, including that Dict (also pure, #6731) still works alongside Set.

using Test

@testset "public Set API is pure struct dispatch (Issue #6732)" begin
    s = Set([1, 2, 3])
    @test typeof(s) == Set{Int64}
    @test eltype(s) == Int64
    @test length(s) == 3
    @test 2 in s && !(9 in s)
    push!(s, 4)
    @test 4 in s && length(s) == 4
    delete!(s, 2)
    @test !(2 in s) && sort(collect(s)) == [1, 3, 4]
    @test pop!(s, 1) == 1 && !(1 in s)
    empty!(s)
    @test length(s) == 0 && isempty(s)
end

@testset "set algebra is pure dispatch (Issue #6732)" begin
    a = Set([1, 2, 3])
    b = Set([2, 3, 4])
    @test sort(collect(union(a, b))) == [1, 2, 3, 4]
    @test sort(collect(intersect(a, b))) == [2, 3]
    @test sort(collect(setdiff(a, b))) == [1]
    @test sort(collect(symdiff(a, b))) == [1, 4]
    @test issubset(Set([1, 2]), a)
    @test !issubset(a, Set([1, 2]))
end

@testset "set construction forms + iteration (Issue #6732)" begin
    @test typeof(Set{String}()) == Set{String}
    @test length(Set([1, 1, 2, 2, 3])) == 3
    @test typeof(Set([i for i in 1:3])) == Set{Int64}
    total = 0
    for x in Set([10, 20, 30])
        total += x
    end
    @test total == 60
end

@testset "Dict still works alongside Set (Issues #6731/#6732)" begin
    d = Dict("a" => 1, "b" => 2)
    d["c"] = 3
    @test d["a"] == 1 && haskey(d, "b") && length(d) == 3
    @test typeof(d) == Dict{String,Int64}
end

true
