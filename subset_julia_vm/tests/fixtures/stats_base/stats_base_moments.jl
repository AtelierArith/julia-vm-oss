using StatsBase

x = [1, 2, 2, 3, 3, 3]
mode(x) == 3 && entropy([0.5, 0.5]) ≈ log(2)
