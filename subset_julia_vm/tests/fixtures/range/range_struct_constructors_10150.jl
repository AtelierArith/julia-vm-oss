using Test

function range_abstract_summary_10150(r::AbstractRange)
    return (first(r), last(r), length(r))
end

function range_ordinal_summary_10150(r::OrdinalRange)
    return (first(r), last(r), length(r))
end

function step_via_function_value_10150(r)
    f = step
    return f(r)
end

@testset "UnitRange direct struct constructors (#10150)" begin
    r_int = UnitRange(1, 3)
    @test typeof(r_int) === UnitRange{Int64}
    @test fieldnames(typeof(r_int)) == (:start, :stop)
    @test r_int.start == 1
    @test r_int.stop == 3
    @test first(r_int) == 1
    @test typeof(step(r_int)) === Int64
    @test step_via_function_value_10150(r_int) == 1
    @test last(r_int) == 3
    @test length(r_int) == 3
    @test r_int[2] == 2
    @test collect(r_int) == [1, 2, 3]
    r_int_iter1 = iterate(r_int)
    @test r_int_iter1 == (1, 1)
    @test iterate(r_int, Int64(r_int_iter1[2])) == (2, 2)
    @test range_abstract_summary_10150(r_int) == (1, 3, 3)

    empty_int = UnitRange(5, 1)
    @test typeof(empty_int) === UnitRange{Int64}
    @test first(empty_int) == 5
    @test last(empty_int) == 4
    @test length(empty_int) == 0
    @test collect(empty_int) == Int64[]
    @test iterate(empty_int) === nothing

    typed_empty_int = UnitRange{Int64}(5, 1)
    @test first(typed_empty_int) == 5
    @test last(typed_empty_int) == 4
    @test length(typed_empty_int) == 0
    @test collect(typed_empty_int) == Int64[]
    @test typed_empty_int isa OrdinalRange
    @test UnitRange <: OrdinalRange
    @test AbstractUnitRange <: OrdinalRange
    @test range_ordinal_summary_10150(r_int) == (1, 3, 3)

    r_big = UnitRange(big(1), big(3))
    @test typeof(r_big) === UnitRange{BigInt}
    @test eltype(r_big) === BigInt
    @test typeof(step(r_big)) === BigInt
    @test step_via_function_value_10150(r_big) == big(1)
    @test r_big[2] == big(2)
    r_big_values = collect(r_big)
    @test typeof(r_big_values) === Vector{BigInt}
    @test length(r_big_values) == 3
    @test r_big_values[1] == big(1)
    @test r_big_values[2] == big(2)
    @test r_big_values[3] == big(3)

    r_f64 = UnitRange(1.0, 3.0)
    @test typeof(r_f64) === UnitRange{Float64}
    @test typeof(step(r_f64)) === Float64
    @test r_f64[2] === 2.0
    @test collect(r_f64) == [1.0, 2.0, 3.0]

    r_f64_fractional = UnitRange(2.3, 5.2)
    @test typeof(r_f64_fractional) === UnitRange{Float64}
    @test first(r_f64_fractional) === 2.3
    @test last(r_f64_fractional) === 4.3
    @test length(r_f64_fractional) == 3
    @test collect(r_f64_fractional) == [2.3, 3.3, 4.3]
    typed_f64_fractional = UnitRange{Float64}(2.3, 5.2)
    @test last(typed_f64_fractional) === 4.3
    @test collect(typed_f64_fractional) == [2.3, 3.3, 4.3]

    r_f32 = UnitRange(1.0f0, 3.0f0)
    @test typeof(r_f32) === UnitRange{Float32}
    @test typeof(step(r_f32)) === Float32
    @test r_f32[2] === 2.0f0
    @test collect(r_f32) == Float32[1.0, 2.0, 3.0]

    r_uint = UnitRange(UInt8(1), UInt16(3))
    @test typeof(r_uint) === UnitRange{UInt16}
    @test eltype(r_uint) === UInt16
    @test typeof(step(r_uint)) === UInt16
    @test r_uint[2] === UInt16(2)
    @test collect(r_uint) == UInt16[1, 2, 3]
end

@testset "StepRange direct struct constructors (#10150)" begin
    r_int = StepRange(1, 2, 6)
    @test typeof(r_int) === StepRange{Int64, Int64}
    @test fieldnames(typeof(r_int)) == (:start, :step, :stop)
    @test r_int.start == 1
    @test r_int.step == 2
    @test r_int.stop == 5
    @test first(r_int) == 1
    @test step(r_int) == 2
    @test step_via_function_value_10150(r_int) == 2
    @test last(r_int) == 5
    @test length(r_int) == 3
    @test r_int[2] == 3
    @test collect(r_int) == [1, 3, 5]
    r_int_iter1 = iterate(r_int)
    @test r_int_iter1 == (1, 1)
    @test iterate(r_int, r_int_iter1[2]) == (3, 3)
    @test range_abstract_summary_10150(r_int) == (1, 5, 3)
    @test range_ordinal_summary_10150(r_int) == (1, 5, 3)

    typed_r_int = StepRange{Int64, Int64}(1, 2, 6)
    @test first(typed_r_int) == 1
    @test last(typed_r_int) == 5
    @test length(typed_r_int) == 3
    @test collect(typed_r_int) == [1, 3, 5]
    @test typed_r_int isa OrdinalRange

    r_big = StepRange(big(1), big(2), big(7))
    @test typeof(r_big) === StepRange{BigInt, BigInt}
    @test eltype(r_big) === BigInt
    @test typeof(step(r_big)) === BigInt
    @test r_big[2] == big(3)
    r_big_values = collect(r_big)
    @test typeof(r_big_values) === Vector{BigInt}
    @test length(r_big_values) == 4
    @test r_big_values[1] == big(1)
    @test r_big_values[2] == big(3)
    @test r_big_values[3] == big(5)
    @test r_big_values[4] == big(7)

    r_char = StepRange('a', 2, 'f')
    @test typeof(r_char) === StepRange{Char, Int64}
    @test eltype(r_char) === Char
    @test first(r_char) == 'a'
    @test step(r_char) == 2
    @test last(r_char) == 'e'
    @test r_char[2] == 'c'
    @test collect(r_char) == ['a', 'c', 'e']
    r_char_iter1 = iterate(r_char)
    @test r_char_iter1 == ('a', 'a')
    @test iterate(r_char, r_char_iter1[2]) == ('c', 'c')

    empty_char_positive = StepRange('f', 2, 'a')
    @test typeof(empty_char_positive) === StepRange{Char, Int64}
    @test first(empty_char_positive) == 'f'
    @test last(empty_char_positive) == 'a'
    @test length(empty_char_positive) == 0
    @test collect(empty_char_positive) == Char[]
    empty_char_negative = StepRange('a', -2, 'f')
    @test first(empty_char_negative) == 'a'
    @test last(empty_char_negative) == 'f'
    @test length(empty_char_negative) == 0
    @test collect(empty_char_negative) == Char[]

    r_uint = StepRange(UInt16(1), UInt8(2), UInt16(6))
    @test typeof(r_uint) === StepRange{UInt16, UInt8}
    @test eltype(r_uint) === UInt16
    @test typeof(step(r_uint)) === UInt8
    @test r_uint[2] === UInt16(3)
    @test collect(r_uint) == UInt16[1, 3, 5]

    @test_throws ArgumentError StepRange(1.0, 1, 3.0)
    @test_throws ArgumentError StepRange(1, 0, 3)
end

true
