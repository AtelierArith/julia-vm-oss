# Regression test for Issue #7793 (PR #7826 follow-up): the field-count
# default-constructor fallback must NOT manufacture the default constructor when
# the call's argument types are not convertible to the (concrete) field types.
#
# When a user OUTER constructor exists but a matching-arity call's argument types
# match NEITHER the outer ctor NOR the field types, the synthesized field-count
# default constructor's implicit `convert(fieldtype, x)` must fail with a
# CATCHABLE runtime MethodError — exactly as upstream Julia does — instead of
# `compile_struct_constructor` raising an UNCATCHABLE compile-time
# "Cannot convert ..." error. The fix adds a side-effect-free convertibility
# pre-check (`coercion_accepts`) before the fallback so non-convertible calls
# fall through to normal dispatch.
#
# Verified against upstream julia 1.12 first (6/6 pass).

using Test

# Single-field struct + arity-1 outer ctor on AbstractString.
struct W1
    s::String
end
W1(s::AbstractString) = W1(String(s))

# Two-field struct + arity-2 outer ctor on Symbols, both fields String.
struct T2
    a::String
    b::String
end
T2(x::Symbol, y::Symbol) = T2(string(x), string(y))

@testset "non-convertible field-count default ctor is a catchable MethodError (#7793)" begin
    # Convertible / declared paths still work.
    @test W1("hi").s == "hi"                          # outer ctor (AbstractString)
    @test (T2(:p, :q).a, T2(:p, :q).b) == ("p", "q")  # outer ctor (Symbols)
    @test (T2("x", "y").a, T2("x", "y").b) == ("x", "y")  # field-count default ctor (exact types)

    # Non-convertible matching-arity calls: the field-count default ctor's
    # convert(String, ::Int) fails as a CATCHABLE error (not a compile abort).
    @test_throws MethodError W1(5)
    @test_throws MethodError T2(1, 2)

    # The catchable error can be observed via try/catch (it is NOT a compile abort).
    caught = false
    try
        W1(5)
    catch e
        caught = e isa MethodError
    end
    @test caught
end

true  # Test passed
