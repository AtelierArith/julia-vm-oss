using Test

@testset "MemoryRef runtime offset operations" begin
    m = Memory{Int64}(4)
    m[1] = 10
    m[2] = 20
    m[3] = 30
    m[4] = 40

    r = memoryref(m, 3)
    @test typeof(r) == MemoryRef{Int64}
    @test memoryindex(r) == 3
    @test parent(r) === m
    @test memoryrefget(r, :not_atomic, true) == 30

    memoryrefset!(r, 99, :not_atomic, true)
    @test m[3] == 99

    r2 = memoryref(r, 2)
    @test memoryindex(r2) == 4
    @test memoryrefget(r2, :not_atomic, true) == 40
end

true
