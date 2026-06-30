using Test

function combine4_4019(a, b, c, d)
    a + b + c + d
end

A = [1, 2]
B = [10, 20]
C = [100, 200]
D = [1000, 2000]

@test broadcast(combine4_4019, A, B, C, D) == [1111, 2222]

dest = [0, 0]
result = broadcast!(combine4_4019, dest, A, B, C, D)

@test result === dest
@test dest == [1111, 2222]

true
