using Distributions

close(a, b; atol=1e-9) = abs(a - b) < atol
closecomplex(z, re, im; atol=1e-9) =
    close(real(z), re; atol=atol) && close(imag(z), im; atol=atol)

function bounds_api_7324()
    return isbounded(Uniform()) &&
           !isbounded(Normal()) &&
           islowerbounded(Exponential()) &&
           !isupperbounded(Poisson()) &&
           isbounded(Bernoulli()) &&
           isbounded(Categorical([0.2, 0.3, 0.5]))
end

function tail_quantile_api_7324()
    n = Normal()
    e = Exponential(2.0)
    g = Geometric(0.4)
    return close(cquantile(n, 0.025), quantile(n, 0.975); atol=1e-8) &&
           close(invlogcdf(e, log(0.25)), quantile(e, 0.25); atol=1e-9) &&
           invlogccdf(g, log(0.2)) == cquantile(g, 0.2)
end

function modes_api_7324()
    mb = modes(Bernoulli(0.5))
    mp = modes(Poisson(3.0))
    md = modes(DiscreteUniform(1, 3))
    mc = modes(Categorical([0.25, 0.375, 0.375]))
    return length(modes(Uniform())) == 0 &&
           length(mb) == 2 && mb[1] == 0 && mb[2] == 1 &&
           length(mp) == 2 && mp[1] == 2 && mp[2] == 3 &&
           length(md) == 1 && md[1] == 1:3 &&
           length(mc) == 2 && mc[1] == 2 && mc[2] == 3
end

function shape_statistics_api_7324()
    return skewness(Normal()) == 0.0 &&
           kurtosis(Normal()) == 0.0 &&
           kurtosis(Normal(), false) == 3.0 &&
           skewness(Exponential()) == 2.0 &&
           kurtosis(Exponential()) == 6.0 &&
           kurtosis(Uniform()) == -1.2 &&
           close(skewness(Beta(2.0, 5.0)), 0.5962847939999439) &&
           close(kurtosis(Beta(2.0, 5.0)), -0.12) &&
           isnan(skewness(Cauchy())) &&
           isnan(kurtosis(Cauchy())) &&
           close(skewness(Weibull(1.5, 2.0)), 1.0719865728909586) &&
           close(kurtosis(Weibull(1.5, 2.0)), 1.3904035615957824) &&
           close(skewness(Categorical([0.2, 0.5, 0.3])), -0.1399416909620978) &&
           close(kurtosis(Categorical([0.2, 0.5, 0.3])), -0.9604331528529779) &&
           close(kurtosis(DiscreteUniform(1, 3)), -1.5)
end

function generating_functions_api_7324()
    zn = cf(Normal(), 0.5)
    zu = cf(DiscreteUniform(1, 3), 0.4)
    return close(mgf(Normal(1.0, 2.0), 0.1), 1.1274968515793757) &&
           closecomplex(zn, 0.8824969025845955, 0.0) &&
           close(mgf(Binomial(4, 0.25), 0.2), 1.2404726487777558) &&
           closecomplex(zu, -0.9473739960019236, 1.1689553130973835)
end

function likelihood_and_kl_api_7324()
    d = Normal()
    x = [0.0, 1.0, -1.0]
    expected = logpdf(d, x[1]) + logpdf(d, x[2]) + logpdf(d, x[3])
    return close(loglikelihood(d, x), expected) &&
           close(kldivergence(Normal(1.0, 2.0), Normal(0.5, 3.0)), 0.14157621921927552) &&
           close(kldivergence(LogNormal(0.1, 0.7), LogNormal(0.2, 1.1)), 0.1585966939909912) &&
           close(kldivergence(Exponential(1.0), Exponential(2.0)), 0.3068528194400547) &&
           close(kldivergence(Poisson(2.0), Poisson(3.0)), 0.18906978378367112) &&
           close(kldivergence(Geometric(0.4), Geometric(0.6)), 0.2027325540540822) &&
           close(kldivergence(Binomial(4, 0.3), Binomial(4, 0.6)), 0.7351475895472486)
end

ok = true
ok = ok && bounds_api_7324()
ok = ok && tail_quantile_api_7324()
ok = ok && modes_api_7324()
ok = ok && shape_statistics_api_7324()
ok = ok && generating_functions_api_7324()
ok = ok && likelihood_and_kl_api_7324()

ok
