using Test

function check_flatten_collect_eltype(itr, expected_type, expected_values)
    result = collect(Base.Iterators.flatten(itr))
    ok = typeof(result) === Vector{expected_type}
    ok = ok && eltype(result) === expected_type
    ok = ok && length(result) == length(expected_values)
    for i in 1:length(expected_values)
        ok = ok && result[i] == expected_values[i]
        ok = ok && typeof(result[i]) === typeof(expected_values[i])
    end
    ok
end

@testset "flatten mixed eltype collect (Issues #4018/#4663)" begin
    @test check_flatten_collect_eltype((Int8[1, 2], Int16[3, 4]), Signed, Any[Int8(1), Int8(2), Int16(3), Int16(4)])
    @test check_flatten_collect_eltype((UInt8[1, 2], UInt16[3, 4]), Unsigned, Any[UInt8(1), UInt8(2), UInt16(3), UInt16(4)])
    @test check_flatten_collect_eltype((Int8[1, 2], UInt8[3, 4]), Integer, Any[Int8(1), Int8(2), UInt8(3), UInt8(4)])
    @test check_flatten_collect_eltype((Float32[1, 2], Float64[3, 4]), AbstractFloat, Any[Float32(1), Float32(2), Float64(3), Float64(4)])
    @test check_flatten_collect_eltype((Int8[], Int16[]), Signed, Any[])
end

true
