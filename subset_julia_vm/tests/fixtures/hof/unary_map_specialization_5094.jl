using Test

@testset "HOF unary map specialization (Issue #5094)" begin
    xs = [-3, 0, 4]
    identity_xs = map(identity, xs)
    @test identity_xs == xs
    @test identity_xs !== xs
    @test typeof(identity_xs) == Vector{Int64}

    abs_xs = map(abs, xs)
    @test abs_xs == [3, 0, 4]
    @test typeof(abs_xs) == Vector{Int64}

    abs2_xs = map(abs2, xs)
    @test abs2_xs == [9, 0, 16]
    @test typeof(abs2_xs) == Vector{Int64}

    neg_xs = map(-, xs)
    @test neg_xs == [3, 0, -4]
    @test typeof(neg_xs) == Vector{Int64}

    zero_xs = map(iszero, xs)
    @test zero_xs == Bool[false, true, false]
    @test typeof(zero_xs) == Vector{Bool}
    @test map(isone, [0, 1, 2]) == Bool[false, true, false]
    @test typeof(map(isone, [0, 1, 2])) == Vector{Bool}
    @test map(signbit, xs) == Bool[true, false, false]
    @test typeof(map(signbit, xs)) == Vector{Bool}
    @test map(iseven, xs) == Bool[false, true, true]
    @test typeof(map(iseven, xs)) == Vector{Bool}
    @test map(isodd, xs) == Bool[true, false, false]
    @test typeof(map(isodd, xs)) == Vector{Bool}

    i32s = Int32[-3, 0, 4]
    @test map(identity, i32s) == i32s
    @test typeof(map(identity, i32s)) == Vector{Int32}
    @test map(iszero, i32s) == Bool[false, true, false]
    @test typeof(map(iszero, i32s)) == Vector{Bool}
    @test map(isone, Int32[0, 1, 2]) == Bool[false, true, false]
    @test typeof(map(isone, Int32[0, 1, 2])) == Vector{Bool}
    @test map(signbit, i32s) == Bool[true, false, false]
    @test typeof(map(signbit, i32s)) == Vector{Bool}
    @test map(iseven, i32s) == Bool[false, true, true]
    @test typeof(map(iseven, i32s)) == Vector{Bool}
    @test map(isodd, i32s) == Bool[true, false, false]
    @test typeof(map(isodd, i32s)) == Vector{Bool}
    @test map(abs, i32s) == Int32[3, 0, 4]
    @test typeof(map(abs, i32s)) == Vector{Int32}
    @test map(abs2, i32s) == Int32[9, 0, 16]
    @test typeof(map(abs2, i32s)) == Vector{Int32}
    @test map(-, i32s) == Int32[3, 0, -4]
    @test typeof(map(-, i32s)) == Vector{Int32}

    u32s = UInt32[3, 0, 4]
    @test map(abs, u32s) == u32s
    @test typeof(map(abs, u32s)) == Vector{UInt32}
    @test map(abs2, u32s) == UInt32[9, 0, 16]
    @test typeof(map(abs2, u32s)) == Vector{UInt32}
    @test map(-, u32s) == UInt32[-UInt32(3), UInt32(0), -UInt32(4)]
    @test typeof(map(-, u32s)) == Vector{UInt32}
    @test map(iszero, u32s) == Bool[false, true, false]
    @test typeof(map(iszero, u32s)) == Vector{Bool}
    @test map(isone, UInt32[0, 1, 2]) == Bool[false, true, false]
    @test typeof(map(isone, UInt32[0, 1, 2])) == Vector{Bool}
    @test map(signbit, u32s) == Bool[false, false, false]
    @test typeof(map(signbit, u32s)) == Vector{Bool}
    @test map(iseven, u32s) == Bool[false, true, true]
    @test typeof(map(iseven, u32s)) == Vector{Bool}
    @test map(isodd, u32s) == Bool[true, false, false]
    @test typeof(map(isodd, u32s)) == Vector{Bool}

    fs = Float64[-1.5, 0.0, 2.5]
    abs_fs = map(abs, fs)
    @test abs_fs == [1.5, 0.0, 2.5]
    @test typeof(abs_fs) == Vector{Float64}

    abs2_fs = map(abs2, fs)
    @test abs2_fs == [2.25, 0.0, 6.25]
    @test typeof(abs2_fs) == Vector{Float64}

    bs = [true, false, true]
    identity_bs = map(identity, bs)
    @test identity_bs == bs
    @test identity_bs !== bs
    @test typeof(identity_bs) == Vector{Bool}

    iszero_bs = map(iszero, bs)
    @test iszero_bs == Bool[false, true, false]
    @test typeof(iszero_bs) == Vector{Bool}

    isone_bs = map(isone, bs)
    @test isone_bs == bs
    @test typeof(isone_bs) == Vector{Bool}

    signbit_bs = map(signbit, bs)
    @test signbit_bs == Bool[false, false, false]
    @test typeof(signbit_bs) == Vector{Bool}

    abs_bs = map(abs, bs)
    @test abs_bs == bs
    @test typeof(abs_bs) == Vector{Bool}

    abs2_bs = map(abs2, bs)
    @test abs2_bs == bs
    @test typeof(abs2_bs) == Vector{Bool}
end

true
