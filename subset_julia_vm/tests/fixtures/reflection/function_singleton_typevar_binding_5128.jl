# Issue #5128: each function has its own singleton type, typeof(f) <: Function.
# A `where {F}` / `where {F<:Function}` type variable matched against a function
# argument must bind to that function's singleton type, not Any.

using Test

# Unbounded type variable binds to the function singleton type.
ftype(x::F) where {F} = F
# Bounded `F<:Function` binds the same singleton type.
gtype(x::F) where {F<:Function} = F
# The bound singleton type is usable as a value.
hsub(x::F) where {F<:Function} = F <: Function
# Singleton identity: the bound type variable is exactly typeof(x).
idsame(x::F) where {F} = (F === typeof(x))
# Two function args get independent singleton types.
distinct(f::F, g::G) where {F, G} = (F === G)

@testset "function singleton typevar binding (Issue #5128)" begin
    @test ftype(sin) === typeof(sin)
    @test ftype(+) === typeof(+)
    @test ftype(cos) === typeof(cos)

    @test gtype(sin) === typeof(sin)

    @test hsub(sin) == true
    @test hsub(+) == true

    @test idsame(sin) == true
    @test idsame(+) == true

    @test distinct(sin, sin) == true
    @test distinct(sin, cos) == false

    # typeof(f) is a subtype of Function and is a concrete singleton type.
    @test typeof(sin) <: Function
    @test (ftype(sin) <: Function) == true
end

true
