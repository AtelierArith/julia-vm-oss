using Test

@testset "range/BigInt endpoints beyond Float64 exact range (Issue #9518)" begin
    base = big(10)^20
    r = base:base+2

    @test typeof(r) == UnitRange{BigInt}
    @test eltype(r) == BigInt
    @test typeof(length(r)) == BigInt
    @test length(r) == big(3)

    @test first(r) == base
    @test first(r) isa BigInt
    @test last(r) == base + 2
    @test last(r) isa BigInt

    @test r[1] == base
    @test r[2] == base + 1
    @test r[3] == base + 2
    @test r[2] isa BigInt

    collected = collect(r)
    @test length(collected) == 3
    @test collected[1] == base
    @test collected[2] == base + 1
    @test collected[3] == base + 2
    @test all(x -> x isa BigInt, collected)

    comp = [x for x in r]
    @test typeof(comp) == Vector{BigInt}
    @test comp[1] == base
    @test comp[2] == base + 1
    @test comp[3] == base + 2

    seen = BigInt[]
    for x in r
        push!(seen, x)
    end
    @test seen[1] == base
    @test seen[2] == base + 1
    @test seen[3] == base + 2

    @test base + 1 in r
    @test !(base + 3 in r)

    stepped = base:big(2):base+4
    @test typeof(stepped) == StepRange{BigInt, BigInt}
    @test typeof(step(stepped)) == BigInt
    @test step(stepped) == big(2)
    @test length(stepped) == big(3)
    @test collect(stepped)[2] == base + 2
end

true
