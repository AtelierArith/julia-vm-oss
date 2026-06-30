using Distributions
using Random

# Seeded global-RNG empirical-mean sanity for discrete distributions.
#
# N and the tolerances are scaled together to keep the test fast (Issue #7589):
# the standard error shrinks only as 1/sqrt(N), so cutting N by 4x doubles the
# SE. The tolerances below are widened by the same factor to preserve each
# check's original standard-error margin.
const N = 5000

function mean_bernoulli()
    Random.seed!(20240620)
    d = Bernoulli(0.3)
    s = 0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_binomial()
    Random.seed!(20240620)
    d = Binomial(10, 0.3)
    s = 0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_poisson()
    Random.seed!(20240620)
    d = Poisson(4.0)
    s = 0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_geometric()
    Random.seed!(20240620)
    d = Geometric(0.25)
    s = 0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

function mean_discreteuniform()
    Random.seed!(20240620)
    d = DiscreteUniform(1, 6)
    s = 0
    for _ in 1:N
        s += rand(d)
    end
    return s / N
end

ok = true
ok = ok && (abs(mean_bernoulli() - 0.3) < 0.04)
ok = ok && (abs(mean_binomial() - 3.0) < 0.16)
ok = ok && (abs(mean_poisson() - 4.0) < 0.2)
ok = ok && (abs(mean_geometric() - 3.0) < 0.4)
ok = ok && (abs(mean_discreteuniform() - 3.5) < 0.16)
# samples are integers
ok = ok && (rand(Poisson(4.0)) isa Int)
ok = ok && (rand(Bernoulli(0.5)) isa Int)

ok
