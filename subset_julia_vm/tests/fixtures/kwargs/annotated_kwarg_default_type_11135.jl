# Issue #11135: a keyword's declared type constrains its DEFAULT as well as
# caller-supplied values. Upstream lowers the keyword sorter to an inner method
# with a typed positional parameter, so a wrong-typed default raises MethodError
# before the user body runs.

using Test

bad_literal_default_11135(; k::Integer = "oops") = k
bad_concrete_default_11135(; k::Int64 = "oops") = k
bad_float_default_11135(; k::Float64 = 1) = k
bad_bool_default_11135(; k::Bool = 1) = k
bad_string_default_11135(; k::String = 1) = k
bad_call_default_11135(; k::Integer = string(1)) = k
bad_stub_default_11135(y, x=2; k::Integer = string(1)) = (y, x, k)
bad_literal_stub_default_11135(y, x=2; k::Integer = "oops") = (y, x, k)
bad_typevar_default_11135(::Type{T}; k::T = "oops") where {T} = k
bad_arrow_default_11135 = (y, x=2; k::Integer = "oops") -> (y, x, k)
bad_full_value_default_11135 = function (; k::Integer = "oops")
    k
end
bad_full_iife_default_11135() = (function (; k::Integer = "oops")
    k
end)()
good_full_iife_supplied_11135() = (function (; k::Integer = "oops")
    k
end)(k = 7)
required_full_value_11135 = function (; k::Integer)
    k
end

struct KwDefaultToken11135
    value::Int
end
bad_struct_default_11135(; k::KwDefaultToken11135 = 1) = k

mutable struct MutableKwDefaultToken11135{T}
    value::T
end
MutableKwDefaultAlias11135 = MutableKwDefaultToken11135{Int}
bad_mutable_alias_default_11135(; k::MutableKwDefaultAlias11135 = 1) = k

anonymous_kwargs_11135 = function (; required::Integer, defaulted::Integer = 3, kwargs...)
    (required, defaulted, length(kwargs))
end

good_literal_default_11135(; k::Integer = 3) = k
good_call_default_11135(; k::Integer = typemax(Int)) = k
good_abstract_default_11135(; k::Real = 2.5) = k
where_supplied_default_11135(::Type{T}; k::T = 1) where {T} = k

default_order_events_11135 = String[]
record_later_default_11135() = (push!(default_order_events_11135, "later"); 2)
throw_later_default_11135() = error("later default wins")
bad_body_default_11135() = "bad"
bad_order_default_11135(; a::Int = "bad", b = record_later_default_11135()) = (a, b)
bad_body_order_default_11135(; a::Int = bad_body_default_11135(), b = record_later_default_11135()) = (a, b)
bad_exception_precedence_11135(; a::Int = "bad", b = throw_later_default_11135()) = (a, b)

@testset "annotated keyword defaults satisfy their declared type (Issue #11135)" begin
    @test_throws MethodError bad_literal_default_11135()
    @test_throws MethodError bad_concrete_default_11135()
    @test_throws MethodError bad_float_default_11135()
    @test_throws MethodError bad_bool_default_11135()
    @test_throws MethodError bad_string_default_11135()
    @test_throws MethodError bad_struct_default_11135()
    @test_throws MethodError bad_mutable_alias_default_11135()
    @test_throws MethodError bad_call_default_11135()
    @test_throws MethodError bad_stub_default_11135(1)
    @test_throws MethodError bad_literal_stub_default_11135(1)
    @test_throws MethodError bad_typevar_default_11135(Int64)
    @test_throws MethodError bad_arrow_default_11135(1)
    @test_throws MethodError bad_full_value_default_11135()
    @test_throws MethodError bad_full_iife_default_11135()

    # Correct supplied values must not be coerced by, or rejected at, the
    # synthetic entry assertion (including specialized slot types).
    @test bad_concrete_default_11135(k = 7) == 7
    @test bad_float_default_11135(k = 2.5) == 2.5
    @test bad_bool_default_11135(k = true)
    @test bad_string_default_11135(k = "ok") == "ok"
    @test bad_struct_default_11135(k = KwDefaultToken11135(4)).value == 4
    @test bad_mutable_alias_default_11135(k = MutableKwDefaultToken11135{Int}(5)).value == 5
    @test bad_full_value_default_11135(k = 7) == 7
    @test good_full_iife_supplied_11135() == 7
    @test_throws UndefKeywordError required_full_value_11135()
    @test required_full_value_11135(k = 8) == 8
    @test anonymous_kwargs_11135(required = 9, extra = 10) == (9, 3, 1)
    @test anonymous_kwargs_11135(required = 9, defaulted = 4) == (9, 4, 0)
    @test_throws UndefKeywordError anonymous_kwargs_11135()

    # A caught bad default must not leak pending validation into pooled frames.
    for i in 1:5
        @test_throws MethodError bad_concrete_default_11135()
        @test bad_concrete_default_11135(k = i) == i
    end

    @test good_literal_default_11135() == 3
    @test good_call_default_11135() == typemax(Int)
    @test good_abstract_default_11135() == 2.5

    # A supplied keyword annotation may depend on a positional where binding.
    @test where_supplied_default_11135(Int64; k = 4) == 4
    @test_throws TypeError where_supplied_default_11135(Int64; k = 4.5)

    # Upstream materializes every omitted default left-to-right before the
    # typed inner-method assertion. A later default's effects/errors therefore
    # happen before the earlier bad annotation raises MethodError.
    empty!(default_order_events_11135)
    @test_throws MethodError bad_order_default_11135()
    @test default_order_events_11135 == ["later"]
    empty!(default_order_events_11135)
    @test_throws MethodError bad_body_order_default_11135()
    @test default_order_events_11135 == ["later"]
    precedence_error_11135 = try
        bad_exception_precedence_11135()
        nothing
    catch e
        e
    end
    @test precedence_error_11135 isa ErrorException

    # A caller-supplied mismatch keeps upstream's distinct TypeError boundary.
    @test_throws TypeError good_literal_default_11135(k = 3.0)
end

true
