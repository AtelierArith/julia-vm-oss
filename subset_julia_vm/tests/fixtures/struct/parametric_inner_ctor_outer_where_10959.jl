using Test

# An ordinary outer constructor may itself have a `where` parameter. Its value
# signature must not make it eligible for an explicit `Window{T}(...)` call:
# upstream distinguishes the implicit Type{Window} / Type{Window{T}} self
# arguments even though sjulia's projected method table does not expose them.
struct Window10959{T}
    value::T
    offset::Int
    width::Int

    function Window10959{T}(value::T, first::Int, last::Int) where T
        new(value, first - 1, last - first + 1)
    end
end

Window10959(value::T, first::Integer, last::Integer) where T =
    Window10959{T}(value, Int(first), Int(last))

# A bare outer and an explicit-parametric inner may have exactly the same value
# signature. The bare call must still select the outer; only `ExactCtor{T}` may
# select the explicit inner.
struct ExactCtor10959{T}
    value::T

    function ExactCtor10959{T}(value::T) where T
        new(value + one(value))
    end
end

ExactCtor10959(value::T) where T = ExactCtor10959{T}(value * 10)

# Runtime expressions such as `typeof(value)` are not statically forwardable
# `where` bindings. They must remain on the dynamic type-application path.
struct RuntimeExprCtor10959{T}
    value::T

    function RuntimeExprCtor10959{T}(value) where T
        new(value)
    end
end

make_runtime_expr_10959(value) = RuntimeExprCtor10959{typeof(value)}(value)

# Preserving a colliding outer row must not change Julia's normal
# last-definition-wins behavior for two inner methods with the same self and
# value signature.
struct RedefinedInner10959{T}
    value::T

    RedefinedInner10959{T}(value::T) where T = new(value + one(value))
    RedefinedInner10959{T}(value::T) where T = new(value + one(value) + one(value))
end

# Bare and explicit-parametric inner constructors are also distinct self
# families even though both originate in the struct body.
struct DualSelfInner10959{T}
    value::T

    DualSelfInner10959(value::T) where T = new{T}(value + 10)
    DualSelfInner10959{T}(value::T) where T = new(value + 1)
end

# When imprecise compile-time argument inference leaves a sole explicit inner
# as the fallback, its typed prologue must still reject a runtime-incompatible
# value instead of silently coercing or constructing invalid field storage.
struct StrictInner10969{T}
    value::T

    StrictInner10969{T}(value::T) where T = new(value)
end

strict_inner_bad_10969(x::T) where T = StrictInner10969{T}(string(x))

struct ConvertingOuter10969{T}
    value::T

    ConvertingOuter10969{T}(value::T) where T = new(value)
end

# Explicit self type arguments participate in value-signature overload
# selection; ::Number and ::T remain distinct inner methods (Issue #10993).
struct StaticExplicitOverload10993{T}
    value::T

    function StaticExplicitOverload10993{T}(value::Number) where {T<:Number}
        1
    end

    function StaticExplicitOverload10993{T}(value::T) where {T<:Number}
        2
    end
end

ConvertingOuter10969{T}(value) where T = ConvertingOuter10969{T}(T(value))
convert_unknown_10969(::Type{T}, value) where T = ConvertingOuter10969{T}(value)

@testset "parametric inner origin and runtime forwarding (Issues #10959, #10967)" begin
    direct = Window10959{String}("abc", 2, 3)
    @test typeof(direct) === Window10959{String}
    @test (direct.offset, direct.width) == (1, 2)

    # Issue #10967: the runtime `T` in the outer body must be forwarded to the
    # inner call; field-count equality cannot trigger raw struct allocation.
    via_outer = Window10959("abcd", Int8(2), Int16(4))
    @test typeof(via_outer) === Window10959{String}
    @test (via_outer.offset, via_outer.width) == (1, 3)

    @test ExactCtor10959(2).value == 21
    @test ExactCtor10959{Int64}(2).value == 3

    runtime_expr = make_runtime_expr_10959(7)
    @test typeof(runtime_expr) === RuntimeExprCtor10959{Int64}
    @test runtime_expr.value == 7

    @test RedefinedInner10959{Int64}(1).value == 3

    @test DualSelfInner10959(1).value == 11
    @test DualSelfInner10959{Int64}(1).value == 2

    @test_throws MethodError strict_inner_bad_10969(1)

    # The primary inner rejects Float64 at runtime; the unique untyped outer
    # converts it, then re-enters the now-applicable inner constructor.
    @test convert_unknown_10969(Int64, 7.0).value == 7
    @test StaticExplicitOverload10993{Float64}(1) == 1
    @test StaticExplicitOverload10993{Float64}(1.0) == 2
end

true
