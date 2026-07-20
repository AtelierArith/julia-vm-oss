using Test

f_8825(x) = x + 1
@test f_8825(1) == 2

eval(:(f_8825(x) = x + 100))
@test f_8825(1) == 101

eval(:(myf_8825(x) = x + 1))
@test myf_8825(2) == 3

true
