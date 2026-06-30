# Issue #6745: the non-mutating reducers/finders
# collect / findfirst / findall / argmin / argmax / prod / minimum / maximum
# (and array iteration) are pure Julia (base/array.jl). The vestigial Rust
# BuiltinId variants (Prod/Minimum/Maximum/Argmin/Argmax/FindFirst/FindAll)
# were dead (never emitted) and removed; this pins that the pure-Julia dispatch
# keeps matching upstream julia 1.12 across element types.

using Test

@testset "reducers match upstream (Issue #6745)" begin
    a = [3, 1, 2, 5, 4]
    @test prod(a) == 120
    @test minimum(a) == 1
    @test maximum(a) == 5
    @test argmin(a) == 2
    @test argmax(a) == 4

    # narrow integer reductions promote to Int (matching upstream)
    @test prod(Int8[2, 3, 4]) === 24
    @test prod(Int[]) === 1
    @test minimum([2.5, 1.5, 3.5]) === 1.5
    @test maximum(Float32[1, 9, 4]) === 9.0f0
    @test argmin([3.0, 1.0, 2.0]) == 2
    @test maximum(["a", "c", "b"]) == "c"
end

@testset "finders / collect / iterate (Issue #6745)" begin
    a = [3, 1, 2, 5, 4]
    @test findfirst(==(2), a) == 3
    @test findfirst(>(10), a) === nothing
    @test findall(>(2), a) == [1, 4, 5]
    @test collect(1:3) == [1, 2, 3]
    @test collect(x^2 for x in 1:4) == [1, 4, 9, 16]

    # array iteration (for-loop / comprehension) drives the same path
    s = 0
    for x in a
        s += x
    end
    @test s == 15

    # first-class function values keep resolving after the BuiltinId removal
    f = argmax
    @test f([10, 40, 20]) == 2
    @test map(minimum, [[3, 1], [9, 2, 7]]) == [1, 2]
end

true
