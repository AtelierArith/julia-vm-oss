using Test

function _has_int8_values(xs, expected)
    if length(xs) != length(expected)
        return false
    end
    for x in expected
        if !(x in xs)
            return false
        end
    end
    return true
end

@testset "Set construction and Set/Vector operations preserve typed keys (#4018, #4609, #4658)" begin
    s = Set(Int8[1, 2])
    @test typeof(s) === Set{Int8}
    @test eltype(s) === Int8

    collected = collect(s)
    @test typeof(collected) === Vector{Int8}
    @test eltype(collected) === Int8
    @test _has_int8_values(collected, Int8[1, 2])

    t = Set(Int8[2, 3])
    set_union = union(s, t)
    @test typeof(set_union) === Set{Int8}
    @test eltype(set_union) === Int8
    @test _has_int8_values(collect(set_union), Int8[1, 2, 3])

    set_intersect = intersect(s, t)
    @test typeof(set_intersect) === Set{Int8}
    @test eltype(set_intersect) === Int8
    @test _has_int8_values(collect(set_intersect), Int8[2])

    set_diff = setdiff(s, t)
    @test typeof(set_diff) === Set{Int8}
    @test eltype(set_diff) === Int8
    @test _has_int8_values(collect(set_diff), Int8[1])

    set_symdiff = symdiff(s, t)
    @test typeof(set_symdiff) === Set{Int8}
    @test eltype(set_symdiff) === Int8
    @test _has_int8_values(collect(set_symdiff), Int8[1, 3])

    mixed_union = union(Set(Int8[1, 2]), Int8[2, 3])
    @test typeof(mixed_union) === Set{Int8}
    @test eltype(mixed_union) === Int8
    @test _has_int8_values(collect(mixed_union), Int8[1, 2, 3])

    vector_set_union = union(Int8[1, 2], Set(Int8[2, 3]))
    @test typeof(vector_set_union) === Vector{Int8}
    @test eltype(vector_set_union) === Int8
    @test vector_set_union == Int8[1, 2, 3]

    float32_set = Set(Float32[1, 2])
    @test typeof(float32_set) === Set{Float32}
    @test eltype(float32_set) === Float32
    @test Float32(1) in float32_set
    @test Float32(2) in float32_set
end

true
