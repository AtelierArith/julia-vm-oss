# Native range type identity and accessor surfaces stay in sync.

using Test

range_dispatch_key(r::UnitRange{T}) where {T} = (:unit, string(T))
range_dispatch_key(r::StepRange{T,S}) where {T,S} = (:step, string(T), string(S))
function range_dispatch_key(r::StepRangeLen{T,R,S,L}) where {T,R,S,L}
    return (:steprangelen, string(T), string(R), string(S), string(L))
end
range_dispatch_key(r) = (:fallback, string(typeof(r)))

function range_dispatch_key_via_bound_function(r)
    f = range_dispatch_key
    return f(r)
end

function range_dispatch_key_from_any(r)
    x = Any[r][1]
    return range_dispatch_key(x)
end

function step_pair(r)
    s = step(r)
    return (typeof(s), s)
end

function step_pair_via_bound_function(r)
    f = step
    s = f(r)
    return (typeof(s), s)
end

function step_pair_from_any(r)
    x = Any[r][1]
    s = step(x)
    return (typeof(s), s)
end

function step_pair_from_any_via_bound_function(r)
    x = Any[r][1]
    f = step
    s = f(x)
    return (typeof(s), s)
end

function accessor_tuple(r)
    return (
        typeof(first(r)),
        first(r),
        typeof(last(r)),
        last(r),
        typeof(r[2]),
        r[2],
    )
end

function accessor_tuple_via_bound_functions(r)
    ffirst = first
    flast = last
    return (
        typeof(ffirst(r)),
        ffirst(r),
        typeof(flast(r)),
        flast(r),
    )
end

function accessor_tuple_from_any_via_bound_functions(r)
    x = Any[r][1]
    ffirst = first
    flast = last
    return (
        typeof(ffirst(x)),
        ffirst(x),
        typeof(flast(x)),
        flast(x),
    )
end

function length_tuple(r)
    x = Any[r][1]
    flength = length
    return (length(r), flength(r), length(x), flength(x))
end

function iteration_edges_from_any(r)
    x = Any[r][1]
    next = iterate(x)
    if next === nothing
        return (0, nothing, nothing)
    end
    count = 1
    first_value = next[1]
    last_value = next[1]
    state = next[2]
    while true
        next = iterate(x, state)
        if next === nothing
            break
        end
        count += 1
        last_value = next[1]
        state = next[2]
    end
    return (count, first_value, last_value)
end

function check_step_surfaces(r, expected)
    @test step_pair(r) == expected
    @test step_pair_via_bound_function(r) == expected
    @test step_pair_from_any(r) == expected
    @test step_pair_from_any_via_bound_function(r) == expected
end

function check_accessor_surfaces(r, expected)
    expected_first_last = (expected[1], expected[2], expected[3], expected[4])
    @test accessor_tuple(r) == expected
    @test accessor_tuple_via_bound_functions(r) == expected_first_last
    @test accessor_tuple_from_any_via_bound_functions(r) == expected_first_last
end

function check_dispatch_surfaces(r, expected)
    @test range_dispatch_key(r) == expected
    @test range_dispatch_key_via_bound_function(r) == expected
    @test range_dispatch_key_from_any(r) == expected
end

function check_length_iteration_surfaces(r, expected_len, expected_values, expected_edges)
    @test length_tuple(r) == (expected_len, expected_len, expected_len, expected_len)
    @test collect(r) == expected_values
    @test collect(Any[r][1]) == expected_values
    @test iteration_edges_from_any(r) == expected_edges
end

