# Kept standalone: extends Base.similar with a method on a Base argument type
# (`similar(::Vector{Int64}, ::Int64)`), i.e. method piracy. Method-table
# extension is process-global, not module-scoped, so wrapping this in a module
# inside an aggregate would leak the pirated method to every later member that
# calls `similar` (e.g. `similar_basic`). Same #5966 class as the
# dispatch/*_user_method_* fixtures; excluded from Issue #10238 module-wrap
# aggregation.
using Test

import Base: similar

function similar(a::Vector{Int64}, n::Int64)
    out = Vector{Int64}(undef, n)
    fill!(out, 4018)
    return out
end

function similar_any_receiver_dispatch_4018(a, n)
    b = similar(a, n)
    return typeof(b) === Vector{Int64} && length(b) == n && b[1] == 4018 && b[n] == 4018
end

@test similar_any_receiver_dispatch_4018([1, 2, 3], 2)

true
