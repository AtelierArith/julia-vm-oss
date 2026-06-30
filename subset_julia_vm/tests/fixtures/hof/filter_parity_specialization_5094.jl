using Test

@testset "HOF filter parity specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 5]
    evens = filter(iseven, xs)
    @test evens == [0, 4]
    @test typeof(evens) == Vector{Int64}

    odds = filter(isodd, xs)
    @test odds == [-3, 5]
    @test typeof(odds) == Vector{Int64}

    i32s = Int32[-3, 0, 4, 5]
    even_i32s = filter(iseven, i32s)
    @test even_i32s == Int32[0, 4]
    @test typeof(even_i32s) == Vector{Int32}

    odd_i32s = filter(isodd, i32s)
    @test odd_i32s == Int32[-3, 5]
    @test typeof(odd_i32s) == Vector{Int32}

    u32s = UInt32[3, 0, 4, 5]
    even_u32s = filter(iseven, u32s)
    @test even_u32s == UInt32[0, 4]
    @test typeof(even_u32s) == Vector{UInt32}

    odd_u32s = filter(isodd, u32s)
    @test odd_u32s == UInt32[3, 5]
    @test typeof(odd_u32s) == Vector{UInt32}

    @test filter(iseven, Int32[1, 3, 5]) == Int32[]
    @test typeof(filter(iseven, Int32[1, 3, 5])) == Vector{Int32}
end

true
