# Test Base.issingletontype - a concrete type with exactly one instance (Issue #5103)

using Test

struct EmptyS end
mutable struct EmptyMS end
struct WithFields
    x::Int
end
abstract type AbsT end
struct ParamS{T} end

@testset "Base.issingletontype" begin
    # Singleton struct (no fields, immutable) -> true
    @test Base.issingletontype(EmptyS) == true

    # Nothing is a singleton type -> true
    @test Base.issingletontype(Nothing) == true

    # Primitive / numeric concrete type -> false
    @test Base.issingletontype(Int) == false

    # Abstract type -> false
    @test Base.issingletontype(AbsT) == false

    # Concrete struct with fields -> false
    @test Base.issingletontype(WithFields) == false

    # UnionAll (parametric without bound parameter) is not concrete -> false
    @test Base.issingletontype(ParamS) == false

    # Empty mutable struct: each instance is distinct -> false
    @test Base.issingletontype(EmptyMS) == false

    # String is variable-size (mutable layout upstream) -> false
    @test Base.issingletontype(String) == false

    # Previously-deferred cases, re-enabled now that isconcretetype is fixed
    # (Issue #5203): a concrete parametric struct instantiation, a function
    # singleton type, and the empty tuple type are all singletons upstream.
    @test Base.issingletontype(ParamS{Int}) == true
    @test Base.issingletontype(typeof(println)) == true
    @test Base.issingletontype(Tuple{}) == true
end

true  # Test passed
