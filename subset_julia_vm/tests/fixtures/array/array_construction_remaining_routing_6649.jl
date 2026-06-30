using Test

function array_dyn_6649(x)
    if x == 1
        return 1
    end
    return 2.5
end

@testset "remaining array construction routes to Array wrapper (#6649)" begin
    typed = Int8[1, 2, 3]
    @test typeof(typed.ref) == MemoryRef{Int8}
    @test typed.size == (3,)
    @test eltype(typed) == Int8
    @test typed[2] == Int8(2)

    empty_ctor = Vector{Int64}()
    @test typeof(empty_ctor.ref) == MemoryRef{Int64}
    @test empty_ctor.size == (0,)
    @test length(empty_ctor) == 0

    comp = [i * i for i in 1:4]
    @test typeof(comp.ref) == MemoryRef{Int64}
    @test comp.size == (4,)
    @test comp[4] == 16

    typed_comp = Float32[i for i in 1:3]
    @test typeof(typed_comp.ref) == MemoryRef{Float32}
    @test typed_comp.size == (3,)
    @test typed_comp[2] == Float32(2)

    typejoined = [array_dyn_6649(i) for i in 1:2 if true]
    @test typeof(typejoined.ref) == MemoryRef{Real}
    @test eltype(typejoined) == Real
    @test typejoined[1] == 1
    @test typejoined[2] == 2.5

    pairs = [(1, 10), (2, 20)]
    destructured = [a + b for (a, b) in pairs]
    @test typeof(destructured.ref) == MemoryRef{Int64}
    @test destructured[1] == 11
    @test destructured[2] == 22

    mixed_pairs = [(1, 10), (2, 20.5)]
    mixed_destructured = [a + b for (a, b) in mixed_pairs]
    @test typeof(mixed_destructured.ref) == MemoryRef{Real}
    @test eltype(mixed_destructured) == Real
    @test mixed_destructured[1] == 11
    @test mixed_destructured[2] == 22.5

    undefed = Array{Int16}(undef, 2)
    @test typeof(undefed.ref) == MemoryRef{Int16}
    @test undefed.size == (2,)

    z = zeros(Int32, 2)
    @test typeof(z.ref) == MemoryRef{Int32}
    @test z == Int32[0, 0]

    f = fill(Int16(7), 2)
    @test typeof(f.ref) == MemoryRef{Int16}
    @test f == Int16[7, 7]

    s = similar(typed, 2)
    @test typeof(s.ref) == MemoryRef{Int8}
    @test s.size == (2,)

    t = trues(2)
    @test length(t) == 2
    @test t[1] == true

    ff = falses(2)
    @test length(ff) == 2
    @test ff[1] == false
end

true
