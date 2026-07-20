using Test

x = BigInt(1) .<< [1:3;]

@test x[1] == BigInt(2)
@test x[2] == BigInt(4)
@test x[3] == BigInt(8)
@test eltype(x) == BigInt

true
