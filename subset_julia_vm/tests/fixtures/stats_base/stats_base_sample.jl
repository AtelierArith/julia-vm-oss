using StatsBase
using Random

Random.seed!(123)
a = [10, 20, 30, 40]
s = sample(a, 4; replace=false)
length(s) == 4 && sort(s) == sort(a)
