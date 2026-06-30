using Distributions
using Random

close(a, b; atol=1e-9) = abs(a - b) < atol

function all_in_interval(xs, lo, hi)
    ok = true
    for x in xs
        ok = ok && lo <= x <= hi
    end
    return ok
end

function central_truncated_normal_api_7325()
    d = Normal()
    td = truncated(d, -1.0, 1.0)
    norm = cdf(d, 1.0) - cdf(d, -1.0)
    return close(cdf(td, -1.0), 0.0; atol=1e-8) &&
           close(cdf(td, 1.0), 1.0; atol=1e-8) &&
           close(pdf(td, 0.0), pdf(d, 0.0) / norm; atol=1e-8) &&
           close(logpdf(td, 0.0), log(pdf(td, 0.0)); atol=1e-8) &&
           close(quantile(td, 0.5), 0.0; atol=1e-8) &&
           close(mean(td), 0.0; atol=1e-8) &&
           minimum(td) == -1.0 &&
           maximum(td) == 1.0 &&
           insupport(td, -1.0) &&
           insupport(td, 1.0) &&
           !insupport(td, -1.1) &&
           !insupport(td, 1.1)
end

function keyword_truncated_bounds_7325()
    lower = truncated(Normal(); lower=-1.0)
    upper = truncated(Normal(); upper=1.0)
    return close(cdf(lower, -1.0), 0.0; atol=1e-8) &&
           minimum(lower) == -1.0 &&
           close(cdf(upper, 1.0), 1.0; atol=1e-8) &&
           maximum(upper) == 1.0
end

function retruncated_bounds_7325()
    td = truncated(Normal(), -1.0, 1.0)
    inner = truncated(td, -0.5, 0.25)
    return minimum(inner) == -0.5 &&
           maximum(inner) == 0.25 &&
           close(cdf(inner, -0.5), 0.0; atol=1e-8) &&
           close(cdf(inner, 0.25), 1.0; atol=1e-8) &&
           close(quantile(inner, 0.0), -0.5; atol=1e-8) &&
           close(quantile(inner, 1.0), 0.25; atol=1e-8)
end

function truncated_sampling_7325()
    td = truncated(Normal(), -1.0, 1.0)
    xs = rand(td, 1000)
    ys = rand(Xoshiro(17), td, 16)
    scalar = rand(Xoshiro(17), td)
    return xs isa Vector &&
           ys isa Vector &&
           length(xs) == 1000 &&
           length(ys) == 16 &&
           all_in_interval(xs, -1.0, 1.0) &&
           all_in_interval(ys, -1.0, 1.0) &&
           -1.0 <= scalar <= 1.0
end

ok = true
ok = ok && central_truncated_normal_api_7325()
ok = ok && keyword_truncated_bounds_7325()
ok = ok && retruncated_bounds_7325()
ok = ok && truncated_sampling_7325()

ok
