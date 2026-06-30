# fieldname(T, i) by 1-based index on user-defined and parametric struct
# types, plus the upstream-matching error behavior for abstract types and
# out-of-range / non-positive indices (ArgumentError). Also re-checks that
# fieldnames / fieldcount stay correct (no regression to #3909 / #5099 /
# #5101). Matches upstream Julia Base.fieldname (Issue #5098).

using Test

struct FieldName5098Foo
    x::Int
    y::Float64
end

struct FieldName5098Box{T}
    val::T
end

struct FieldName5098Pair{A, B}
    first::A
    second::B
end

struct FieldName5098Single
    only::Int
end

abstract type FieldName5098Abstract end

@testset "fieldname by index on a plain struct (Issue #5098)" begin
    @test fieldname(FieldName5098Foo, 1) === :x
    @test fieldname(FieldName5098Foo, 2) === :y
    @test isa(fieldname(FieldName5098Foo, 1), Symbol)
end

@testset "fieldname on concrete parametric instantiations (Issue #5098)" begin
    @test fieldname(FieldName5098Box{Int}, 1) === :val
    @test fieldname(FieldName5098Box{Float64}, 1) === :val
    @test fieldname(FieldName5098Pair{Int, String}, 1) === :first
    @test fieldname(FieldName5098Pair{Int, String}, 2) === :second
end

@testset "fieldname out-of-range index throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Foo, 3)
    @test_throws ArgumentError fieldname(FieldName5098Single, 2)
end

@testset "fieldname non-positive index throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Foo, 0)
    @test_throws ArgumentError fieldname(FieldName5098Foo, -1)
end

@testset "fieldname on abstract type throws ArgumentError (Issue #5098)" begin
    @test_throws ArgumentError fieldname(FieldName5098Abstract, 1)
end

@testset "fieldnames / fieldcount stay correct (Issue #5098 regression)" begin
    @test fieldnames(FieldName5098Foo) === (:x, :y)
    @test fieldcount(FieldName5098Foo) == 2
    @test fieldnames(FieldName5098Box{Int}) === (:val,)
    @test fieldcount(FieldName5098Box{Int}) == 1
    @test fieldnames(FieldName5098Pair{Int, String}) === (:first, :second)
    @test fieldcount(FieldName5098Pair{Int, String}) == 2
    # fieldname is consistent with fieldnames at every index.
    @test fieldname(FieldName5098Pair{Int, String}, 1) === fieldnames(FieldName5098Pair{Int, String})[1]
    @test fieldname(FieldName5098Pair{Int, String}, 2) === fieldnames(FieldName5098Pair{Int, String})[2]
end

true
