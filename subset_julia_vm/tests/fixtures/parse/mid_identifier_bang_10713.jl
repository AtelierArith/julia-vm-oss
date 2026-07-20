using Test

foo!bar = 1
@test foo!bar == 1

is!valid! = 2
@test is!valid! == 2

name! = "B"
@test_throws UndefVarError eval(Meta.parse("\"$name!x\""))

name!x = "C"
@test "$name!x" == "C"

lhs!rhs = 3
@test lhs!rhs != 4
@test lhs!rhs !== nothing

true
