using Test

a = Array{Int64}(undef, ())
a[1] = 42
@test getindex(a) == 42
@test a[] == 42
@test a[1] == 42

t = Array{Tuple{}}(undef, ())
t[1] = ()
@test getindex(t) == ()
@test t[] == ()
@test t[1] == ()

empty_ints = Int64[]
@test typeof(empty_ints) === Vector{Int64}
@test length(empty_ints) == 0

true
