using Distributions
using Random

tol = 1e-6

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

function negativebinomial_api_7330()
    d = NegativeBinomial(5.0, 0.4)
    xs = rand(Xoshiro(123), d, 2000)
    return params(d) == (5.0, 0.4) &&
           close(succprob(d), 0.4) &&
           close(failprob(d), 0.6) &&
           close(mean(d), 7.5) &&
           close(var(d), 18.749999999999996) &&
           mode(d) == 5 &&
           minimum(d) == 0 &&
           maximum(d) == Inf &&
           close(pdf(d, 3), 0.07741440000000026) &&
           close(logpdf(d, 3), -2.5585824691793304) &&
           close(cdf(d, 3), 0.17367040000000009) &&
           quantile(d, 0.5) == 7 &&
           samples_in_support(d, xs) &&
           close_atol(sample_mean(xs), mean(d), 0.55)
end

function hypergeometric_api_7330()
    d = Hypergeometric(30, 20, 10)
    xs = rand(Xoshiro(123), d, 2000)
    a = rand(Xoshiro(321), d, 2, 3)
    return params(d) == (30, 20, 10) &&
           close(mean(d), 6.0) &&
           close(var(d), 1.9591836734693882) &&
           mode(d) == 6 &&
           minimum(d) == 0 &&
           maximum(d) == 10 &&
           support(d) == 0:10 &&
           close(pdf(d, 6), 0.2800586031053713) &&
           close(logpdf(d, 6), -1.2727564009075107) &&
           close(cdf(d, 6), 0.6350317132231632) &&
           quantile(d, 0.5) == 6 &&
           samples_in_support(d, xs) &&
           close_atol(sample_mean(xs), mean(d), 0.16) &&
           size(a) == (2, 3) &&
           samples_in_support(d, a)
end

function betabinomial_api_7330()
    d = BetaBinomial(12, 2.0, 5.0)
    xs = rand(Xoshiro(123), d, 2000)
    return params(d) == (12, 2.0, 5.0) &&
           ntrials(d) == 12 &&
           close(mean(d), 3.4285714285714284) &&
           close(var(d), 5.816326530612245) &&
           mode(d) == 2 &&
           minimum(d) == 0 &&
           maximum(d) == 12 &&
           support(d) == 0:12 &&
           close(pdf(d, 3), 0.15406162464986023) &&
           close(logpdf(d, 3), -1.8704025965471662) &&
           close(cdf(d, 3), 0.5609243697478992) &&
           quantile(d, 0.5) == 3 &&
           samples_in_support(d, xs) &&
           close_atol(sample_mean(xs), mean(d), 0.25)
end

ok = true
ok = ok && negativebinomial_api_7330()
ok = ok && hypergeometric_api_7330()
ok = ok && betabinomial_api_7330()

ok
