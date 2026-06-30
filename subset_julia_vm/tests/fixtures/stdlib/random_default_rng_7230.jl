using Random
using Test

# Issue #7230: Random.default_rng() and GLOBAL_RNG return the VM's global RNG.
# rand(default_rng()) / randn(default_rng()) must advance the SAME stream as
# bare rand() / randn() (verified against upstream julia 1.12.6).

function default_rng_returns_rng_7230()
    g = Random.default_rng()
    x = rand(g)
    return x isa Float64 && 0.0 <= x < 1.0
end

# After seed!, the first draw via default_rng() equals the first bare draw.
function default_rng_shares_global_stream_7230()
    Random.seed!(42)
    a = rand()
    Random.seed!(42)
    c = rand(Random.default_rng())
    return a == c
end

# default_rng() and bare rand() interleave on the same stream.
function default_rng_interleaves_7230()
    Random.seed!(7)
    a = rand()
    b = rand(Random.default_rng())
    Random.seed!(7)
    c = rand(Random.default_rng())
    d = rand()
    return a == c && b == d
end

# randn through default_rng() advances the same normal stream as bare randn().
function default_rng_randn_shares_stream_7230()
    Random.seed!(99)
    a = randn()
    Random.seed!(99)
    c = randn(Random.default_rng())
    return a == c
end

# GLOBAL_RNG is an alias of default_rng().
function global_rng_alias_7230()
    Random.seed!(123)
    a = rand()
    Random.seed!(123)
    c = rand(Random.GLOBAL_RNG)
    return a == c
end

# default_rng() is an AbstractRNG.
function default_rng_is_abstractrng_7230()
    return Random.default_rng() isa AbstractRNG
end

# typeof(default_rng()) is TaskLocalRNG (matches upstream).
function default_rng_typeof_7230()
    return typeof(Random.default_rng()) === typeof(Random.GLOBAL_RNG)
end

@testset "Random.default_rng()/GLOBAL_RNG (Issue #7230)" begin
    @test default_rng_returns_rng_7230()
    @test default_rng_shares_global_stream_7230()
    @test default_rng_interleaves_7230()
    @test default_rng_randn_shares_stream_7230()
    @test global_rng_alias_7230()
    @test default_rng_is_abstractrng_7230()
    @test default_rng_typeof_7230()
end

true
