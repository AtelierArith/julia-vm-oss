using Test

inc4265_typeof_trait(x) = x + 1
tofloat4265_typeof_trait(x) = x + 0.5

@testset "Base.Generator typeof trait and collect inference (Issue #4265)" begin
    g = Base.Generator(inc4265_typeof_trait, [1, 2, 3])

    @test typeof(Base.IteratorSize(typeof(g))) === typeof(Base.HasShape{1}())
    @test typeof(Base.IteratorEltype(typeof(g))) === typeof(Base.EltypeUnknown())
    @test Base.infer_return_type(collect, Tuple{typeof(g)}) === Vector{Int64}
    G = typeof(g)
    @test Tuple{G} === Tuple{typeof(g)}
    @test Base.infer_return_type(collect, Tuple{G}) === Vector{Int64}
    @test Base.infer_return_type(
        collect,
        Tuple{Base.Generator{Vector{Int64}, typeof(inc4265_typeof_trait)}},
    ) === Vector{Int64}
    @test collect(g) == [2, 3, 4]
    @test typeof(collect(g)) === Vector{Int64}

    gf = Base.Generator(tofloat4265_typeof_trait, [1, 2, 3])
    @test Base.infer_return_type(collect, Tuple{typeof(gf)}) === Vector{Float64}
    GF = typeof(gf)
    @test Tuple{GF} === Tuple{typeof(gf)}
    @test Base.infer_return_type(collect, Tuple{GF}) === Vector{Float64}
    @test Base.infer_return_type(
        collect,
        Tuple{Base.Generator{Vector{Int64}, typeof(tofloat4265_typeof_trait)}},
    ) === Vector{Float64}
    @test typeof(collect(gf)) === Vector{Float64}
end

true
