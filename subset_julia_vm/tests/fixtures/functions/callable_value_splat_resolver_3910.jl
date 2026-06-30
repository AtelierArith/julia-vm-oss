using Test

function callable_value_splat_resolver_3910(x::Any, y::Any)
    return :any_pair
end

function callable_value_splat_resolver_3910(x::Int64, y::Int64)
    return :int_pair
end

function callable_value_splat_resolver_3910(x::Int64, ys::Int64...)
    return :int_vararg
end

function call_splat_resolver_3910(f, xs)
    g = f
    return g(xs...)
end

@testset "callable value splat resolver uses shared dispatch (Issue #3910)" begin
    @test call_splat_resolver_3910(callable_value_splat_resolver_3910, (1, 2)) == :int_pair
    @test call_splat_resolver_3910(callable_value_splat_resolver_3910, (1, 2, 3)) == :int_vararg
    @test call_splat_resolver_3910(callable_value_splat_resolver_3910, ("a", "b")) == :any_pair
end

true
