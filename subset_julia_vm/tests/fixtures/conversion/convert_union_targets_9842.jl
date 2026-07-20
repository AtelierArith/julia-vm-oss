# convert(::Type{Union{...}}, x) follows upstream identity and member conversion.

using Test

struct MaybeFloat9842
    x::Union{Nothing, Float64}
end

struct UnionConvertA9842
    x::Int
end

struct UnionConvertB9842
    x::Int
end

Base.convert(::Type{Union{UnionConvertA9842,UnionConvertB9842}}, x::Int) = UnionConvertA9842(x)
Base.convert(::Type{Union{Int64,Float64}}, x::Int64) = Float64(x + 1)

@testset "convert to Union targets" begin
    @test convert(Union{Nothing, Float64}, 1.0) === 1.0
    @test convert(Union{Nothing, Float64}, nothing) === nothing
    @test convert(Union{Missing, Int64}, 3) === 3
    @test convert(Union{Float32, Int64}, 3) === 3
    @test convert(Union{Float64, Int64}, 3) === 4.0
    @test typeof(convert(Union{Float64, Int64}, 3)) === Float64

    widened = convert(Union{Nothing, Float64}, 1)
    @test widened === 1.0
    @test typeof(widened) === Float64

    missing_widened = convert(Union{Missing, Float64}, 3)
    @test missing_widened === 3.0
    @test typeof(missing_widened) === Float64

    @test_throws InexactError convert(Union{Nothing, Int64}, 1.5)

    # Union identity is handled before dispatch, but non-identity conversion
    # must still honor an exact user-defined method before structural fallback.
    user_converted = convert(Union{UnionConvertA9842,UnionConvertB9842}, 7)
    @test user_converted == UnionConvertA9842(7)
    @test Union{UnionConvertA9842,UnionConvertB9842}[8] == [UnionConvertA9842(8)]

    m1 = MaybeFloat9842(1)
    m2 = MaybeFloat9842(nothing)
    @test m1.x === 1.0
    @test typeof(m1.x) === Float64
    @test m2.x === nothing
end

true
