module Primes

include("primality.jl")
include("generation.jl")
include("factorization.jl")
include("arithmetic.jl")

export isprime, primes, primesmask, nextprime, prevprime, prime
export Factorization, factor, eachfactor, prodfactors
export radical, totient, divisors

end
