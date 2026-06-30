using Distributions
using Random

tol = 1e-5

close(a, b) = abs(a - b) < tol
close_atol(a, b, atol) = abs(a - b) < atol

function samples_in_support(d, xs)
    lo = minimum(d)
    hi = maximum(d)
    for x in xs
        if !(lo <= x <= hi)
            return false
        end
    end
    return true
end

function sample_mean(xs)
    s = 0.0
    for x in xs
        s += x
    end
    return s / length(xs)
end

function skellam_api_7331()
    d = Skellam(4.0, 1.5)
    xs = rand(Xoshiro(123), d, 3000)
    a = rand(Xoshiro(321), d, 2, 3)
    return params(d) == (4.0, 1.5) &&
           close(mean(d), 2.5) &&
           close(var(d), 5.5) &&
           minimum(d) == -Inf &&
           maximum(d) == Inf &&
           close(pdf(d, -2), 0.02427201604814087) &&
           close(logpdf(d, -2), -3.718432597776431) &&
           close(cdf(d, -2), 0.03638519571637263) &&
           close(pdf(d, 3), 0.1645649554726972) &&
           close(cdf(d, 3), 0.6787966150124363) &&
           samples_in_support(d, xs) &&
           close_atol(sample_mean(xs), mean(d), 0.20) &&
           size(a) == (2, 3) &&
           samples_in_support(d, a)
end

function dirac_api_7331()
    d = Dirac(3)
    xs = rand(Xoshiro(123), d, 20)
    return mean(d) == 3 &&
           median(d) == 3 &&
           var(d) == 0 &&
           mode(d) == 3 &&
           minimum(d) == 3 &&
           maximum(d) == 3 &&
           support(d) == (3,) &&
           pdf(d, 3) == 1.0 &&
           pdf(d, 2) == 0.0 &&
           logpdf(d, 3) == 0.0 &&
           logpdf(d, 2) == -Inf &&
           cdf(d, 2) == 0.0 &&
           cdf(d, 3) == 1.0 &&
           quantile(d, 0.25) == 3 &&
           sample_mean(xs) == 3.0
end

function poissonbinomial_api_7331()
    d = PoissonBinomial([0.2, 0.5, 0.8])
    xs = rand(Xoshiro(123), d, 3000)
    a = rand(Xoshiro(321), d, 2, 3)
    # Workaround: tuple equality with equal Vector elements returns false in
    # sjulia, so compare the vector parameter directly instead. (Issue #7803)
    return params(d)[1] == [0.2, 0.5, 0.8] &&
           succprob(d) == [0.2, 0.5, 0.8] &&
           failprob(d) == [0.8, 0.5, 0.19999999999999996] &&
           ntrials(d) == 3 &&
           close(mean(d), 1.5) &&
           close(var(d), 0.5700000000000001) &&
           mode(d) == 1 &&
           modes(d) == [2, 1] &&
           minimum(d) == 0 &&
           maximum(d) == 3 &&
           support(d) == 0:3 &&
           close(pdf(d, 0), 0.07999999999999999) &&
           close(pdf(d, 1), 0.42000000000000004) &&
           close(pdf(d, 2), 0.42000000000000004) &&
           close(pdf(d, 3), 0.08000000000000002) &&
           close(logpdf(d, 1), -0.8675005677047231) &&
           close(cdf(d, 1), 0.5) &&
           close(cdf(d, 2), 0.9199999999999999) &&
           quantile(d, 0.5) == 1 &&
           samples_in_support(d, xs) &&
           close_atol(sample_mean(xs), mean(d), 0.08) &&
           size(a) == (2, 3) &&
           samples_in_support(d, a)
end

ok = true
ok = ok && skellam_api_7331()
ok = ok && dirac_api_7331()
ok = ok && poissonbinomial_api_7331()

ok
