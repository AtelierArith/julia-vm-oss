# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Random - Random number generation utilities
# =============================================================================
# Based on Julia's stdlib/Random
#
# This module provides random number generation utilities.
# The core RNG functions (rand, randn) are built-in and always available.
# This module provides additional utilities like seed!.
#
# Note: seed! is implemented as a VM builtin instruction (SeedGlobalRng)
# and is handled specially by the compiler.

module Random

export seed!, shuffle!, shuffle, randperm!, randperm, randcycle!, randcycle, randsubseq!, randsubseq

# seed!(n) - Reseed the global random number generator
# This function is implemented as a builtin and handled by compile_module_call.
# The declaration here is for documentation and module structure purposes.
#
# Usage:
#   Random.seed!(42)      # Set seed to 42
#   a = rand()            # Generate random number
#   Random.seed!(42)      # Reset to same seed
#   b = rand()            # a == b (same sequence)

# =============================================================================
# Internal: random integer in [1, n]
# =============================================================================
# Since rand(1:n) is not available as a builtin, we use rand() (Float64 in [0,1))
# and convert to integer range.
function _rand_int(n)
    return Int64(floor(rand() * n)) + 1
end

# =============================================================================
# shuffle! - In-place Fisher-Yates shuffle
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:227-235
#
# Randomly permute the elements of array `a` in-place using the
# Fisher-Yates (Knuth) shuffle algorithm.

function shuffle!(a)
    n = length(a)
    for i in 2:n
        j = _rand_int(i)
        temp = a[i]
        a[i] = a[j]
        a[j] = temp
    end
    return a
end

# =============================================================================
# shuffle - Return a randomly permuted copy
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:279-280

function shuffle(a)
    return shuffle!(copy(a))
end

# =============================================================================
# randperm! - Fill array with random permutation of 1:length(a)
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:339-353
#
# Inside-out Fisher-Yates algorithm: generates a random permutation
# by building it element by element.

function randperm!(a)
    n = length(a)
    if n == 0
        return a
    end
    a[1] = 1
    for i in 2:n
        j = _rand_int(i)
        if i != j
            a[i] = a[j]
        end
        a[j] = i
    end
    return a
end

# =============================================================================
# randperm - Generate a random permutation of 1:n
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:309-311

function randperm(n)
    a = zeros(Int64, n)
    return randperm!(a)
end

# =============================================================================
# randcycle! - In-place random cyclic permutation (Sattolo's algorithm)
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:420-433
#
# Generates a random cyclic permutation of length n. A cyclic permutation
# has exactly one cycle containing all elements (no fixed points for n > 1).
# Uses Sattolo's algorithm: like Fisher-Yates but j is chosen from 1:i-1
# instead of 1:i, guaranteeing a single cycle.

function randcycle!(a)
    n = length(a)
    if n == 0
        return a
    end
    a[1] = 1
    for i in 2:n
        j = _rand_int(i - 1)
        a[i] = a[j]
        a[j] = i
    end
    return a
end

# =============================================================================
# randcycle - Generate a random cyclic permutation of 1:n
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:389-391

function randcycle(n)
    a = zeros(Int64, n)
    return randcycle!(a)
end

# =============================================================================
# randsubseq! - In-place random subsequence (Bernoulli sampling)
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:98-125
#
# Each element of A is included in S independently with probability p.
# S is emptied first, then filled with the selected elements.

function randsubseq!(S, A, p)
    empty!(S)
    if p <= 0.0
        return S
    end
    if p >= 1.0
        for i in 1:length(A)
            push!(S, A[i])
        end
        return S
    end
    for i in 1:length(A)
        if rand() <= p
            push!(S, A[i])
        end
    end
    return S
end

# =============================================================================
# randsubseq - Return a random subsequence
# =============================================================================
# Based on Julia's stdlib/Random/src/misc.jl:142-143

function randsubseq(A, p)
    S = []
    return randsubseq!(S, A, p)
end

end # module Random
