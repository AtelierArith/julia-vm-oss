# Issue #10310: the Euclidean-modulo special case (`EuclideanModuloI64Function`
# / `EuclideanModuloI64LoopBlock`) hard-coded exactly the coprime-gcd loop shape
# below and was retired in favor of the general typed-loop path. `TypedLoopOp`
# (frame-based loop execution) and the frame-less `I64FunctionBlock` call path
# already cover `ModI64`/`LoadModI64Slot`/slot loads-stores, so this loop is now
# recognized and executed generically instead of through a per-kernel
# recognizer. This fixture is the correctness oracle for that general path: the
# results must match upstream Julia exactly, including the edge case where the
# loop body never runs (`b == 0` on entry).
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

# calc_pi-style consumer (benchmarks/calc_pi_n*.jl): the loop result feeds a
# comparison inside a nested loop, exercising the same call/branch shape the
# coprime-pi benchmark relies on.
function count_coprime_pairs(n::Int64)
    cnt = 0
    for a in 1:n
        for b in 1:n
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    cnt
end

println(mygcd(48, 18) == 6)
println(mygcd(1071, 462) == 21)
println(mygcd(17, 5) == 1)
println(mygcd(100, 100) == 100)
# Loop body never runs (b == 0 on entry): returns a unchanged.
println(mygcd(7, 0) == 7)
# a == 0 on entry: one iteration, (a, b) = (b, 0 % b) = (b, 0), returns b.
println(mygcd(0, 7) == 7)
println(count_coprime_pairs(10) == 63)
"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
