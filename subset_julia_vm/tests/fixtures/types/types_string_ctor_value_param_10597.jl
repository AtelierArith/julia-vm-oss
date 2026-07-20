using Test

# Regression fixture for Issue #10597: the uppercase `String` constructor applied
# to a where-bound value parameter (`g(::VP{v}) where {v} = String(v)`) used to
# raise a spurious compile-time `Unknown function: String`. `String` has a
# pure-Julia method table (String(::Symbol) = string(s), …) but no builtin op, so
# the static builtin-fallback arm in the call compiler misrouted the runtime-
# unknown value-parameter argument to `compile_builtin_call`. The fix lets a
# value-type-parameter argument fall through to runtime dynamic dispatch, exactly
# like an `Any`-typed argument, so `String(v)` dispatches `String(::Symbol)` on
# the actual runtime value.
struct VP10597{v} end

# The reported case: uppercase `String` constructor on a where-bound value param.
g_string_10597(::VP10597{v}) where {v} = String(v)
# Sibling lowercase / Symbol constructors already worked; keep them green so a
# future routing change cannot regress them while "fixing" String.
g_lower_10597(::VP10597{v}) where {v} = string(v)
g_symbol_10597(::VP10597{v}) where {v} = Symbol(v)

# `String` on a type-parameter *value* argument already worked before the fix.
h_typeparam_10597(x::T) where {T} = String(x)

# Note: only Symbol-*literal* value parameters are exercised here. Non-literal /
# constructor-form value parameters such as `VP{Symbol("a b")}` bind as a
# `DataType` wrapper in sjulia (they already mis-print under the working
# `string(v)` too) — that is the separate binding gap tracked by Issue #10599,
# not the compile-time constructor-name resolution fixed here.
@testset "String constructor on where-bound value parameter (Issue #10597)" begin
    @test g_string_10597(VP10597{:tag}()) == "tag"
    @test g_string_10597(VP10597{:tag}()) isa String
    @test g_string_10597(VP10597{:hello}()) == "hello"
    @test g_lower_10597(VP10597{:tag}()) == "tag"
    @test g_symbol_10597(VP10597{:tag}()) === :tag
    @test h_typeparam_10597(:tag) == "tag"
end

true
