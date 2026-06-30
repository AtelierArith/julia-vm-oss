using Test

@testset "Array dimension type parameters" begin
    v = [1, 2, 3]
    m = [1 2; 3 4]
    a3 = similar(v, 2, 1, 2)

    @test typeof(v) == Vector{Int64}
    @test typeof(v) == Array{Int64, 1}
    @test v isa Vector{Int64}
    @test v isa Array{Int64, 1}
    @test !(v isa Matrix{Int64})
    @test !(v isa Array{Int64, 2})

    @test typeof(m) == Matrix{Int64}
    @test typeof(m) == Array{Int64, 2}
    @test m isa Matrix{Int64}
    @test m isa Array{Int64, 2}
    @test !(m isa Vector{Int64})
    @test !(m isa Array{Int64, 1})

    @test typeof(a3) == Array{Int64, 3}
    @test a3 isa Array{Int64, 3}
    @test a3 isa Array
    @test !(a3 isa Array{Int64, 2})
    @test !(a3 isa Matrix{Int64})
end

@testset "Pure Julia Array wrapper projects rank from _size" begin
    mem = Memory{Int64}(4)
    for i in 1:4
        mem[i] = i
    end

    w1 = wrap(Array, mem, 4)
    w2 = wrap(Array, mem, (2, 2))
    w3 = wrap(Array, mem, (2, 1, 2))

    @test typeof(w1) == Vector{Int64}
    @test w1 isa Array{Int64, 1}
    @test !(w1 isa Array{Int64, 2})

    @test typeof(w2) == Matrix{Int64}
    @test w2 isa Array{Int64, 2}
    @test !(w2 isa Array{Int64, 1})

    @test typeof(w3) == Array{Int64, 3}
    @test w3 isa Array{Int64, 3}
    @test !(w3 isa Array{Int64, 2})
end

true
