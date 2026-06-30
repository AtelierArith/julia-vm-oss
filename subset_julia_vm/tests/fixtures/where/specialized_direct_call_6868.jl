using Test

# Issue #6868: a direct call to a `where`-parametric method
# (`f(x::T) where T<:Real`) used to run the method's fully generic body with
# every parameter bound to `Any`, dynamically dispatching the inner operators —
# slower than both the untyped-generic and the concrete-typed forms. The fix
# specializes the body for the concrete runtime argument types on the direct
# call path (cached), so it now produces concretely-typed results while still
# honoring the type bound and the type variable `T`.
#
# This test guards behavior (not timing): the specialized body must compute the
# same values/types as upstream Julia, the `T<:Real` bound must still reject a
# non-Real argument with a MethodError, and bodies that reference `T`
# (`zero(T)`, `one(T)`) must keep `T` bound to the concrete argument type.

@testset "where-parametric direct-call specialization (Issue #6868)" begin
    f(x::T) where {T<:Real} = x + 1

    @test f(3) == 4
    @test f(2.5) == 3.5
    @test typeof(f(3)) == Int64
    @test typeof(f(2.5)) == Float64

    # Repeated calls (exercises the specialization cache) stay correct.
    acc = 0.0
    for i in 1:1000
        acc += f(0.001 * i)
    end
    # sum_{i=1}^{1000} (0.001*i + 1) = 0.001*500500 + 1000 = 1500.5
    @test acc ≈ 1500.5

    # The `T<:Real` bound is still enforced: a String argument has no method.
    @test_throws MethodError f("hello")

    # Bodies that reference the type variable keep `T` bound to the concrete type.
    g(x::T) where {T<:Real} = zero(T) + x
    @test g(5) == 5
    @test g(2.0) == 2.0
    @test typeof(g(5)) == Int64
    @test typeof(g(2.0)) == Float64

    # Parametric container bound (`Vector{T} where T<:Real`).
    h(xs::Vector{T}) where {T<:Real} = xs[1] + one(T)
    @test h([10, 20]) == 11
    @test h([1.5, 2.5]) == 2.5
    @test typeof(h([10, 20])) == Int64
    @test typeof(h([1.5, 2.5])) == Float64
end

true
