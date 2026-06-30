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

function chi_api_7329()
    d = Chi(4.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (4.0,) &&
           close(mean(d), 1.8799712059732505) &&
           close(var(d), 0.4657082647114823) &&
           close_atol(median(d), 1.8321282651695876, 1e-5) &&
           close(mode(d), 1.7320508075688772) &&
           close(entropy(d), 1.0192499070723267) &&
           minimum(d) == 0.0 &&
           maximum(d) == Inf &&
           close(pdf(d, 1.5), 0.5478510386672151) &&
           close(logpdf(d, 1.5), -0.6017518562354522) &&
           close(cdf(d, 1.5), 0.3101135068635068) &&
           close_atol(quantile(d, 0.5), 1.8321282651695876, 1e-5) &&
           samples_in_support(d, xs)
end

function erlang_api_7329()
    d = Erlang(3, 2.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (3, 2.0) &&
           shape(d) == 3 &&
           close(scale(d), 2.0) &&
           close(rate(d), 0.5) &&
           close(mean(d), 6.0) &&
           close(var(d), 12.0) &&
           close_atol(median(d), 5.34812062744712, 1e-5) &&
           close(mode(d), 4.0) &&
           close(entropy(d), 2.5407256909229563) &&
           close(pdf(d, 4.0), 0.1353352832366127) &&
           close(logpdf(d, 4.0), -2.0) &&
           close(cdf(d, 4.0), 0.3233235838169365) &&
           close_atol(quantile(d, 0.5), 5.34812062744712, 1e-5) &&
           samples_in_support(d, xs)
end

function inversegamma_api_7329()
    d = InverseGamma(4.0, 2.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (4.0, 2.0) &&
           close(shape(d), 4.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 0.6666666666666666) &&
           close(var(d), 0.2222222222222222) &&
           close_atol(median(d), 0.5446532987303828, 1e-5) &&
           close(mode(d), 0.4) &&
           close(entropy(d), 0.2043183076289975) &&
           close(pdf(d, 0.75), 0.7808071775270097) &&
           close(logpdf(d, 0.75), -0.24742705139603594) &&
           close(cdf(d, 0.75), 0.7214269441774827) &&
           close_atol(quantile(d, 0.5), 0.5446532987303828, 1e-5) &&
           samples_in_support(d, xs)
end

function inversegaussian_api_7329()
    d = InverseGaussian(1.5, 3.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (1.5, 3.0) &&
           close(shape(d), 3.0) &&
           close(mean(d), 1.5) &&
           close(var(d), 1.125) &&
           close_atol(median(d), 1.2065085619440028, 1e-5) &&
           close(mode(d), 0.75) &&
           close(pdf(d, 1.0), 0.5849089671682235) &&
           close(logpdf(d, 1.0), -0.5362990555372845) &&
           close(cdf(d, 1.0), 0.3881108178559103) &&
           close_atol(quantile(d, 0.5), 1.2065085619440028, 1e-5) &&
           samples_in_support(d, xs)
end

function arcsine_api_7329()
    d = Arcsine(0.0, 2.0)
    xs = rand(Xoshiro(123), d, 64)
    ms = modes(d)
    return params(d) == (0.0, 2.0) &&
           close(location(d), 0.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 1.0) &&
           close(var(d), 0.5) &&
           close(median(d), 1.0) &&
           close(mode(d), 0.0) &&
           ms[1] == 0.0 && ms[2] == 2.0 &&
           close(entropy(d), 0.4515827052894549) &&
           close(pdf(d, 0.5), 0.3675525969478614) &&
           close(logpdf(d, 0.5), -1.0008888496235098) &&
           close(cdf(d, 0.5), 0.33333333333333337) &&
           close(quantile(d, 0.75), 1.7071067811865475) &&
           samples_in_support(d, xs)
end

function triangular_api_7329()
    d = TriangularDist(0.0, 4.0, 1.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (0.0, 4.0, 1.0) &&
           close(mean(d), 1.6666666666666667) &&
           close(var(d), 0.7222222222222222) &&
           close(median(d), 1.5505102572168221) &&
           close(mode(d), 1.0) &&
           close(entropy(d), 1.1931471805599454) &&
           minimum(d) == 0.0 &&
           maximum(d) == 4.0 &&
           close(pdf(d, 1.0), 0.5) &&
           close(logpdf(d, 1.0), -0.6931471805599453) &&
           close(cdf(d, 1.0), 0.25) &&
           close(quantile(d, 0.75), 2.267949192431123) &&
           samples_in_support(d, xs)
end

function symtriangular_api_7329()
    d = SymTriangularDist(1.0, 2.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (1.0, 2.0) &&
           close(location(d), 1.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 1.0) &&
           close(var(d), 0.6666666666666666) &&
           close(median(d), 1.0) &&
           close(mode(d), 1.0) &&
           close(entropy(d), 1.1931471805599454) &&
           minimum(d) == -1.0 &&
           maximum(d) == 3.0 &&
           close(pdf(d, 1.5), 0.375) &&
           close(logpdf(d, 1.5), -0.9808292530117262) &&
           close(cdf(d, 1.5), 0.71875) &&
           close(quantile(d, 0.75), 1.5857864376269049) &&
           samples_in_support(d, xs)
end

function cosine_api_7329()
    d = Cosine(0.0, 2.0)
    xs = rand(Xoshiro(123), d, 16)
    return params(d) == (0.0, 2.0) &&
           close(location(d), 0.0) &&
           close(scale(d), 2.0) &&
           close(mean(d), 0.0) &&
           close(var(d), 0.5227638641946311) &&
           close(median(d), 0.0) &&
           close(mode(d), 0.0) &&
           minimum(d) == -2.0 &&
           maximum(d) == 2.0 &&
           close(pdf(d, 0.5), 0.42677669529663687) &&
           close(logpdf(d, 0.5), -0.8514943643803203) &&
           close(cdf(d, 0.5), 0.7375395395196382) &&
           close_atol(quantile(d, 0.6), 0.2016782480495749, 1e-5) &&
           samples_in_support(d, xs)
end

function semicircle_api_7329()
    d = Semicircle(2.0)
    xs = rand(Xoshiro(123), d, 64)
    return params(d) == (2.0,) &&
           close(mean(d), 0.0) &&
           close(var(d), 1.0) &&
           close(median(d), 0.0) &&
           close(mode(d), 0.0) &&
           close(entropy(d), 1.3378770664093453) &&
           minimum(d) == -2.0 &&
           maximum(d) == 2.0 &&
           close(pdf(d, 1.0), 0.27566444771089604) &&
           close(logpdf(d, 1.0), -1.2885709220752903) &&
           close(cdf(d, 1.0), 0.8044988905221147) &&
           close_atol(quantile(d, 0.75), 0.8079455065990343, 1e-5) &&
           samples_in_support(d, xs)
end

function kumaraswamy_api_7329()
    d = Kumaraswamy(2.0, 3.0)
    xs = rand(Xoshiro(123), d, 64)
    a = rand(Xoshiro(321), d, 2, 3)
    return params(d) == (2.0, 3.0) &&
           close(mean(d), 0.457142857142857) &&
           close(var(d), 0.04102040816326541) &&
           close(median(d), 0.45420201894740647) &&
           close(mode(d), 0.4472135954999579) &&
           close(entropy(d), -0.2084261358947216) &&
           minimum(d) == 0.0 &&
           maximum(d) == 1.0 &&
           close(pdf(d, 0.5), 1.6874999999999998) &&
           close(logpdf(d, 0.5), 0.5232481437645478) &&
           close(cdf(d, 0.5), 0.578125) &&
           close(quantile(d, 0.75), 0.6083087004577227) &&
           samples_in_support(d, xs) &&
           size(a) == (2, 3) &&
           samples_in_support(d, a)
end

ok = true
ok = ok && chi_api_7329()
ok = ok && erlang_api_7329()
ok = ok && inversegamma_api_7329()
ok = ok && inversegaussian_api_7329()
ok = ok && arcsine_api_7329()
ok = ok && triangular_api_7329()
ok = ok && symtriangular_api_7329()
ok = ok && cosine_api_7329()
ok = ok && semicircle_api_7329()
ok = ok && kumaraswamy_api_7329()

ok
