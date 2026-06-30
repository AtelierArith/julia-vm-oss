using Test

function callable_value_single_candidate_vararg_3910(xs::Int64...)
    return length(xs)
end

function call_function_value_two_args_3910(f, x, y)
    g = f
    return g(x, y)
end

function call_function_value_three_args_3910(f, x, y, z)
    g = f
    return g(x, y, z)
end

@test call_function_value_two_args_3910(callable_value_single_candidate_vararg_3910, 1, 2) == 2
@test call_function_value_three_args_3910(callable_value_single_candidate_vararg_3910, 1, 2, 3) == 3

true
