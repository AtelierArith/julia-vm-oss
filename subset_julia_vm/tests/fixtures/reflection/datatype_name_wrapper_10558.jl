using Test

struct WrapperField10557{T} end
struct WrapperPair10558{A,B} end
struct WrapperNoParam10558 end
@enum WrapperColor10558 wc_red wc_green wc_blue

@testset "DataType.name.wrapper exposes the canonical generic wrapper (Issue #10558)" begin
    # Primary MWE: the concrete parametric type recovers its generic wrapper.
    @test WrapperField10557{Int64}.name.wrapper === WrapperField10557

    # Identity is stable across concrete instantiations.
    @test WrapperField10557{Int64}.name.wrapper ===
          WrapperField10557{Float64}.name.wrapper

    # Multi-parameter user struct.
    @test WrapperPair10558{Int,Float64}.name.wrapper === WrapperPair10558

    # Builtin parametric types (including Base display aliases that share a
    # TypeName: Vector/Matrix/Array all resolve through the Array wrapper).
    @test Complex{Float64}.name.wrapper === Complex
    @test Rational{Int}.name.wrapper === Rational
    @test Vector{Int}.name.wrapper === Array
    @test Matrix{Int}.name.wrapper === Array
    @test Array{Int,2}.name.wrapper === Array

    # Non-parametric and abstract types are their own wrapper.
    @test Number.name.wrapper === Number
    @test WrapperNoParam10558.name.wrapper === WrapperNoParam10558
    @test Int64.name.wrapper === Int64
    @test typeof(1).name.wrapper === Int64
    @test WrapperColor10558.name.wrapper === WrapperColor10558

    # The wrapper is a UnionAll for parametric types.
    @test WrapperField10557{Int64}.name.wrapper isa UnionAll

    # getfield form matches the property form.
    @test getfield(WrapperField10557{Int64}.name, :wrapper) === WrapperField10557

    # Unknown fields still raise a catchable FieldError.
    @test_throws FieldError WrapperField10557{Int64}.name.nope
end

true
