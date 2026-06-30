using Base.Iterators

import Base: collect

collect(e::Base.Iterators.Enumerate{Vector{Int64}}) = :enumerate_dispatch
collect(r::Base.Iterators.Rest{Vector{Int64}, Int64}) = :rest_dispatch
collect(z::Base.Iterators.Zip) = :zip_dispatch

runtime_collect_3910(x::Any) = collect(x)

@assert runtime_collect_3910(enumerate([1, 2])) === :enumerate_dispatch

arr = [10, 20, 30]
state = iterate(arr)[2]
@assert runtime_collect_3910(rest(arr, state)) === :rest_dispatch

@assert runtime_collect_3910(zip([1, 2], [3, 4])) === :zip_dispatch

true
