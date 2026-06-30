using Base.Iterators

import Base: collect

collect(e::Base.Enumerate{Any}) = :enumerate_dispatch
collect(e::Base.Iterators.Enumerate{Vector{Int64}}) = :enumerate_dispatch
collect(r::Base.Iterators.Rest{Any, Any}) = :rest_dispatch
collect(r::Base.Iterators.Rest{Vector{Int64}, Int64}) = :rest_dispatch

runtime_collect_4052(x::Any) = collect(x)

values = collect(enumerate([1, 2]))
@assert values === :enumerate_dispatch

runtime_values = runtime_collect_4052(enumerate([1, 2]))
@assert runtime_values === :enumerate_dispatch

arr = [10, 20, 30]
state = iterate(arr)[2]
rest_values = collect(rest(arr, state))
@assert rest_values === :rest_dispatch

runtime_rest_values = runtime_collect_4052(rest(arr, state))
@assert runtime_rest_values === :rest_dispatch

true
