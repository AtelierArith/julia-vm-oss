using Random

# Issue #9329: Random.rand! / Random.randn! fill an existing array in place and
# return that same array. The assertions are type/behavioral instead of exact
# stream checks because MersenneTwister's upstream dSFMT stream remains tracked
# separately (Issue #8998).
#
# Issue #9553: randn(rng, ::Type{Float32}) is the scalar typed normal draw used
# by upstream's randn! design; it must not be interpreted as randn(rng, dims...).

function all_unit_interval_9329(A)
    for i in eachindex(A)
        x = A[i]
        if !(0.0 <= x < 1.0)
            return false
        end
    end
    return true
end

function all_finite_9329(A)
    for i in eachindex(A)
        if !isfinite(A[i])
            return false
        end
    end
    return true
end

function any_nonzero_9329(A)
    for i in eachindex(A)
        if A[i] != 0
            return true
        end
    end
    return false
end

function rand_bang_fills_float_vector_9329()
    v = zeros(4)
    ret = rand!(v)
    return ret === v &&
           typeof(v) == Vector{Float64} &&
           all_unit_interval_9329(v) &&
           any_nonzero_9329(v)
end

function rand_bang_fills_float_matrix_9329()
    m = zeros(2, 3)
    ret = rand!(m)
    return ret === m &&
           typeof(m) == Matrix{Float64} &&
           size(m, 1) == 2 &&
           size(m, 2) == 3 &&
           all_unit_interval_9329(m)
end

function rand_bang_preserves_integer_eltype_9329()
    a = zeros(Int, 4)
    b = zeros(Int, 4)
    rand!(Xoshiro(7), a)
    rand!(Xoshiro(7), b)
    return typeof(a) == Vector{Int64} &&
           all(x -> x isa Int64, a) &&
           a == b
end

function randn_bang_fills_float_vector_9329()
    v = zeros(5)
    ret = randn!(v)
    return ret === v &&
           typeof(v) == Vector{Float64} &&
           length(v) == 5 &&
           all_finite_9329(v) &&
           any_nonzero_9329(v)
end

function randn_bang_preserves_float32_eltype_9329()
    a = zeros(Float32, 4)
    b = zeros(Float32, 4)
    ret = randn!(Xoshiro(7), a)
    randn!(Xoshiro(7), b)
    return ret === a &&
           typeof(a) == Vector{Float32} &&
           all(x -> x isa Float32, a) &&
           all_finite_9329(a) &&
           a == b
end

function randn_bang_rejects_nonfloat_nonempty_9329()
    empty = zeros(Int, 0)
    if randn!(Xoshiro(1), empty) !== empty
        return false
    end

    a = zeros(Int, 2)
    try
        randn!(Xoshiro(1), a)
        return false
    catch e
        return e isa MethodError && a == zeros(Int, 2)
    end
end

function typed_randn_scalar_float32_9553()
    x = randn(Xoshiro(7), Float32)
    y = randn(Float32)
    return x isa Float32 &&
           y isa Float32 &&
           isfinite(x) &&
           isfinite(y)
end

println(rand_bang_fills_float_vector_9329())
println(rand_bang_fills_float_matrix_9329())
println(rand_bang_preserves_integer_eltype_9329())
println(randn_bang_fills_float_vector_9329())
println(randn_bang_preserves_float32_eltype_9329())
println(randn_bang_rejects_nonfloat_nonempty_9329())
println(typed_randn_scalar_float32_9553())

rand_bang_fills_float_vector_9329() &&
    rand_bang_fills_float_matrix_9329() &&
    rand_bang_preserves_integer_eltype_9329() &&
    randn_bang_fills_float_vector_9329() &&
    randn_bang_preserves_float32_eltype_9329() &&
    randn_bang_rejects_nonfloat_nonempty_9329() &&
    typed_randn_scalar_float32_9553()
