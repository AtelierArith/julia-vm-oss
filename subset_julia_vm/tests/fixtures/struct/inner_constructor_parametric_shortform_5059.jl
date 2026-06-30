# Short-form parametric inner constructors with new{T} (Issue #5059)
#
# The short-form `Foo{T}(x) where T = new{T}(x)` was being dropped during
# lowering (the `where`-clause LHS of the assignment was not recognized), so
# the auto-generated default constructor silently took over and ignored the
# user's `new{...}` body — including any value transform or field reordering.
#
# This verifies that the short-form parametric inner constructor body now runs:
#   * a transforming body (`new{T}(x * 10)`) is honored;
#   * `new{A,B}(y, x)` assigns its arguments in `new` order, not call order;
#   * a type parameter inferred from an argument (`Bar(x::T)`) produces the
#     fully-instantiated parametric type.

using Test

# Single type parameter whose body transforms the argument.
struct Tracked5059{T}
    x::T
    Tracked5059{T}(x) where T = new{T}(x * 10)
end

# Single type parameter inferred from the argument.
struct Bar5059{T}
    x::T
    Bar5059(x::T) where T = new{T}(x)
end

# Two type parameters, with new{A,B} reordering its arguments.
struct Swap5059{A,B}
    a::A
    b::B
    Swap5059{A,B}(x, y) where {A,B} = new{A,B}(y, x)
end

@testset "Short-form parametric inner constructors (Issue #5059)" begin
    # Body runs and honors the transform (default ctor would store 5).
    t = Tracked5059{Int}(5)
    @test t.x == 50

    # Type parameter inferred from the argument: full type and value.
    b = Bar5059(3.14)
    @test typeof(b) == Bar5059{Float64}
    @test b.x == 3.14

    # new{A,B}(y, x): call (2.0, 1) -> x=2.0, y=1 -> new(1, 2.0) -> a=1, b=2.0.
    # The body must run (not the default a=x, b=y constructor).
    s = Swap5059{Int,Float64}(2.0, 1)
    @test s.a == 1
    @test s.b == 2.0
end

true
