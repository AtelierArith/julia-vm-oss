using Test

function callable_value_resolver_3910(x::Any)
    return :any
end

function callable_value_resolver_3910(x::Int64)
    return :int
end

function callable_value_resolver_3910(x::Float64)
    return :float
end

function call_through_value_3910(f, x)
    g = f
    y::Any = x
    return g(y)
end

@test call_through_value_3910(callable_value_resolver_3910, 1) == :int
@test call_through_value_3910(callable_value_resolver_3910, 1.5) == :float
@test call_through_value_3910(callable_value_resolver_3910, "fallback") == :any

true
