using Test

# Issue #2468: Runtime dispatch should bind where T type parameters
# When a parametric method is called through runtime dispatch (DynamicNeg, CallDynamic, etc.),
# the type parameter T from the where clause must be bound in the call frame.

struct Wrapper{T}
    val::T
end

# Unary negation with where T (dispatched via DynamicNeg -> find_best_method_index)
function Base.:-(x::Wrapper{T}) where T
    Wrapper{T}(-x.val)
end

# Binary addition with where T (dispatched via CallDynamic* paths)
function Base.:+(x::Wrapper{T}, y::Wrapper{T}) where T
    Wrapper{T}(x.val + y.val)
end

@testset "Runtime dispatch binds where T type parameters (Issue #2468)" begin
    a = Wrapper{Int64}(3)
    b = Wrapper{Int64}(5)

    # Negation: uses runtime dispatch when compiler sees type as Any
    c = -a
    @test c.val == -3

    # Addition: uses runtime dispatch when compiler sees type as Any
    d = a + b
    @test d.val == 8
end

true
