using Test

z = Int8(100)im

@test real(z) == 0
@test imag(z) == 100
@test string(z) == "0 + 100im"

true
