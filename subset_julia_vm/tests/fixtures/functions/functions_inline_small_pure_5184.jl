using Test

addone(x) = x + 1
twice(x) = x + x

result = twice(addone(20))

@test result == 42

true
