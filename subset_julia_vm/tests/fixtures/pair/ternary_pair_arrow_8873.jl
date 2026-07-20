using Test

x = (first = 3, second = 4)
y = x.first > 2 ? x.first => 2 * x.second : x

@test y == (3 => 8)

true