@testset "native range identity and accessors stay synchronized (#9815)" begin
    r_unit_uint = UInt8(1):UInt8(3)
    @test typeof(r_unit_uint) === UnitRange{UInt8}
    @test typeof(collect(Any[r_unit_uint][1])) === Vector{UInt8}

    r_unit_big = big(1):big(3)
    @test typeof(r_unit_big) === UnitRange{BigInt}
    @test eltype(r_unit_big) === BigInt
    check_dispatch_surfaces(r_unit_big, (:unit, "BigInt"))
    check_step_surfaces(r_unit_big, (BigInt, big(1)))
    check_accessor_surfaces(r_unit_big, (BigInt, big(1), BigInt, big(3), BigInt, big(2)))
    check_length_iteration_surfaces(r_unit_big, big(3), [big(1), big(2), big(3)], (3, big(1), big(3)))

    r_big = big(1):2:big(9)
    @test typeof(r_big) === StepRange{BigInt, Int64}
    @test eltype(r_big) === BigInt
    check_dispatch_surfaces(r_big, (:step, "BigInt", "Int64"))
    check_step_surfaces(r_big, (Int64, 2))
    check_accessor_surfaces(r_big, (BigInt, big(1), BigInt, big(9), BigInt, big(3)))
    check_length_iteration_surfaces(
        r_big,
        big(5),
        [big(1), big(3), big(5), big(7), big(9)],
        (5, big(1), big(9)),
    )

    r_narrow = Int8(1):Int8(2):Int16(5)
    @test typeof(r_narrow) === StepRange{Int16, Int8}
    @test eltype(r_narrow) === Int16
    check_dispatch_surfaces(r_narrow, (:step, "Int16", "Int8"))
    check_step_surfaces(r_narrow, (Int8, Int8(2)))
    check_accessor_surfaces(r_narrow, (Int16, Int16(1), Int16, Int16(5), Int16, Int16(3)))
    @test typeof(collect(Any[r_narrow][1])) === Vector{Int16}
    check_length_iteration_surfaces(
        r_narrow,
        3,
        Int16[1, 3, 5],
        (3, Int16(1), Int16(5)),
    )

    r_uint = UInt8(1):UInt8(1):UInt16(5)
    @test typeof(r_uint) === StepRange{UInt16, UInt8}
    @test eltype(r_uint) === UInt16
    check_dispatch_surfaces(r_uint, (:step, "UInt16", "UInt8"))
    check_step_surfaces(r_uint, (UInt8, UInt8(1)))
    check_accessor_surfaces(r_uint, (UInt16, UInt16(1), UInt16, UInt16(5), UInt16, UInt16(2)))
    @test typeof(collect(Any[r_uint][1])) === Vector{UInt16}
    check_length_iteration_surfaces(
        r_uint,
        5,
        UInt16[1, 2, 3, 4, 5],
        (5, UInt16(1), UInt16(5)),
    )

    r_char = 'a':Int8(1):'c'
    @test typeof(r_char) === StepRange{Char, Int8}
    @test eltype(r_char) === Char
    check_dispatch_surfaces(r_char, (:step, "Char", "Int8"))
    check_step_surfaces(r_char, (Int8, Int8(1)))
    check_accessor_surfaces(r_char, (Char, 'a', Char, 'c', Char, 'b'))
    check_length_iteration_surfaces(r_char, 3, ['a', 'b', 'c'], (3, 'a', 'c'))

    r_float = 0.0:0.5:1.0
    @test string(typeof(r_float)) ==
          "StepRangeLen{Float64, Base.TwicePrecision{Float64}, Base.TwicePrecision{Float64}, Int64}"
    @test eltype(r_float) === Float64
    check_dispatch_surfaces(
        r_float,
        (
            :steprangelen,
            "Float64",
            "Base.TwicePrecision{Float64}",
            "Base.TwicePrecision{Float64}",
            "Int64",
        ),
    )
    check_step_surfaces(r_float, (Float64, 0.5))
    check_accessor_surfaces(r_float, (Float64, 0.0, Float64, 1.0, Float64, 0.5))
    check_length_iteration_surfaces(r_float, 3, [0.0, 0.5, 1.0], (3, 0.0, 1.0))

    xs = range(-2.0, 1.0; length=5)
    ys = range(1.2, -1.2; length=3)
    row = xs'
    grid = row .+ im .* ys
    @test length(row) == 5
    @test row[1, 5] == 1.0
    @test length(grid) == 15
    @test grid[1, 1] == -2.0 + 1.2im
    @test grid[3, 5] == 1.0 - 1.2im
end

true
