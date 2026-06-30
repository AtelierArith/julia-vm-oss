using Distributions

tol = 1e-6

# Workaround: omitted keyword defaults that reference globals can evaluate as 0
# in sjulia (Issue #7774). Keep tolerance helpers positional-only here.
close(a, b) = abs(a - b) < tol
close_atol(a, b, atol) = abs(a - b) < atol

function close_vec(a, b)
    if length(a) != length(b)
        return false
    end
    ok = true
    for i in 1:length(a)
        ok = ok && close(a[i], b[i])
    end
    return ok
end

function continuous_fit_suffstats_7326()
    normal_data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
    dn = fit_mle(Normal, suffstats(Normal, normal_data))

    exp_data = [1.0, 2.0, 3.0, 4.0]
    de = fit_mle(Exponential, suffstats(Exponential, exp_data))

    gamma_data = [1.0, 2.0, 3.0, 4.0, 5.0]
    dg = fit_mle(Gamma, suffstats(Gamma, gamma_data))

    beta_data = [0.2, 0.3, 0.4, 0.5, 0.6]
    db_fit = fit(Beta, beta_data)
    db_mle = fit_mle(Beta, beta_data)

    dl = fit_mle(LogNormal, [1.0, exp(1.0), exp(2.0)])
    dw = fit_mle(Weibull, [1.0, 2.0, 3.0, 4.0, 5.0])
    dc = fit(Cauchy, [1.0, 2.0, 3.0, 4.0, 100.0])

    return close(mean(dn), 5.0) &&
           close(std(dn), 2.0) &&
           close(scale(de), 2.5) &&
           close_atol(shape(dg), 3.7016438100088096, 1e-5) &&
           close_atol(scale(dg), 0.8104507494449771, 1e-5) &&
           close_atol(db_fit.α, 3.44, 1e-5) &&
           close_atol(db_fit.β, 5.16, 1e-5) &&
           close_atol(db_mle.α, 4.573737861536411, 1e-5) &&
           close_atol(db_mle.β, 6.8772968782526815, 1e-5) &&
           close(params(dl)[1], 1.0) &&
           close(params(dl)[2], 1.0) &&
           close_atol(shape(dw), 2.293806670712317, 1e-5) &&
           close_atol(scale(dw), 3.394290718109487, 1e-5) &&
           close(location(dc), 3.0) &&
           close(scale(dc), 1.0)
end

function discrete_fit_suffstats_7326()
    bern = fit_mle(Bernoulli, suffstats(Bernoulli, [1, 0, 1, 1, 0]))
    binom = fit_mle(Binomial, 5, [1, 2, 3, 2])
    pois = fit(Poisson, [1, 2, 3, 2, 2])
    geom = fit_mle(Geometric, suffstats(Geometric, [0, 1, 2, 0, 2]))
    cat = fit(Categorical, [1, 2, 2, 3, 3, 3])
    cat4 = fit_mle(Categorical, suffstats(Categorical, 4, [1, 4, 4, 2]))

    return close(succprob(bern), 0.6) &&
           params(binom) == (5, 0.4) &&
           close(rate(pois), 2.0) &&
           close(succprob(geom), 0.5) &&
           close_vec(probs(cat), [1.0 / 6.0, 2.0 / 6.0, 3.0 / 6.0]) &&
           close_vec(probs(cat4), [0.25, 0.25, 0.0, 0.5])
end

ok = true
ok = ok && continuous_fit_suffstats_7326()
ok = ok && discrete_fit_suffstats_7326()

ok
