# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Broadcast - Standard library module for broadcasting operations
# =============================================================================
# Based on Julia's base/broadcast.jl
#
# In Julia, the Broadcast module provides the infrastructure for broadcasting
# operations (element-wise operations that extend scalars and arrays).
#
# SubsetJuliaVM implements broadcasting at the VM level with:
# - Dot operators: .+, .-, .*, ./, .^, .<, .>, .<=, .>=, .==, .!=, .&, .|
# - Dot function calls: sqrt.([1,4,9]), sin.([0, pi/2])
# - broadcast(f, A): Apply function f element-wise to array A
# - broadcast(f, A, B): Apply binary function with shape broadcasting
# - broadcast!(f, dest, A, B): In-place broadcast
#
# Supported patterns:
#   [1,2,3] .+ [4,5,6]       # Array .op Array
#   [1,2,3] .* 2             # Array .op Scalar
#   2 .- [1,2,3]             # Scalar .op Array
#   (1,2,3) .+ (4,5,6)       # Tuple .op Tuple
#   sqrt.([1,4,9])           # Function.(Array)
#   broadcast(+, A, B)       # Function form
#   f.([1,2], Ref(10))       # With Ref() for constants
#
# Pure Julia migration status (broadcast-pure-julia milestone):
#   Phase 5: flatten/isflat, AndAnd/OrOr — implemented in base/broadcast.jl
#   Phase 6: dot syntax lowering, @. macro — blocked on Phase 0-4
#   Phase 7: VM deprecation, regression tests, docs — partially complete
#
# See also: subset_julia_vm/src/julia/base/broadcast.jl for Pure Julia code

module Broadcast

# Export the broadcast function for explicit use
# Note: Dot operators (.+, .*, etc.) are handled by the VM directly
# and don't require importing this module.
export broadcast, broadcast!

# Note: The actual implementations are currently at the VM level.
# This module provides namespace compatibility with Julia's Broadcast module.
#
# In full Julia, Broadcast is a baremodule with complex style-based dispatch.
# In SubsetJuliaVM, broadcasting is currently implemented as VM instructions:
# - BroadcastBinOp: Element-wise binary operations
# - BroadcastUnaryOp: Element-wise unary operations
# - BroadcastUserFunc: User-defined function broadcast
# - BroadcastFunc2: Binary function broadcast with shape broadcasting
# - BroadcastFunc2InPlace: In-place variant
#
# These will be progressively replaced by Pure Julia implementations
# as the broadcast-pure-julia milestone progresses.

end # module Broadcast
