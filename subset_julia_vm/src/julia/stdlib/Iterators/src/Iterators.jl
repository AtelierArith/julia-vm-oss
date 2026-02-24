# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Iterators - Standard library module for iterator utilities
# =============================================================================
# Based on Julia's base/iterators.jl
#
# In Julia, the Iterators module provides convenience functions for working
# with iterators. Many of these functions are already implemented in Base
# and are re-exported here for compatibility.
#
# Supported iterator types and functions:
#   enumerate(iter)     - yield (index, element) pairs
#   zip(a, b)           - parallel iteration over two collections
#   take(iter, n)       - first n elements
#   drop(iter, n)       - skip first n elements
#   takewhile(f, iter)  - take while predicate holds
#   dropwhile(f, iter)  - drop while predicate holds
#   cycle(iter)         - infinite cyclic repetition
#   repeated(x)         - infinite repetition of x
#   repeated(x, n)      - n repetitions of x
#   rest(iter, state)   - remaining elements after state
#   flatten(iter)       - flatten nested iterators
#   flatmap(f, iter)    - map then flatten (Issue #2115)
#   partition(iter, n)  - group into chunks of n
#   product(a, b)       - Cartesian product
#   countfrom()         - infinite counter starting at 1
#   countfrom(start)    - infinite counter starting at start
#   countfrom(start, step) - infinite counter with custom step
#   peel(iter)          - split into first element and rest
#   nth(iter, n)        - get nth element of iterator
#
# Higher-order function iterators:
#   filter(f, iter)      - filter elements by predicate
#   map(f, iter)         - transform elements
#   accumulate(op, iter) - cumulative reduction
#   reverse(iter)        - reverse iteration (Issue #2159)
#
# NOT yet implemented:
#   Stateful            - stateful iterator wrapper
#   findeach(pattern, str) - find all occurrences

module Iterators

# Export iterator functions that are available in Base
# These are re-exported for API compatibility with Julia's Iterators module
export enumerate, zip, rest, countfrom, take, drop, takewhile, dropwhile,
       cycle, repeated, product, flatten, flatmap, partition, peel, nth,
       filter, map, reverse, accumulate

# Note: The actual implementations are in Base (base/iteration.jl).
# This module re-exports them for Julia compatibility.
# Users can either `using Iterators` or use the functions directly from Base.

# Re-export from Base
# In SubsetJuliaVM, these functions are already globally available,
# so we just need to provide the module namespace for compatibility.

# Note: In full Julia, Iterators is a baremodule that imports from Base
# and provides additional wrapper types. In SubsetJuliaVM, all the
# iterator functionality is implemented in base/iteration.jl.

end # module Iterators
