using Test

# Issue #5159: repr/show of Bool arrays must match upstream Julia.
# Bool elements render as the integers 1/0 (not true/false) and a
# `Bool[...]` type prefix is emitted, because upstream classifies Bool
# as a non-implicit eltype in its typeinfo-aware array show.

@testset "repr(::Vector{Bool}) renders Bool[1, 0] (Issue #5159)" begin
    @test repr([true, false]) == "Bool[1, 0]"
    @test repr([true]) == "Bool[1]"
    @test repr([false, false, true]) == "Bool[0, 0, 1]"
    @test repr([true, true, true]) == "Bool[1, 1, 1]"
end

@testset "print(::Vector{Bool}) matches repr (Issue #5159)" begin
    io = IOBuffer()
    print(io, [true, false])
    @test String(take!(io)) == "Bool[1, 0]"
end

@testset "string(::Vector{Bool}) matches repr (Issue #5159)" begin
    @test string([true, false]) == "Bool[1, 0]"
end

@testset "repr(::Matrix{Bool}) renders Bool[...] form (Issue #5159)" begin
    @test repr(Bool[true false; false true]) == "Bool[1 0; 0 1]"
    @test repr(Bool[true true; false false]) == "Bool[1 1; 0 0]"
end

@testset "print/string(::Matrix{Bool}) matches repr (Issue #5159)" begin
    @test string(Bool[true false; false true]) == "Bool[1 0; 0 1]"
    io = IOBuffer()
    print(io, Bool[true false; false true])
    @test String(take!(io)) == "Bool[1 0; 0 1]"
end

@testset "empty Bool vector repr preserves type (Issue #5159)" begin
    @test repr(Bool[]) == "Bool[]"
end

@testset "non-Bool element types are unaffected (Issue #5159)" begin
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr([1.0, 2.0]) == "[1.0, 2.0]"
    @test repr([1 2; 3 4]) == "[1 2; 3 4]"
end

true
