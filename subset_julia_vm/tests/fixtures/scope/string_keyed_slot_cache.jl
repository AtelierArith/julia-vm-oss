# Issue #5179: string-keyed Load*/Store* locals must resolve to the same slot
# whether the name->slot lookup goes through the pre-computed map (fast path) or
# the legacy linear scan over slot_names. Exercise String, Int64, Float64 and
# Bool locals that are repeatedly stored and loaded inside a function body so the
# name->slot resolution path runs many times.

using Test

function string_local_roundtrip(n)
    s = "start"
    acc = 0
    for i in 1:n
        s = string(s, "-", i)
        acc += length(s)
    end
    return (s, acc)
end

function typed_locals(n)
    total = 0          # Int64 local
    weight = 1.0       # Float64 local
    flag = true        # Bool local
    for i in 1:n
        total += i
        weight = weight + Float64(i)
        flag = !flag
    end
    return (total, weight, flag)
end

@testset "Issue #5179 string-keyed slot cache parity" begin
    s, acc = string_local_roundtrip(3)
    @test s == "start-1-2-3"
    @test acc == 7 + 9 + 11

    total, weight, flag = typed_locals(4)
    @test total == 1 + 2 + 3 + 4
    @test weight == 1.0 + 1.0 + 2.0 + 3.0 + 4.0
    # flag starts true and is toggled 4 times, so it returns to true.
    @test flag == true
end

true
