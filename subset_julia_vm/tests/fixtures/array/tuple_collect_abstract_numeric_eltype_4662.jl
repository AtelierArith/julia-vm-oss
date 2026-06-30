using Test

function check_collect_tuple_eltype(t, expected_type)
    result = collect(t)
    ok = typeof(result) === Vector{expected_type}
    ok = ok && eltype(result) === expected_type
    ok = ok && length(result) == length(t)
    for i in 1:length(t)
        ok = ok && result[i] == t[i]
        ok = ok && typeof(result[i]) === typeof(t[i])
    end
    ok
end

function check_typed_undef_eltype(T)
    result = Vector{T}(undef, 2)
    ok = typeof(result) === Vector{T}
    ok = ok && eltype(result) === T
    ok = ok && length(result) == 2
    ok
end

@testset "tuple collect abstract numeric eltype (Issues #4018/#4662)" begin
    @test check_collect_tuple_eltype((Int8(1), Int16(2)), Signed)
    @test check_collect_tuple_eltype((UInt8(1), UInt16(2)), Unsigned)
    @test check_collect_tuple_eltype((Int8(1), UInt8(2)), Integer)
    @test check_collect_tuple_eltype((Float32(1), Float64(2)), AbstractFloat)
    @test check_collect_tuple_eltype((Int8(1), Float64(2)), Real)

    @test check_typed_undef_eltype(Number)
    @test check_typed_undef_eltype(Real)
    @test check_typed_undef_eltype(Integer)
    @test check_typed_undef_eltype(Signed)
    @test check_typed_undef_eltype(Unsigned)
    @test check_typed_undef_eltype(AbstractFloat)
end

true
