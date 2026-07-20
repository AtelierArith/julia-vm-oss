module DataStructures

# SubsetJuliaVM DataStructures.jl MVP (Issue #8141).
#
# QuadGK.jl uses DataStructures only for the array-backed binary heap helpers
# in `src/heaps/arrays_as_heaps.jl`.  This package keeps the upstream module
# name, exports, and heap ordering surface while deferring the broader
# collection types until they are needed.

using Base.Order: Ordering, Forward, Reverse, lt

export AbstractHeap,
    BinaryMaxHeap,
    heapify!,
    heapify,
    heappop!,
    heappush!,
    isheap

abstract type AbstractHeap{VT} end

include("heaps/arrays_as_heaps.jl")

end
