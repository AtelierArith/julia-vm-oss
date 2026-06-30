using Test

rank_dispatch(x::Array{Int64, 1}) = 1
rank_dispatch(x::Array{Int64, 2}) = 2
rank_dispatch(x::Array{Int64, 3}) = 3

alias_dispatch(x::Vector{Int64}) = 10
alias_dispatch(x::Matrix{Int64}) = 20

@testset "Array dimension parameter dispatch" begin
    v = [1, 2, 3]
    m = [1 2; 3 4]
    a3 = similar(v, 1, 3, 1)

    @test rank_dispatch(v) == 1
    @test rank_dispatch(m) == 2
    @test rank_dispatch(a3) == 3

    @test alias_dispatch(v) == 10
    @test alias_dispatch(m) == 20
end

@testset "Pure Julia Array wrapper participates in dimension dispatch" begin
    mem = Memory{Int64}(4)
    for i in 1:4
        mem[i] = i
    end

    w1 = wrap(Array, mem, 4)
    w2 = wrap(Array, mem, (2, 2))
    w3 = wrap(Array, mem, (2, 1, 2))

    @test rank_dispatch(w1) == 1
    @test rank_dispatch(w2) == 2
    @test rank_dispatch(w3) == 3
end

true
