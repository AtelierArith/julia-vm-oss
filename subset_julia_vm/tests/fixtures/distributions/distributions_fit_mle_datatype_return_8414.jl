using Distributions

function fit_mle_datatype_return_8414()
    ss = suffstats(Binomial, 5, [1, 2, 3, 2])
    b1 = fit_mle(Binomial, ss)
    b2 = fit_mle(Binomial, 5, [1, 2, 3, 2])

    return typeof(b1) === Binomial{Float64} &&
           b1 !== nothing &&
           params(b1) == (5, 0.4) &&
           typeof(b2) === Binomial{Float64} &&
           b2 !== nothing &&
           params(b2) == (5, 0.4)
end

fit_mle_datatype_return_8414()
