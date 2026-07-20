# A `Union` type value classifies as the `Union` kind — not `DataType` — for
# isa/dispatch (consistent with `typeof`), and exposes its branch types
# through the fields `a`/`b` like upstream's `Union` struct. Issue #10313.

using Test

@testset "Union value isa classification (Issue #10313)" begin
    u = Union{Int64,Float64}
    @test typeof(u) == Union
    @test !(u isa DataType)
    @test u isa Union
    @test !(u isa UnionAll)
    @test u isa Type
    # The `Union` kind itself is an ordinary DataType.
    @test Union isa DataType
    # `Union{}` (Bottom) is Core.TypeofBottom, not a Union object.
    @test !(Union{} isa Union)
    @test Union{} isa Type
end

@testset "Union branch-type fields a/b (Issue #10313)" begin
    u = Union{Int64,Float64}
    # Upstream normalizes arm order at construction (`union_sort_cmp` in
    # julia/src/jltypes.c): singletons first, then isbits DataTypes, then
    # other DataTypes, then UnionAlls; ties break alphabetically. Both isbits
    # arms here sort alphabetically, so `a` is Float64.
    @test u.a === Float64
    @test u.b === Int64
    # An n-arm union nests right-associated binary a/b chains.
    v = Union{Int,Float64,String}
    @test v.a === Float64
    @test v.b === Union{Int64,String}
    @test v.b.a === Int64
    @test v.b.b === String
    # Singleton arms sort ahead of isbits arms; UnionAll arms sort last.
    @test Union{Nothing,Int64}.a === Nothing
    @test Union{Nothing,Int64}.b === Int64
    @test Union{Vector,Int64}.a === Int64
    @test Union{Vector,Int64}.b === Vector
    # The getfield spelling matches dot access.
    @test getfield(u, :a) === Float64
    @test getfield(Union{Int,Float64,String}, :b) === Union{Int64,String}
end

f10313_isa(u) = u isa DataType
f10313_a(u) = u.a

@testset "Union classification through function calls (Issue #10313)" begin
    @test !f10313_isa(Union{Int64,Float64})
    @test f10313_a(Union{Int64,Float64}) === Float64
end

true
