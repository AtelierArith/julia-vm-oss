using Test

struct ParamTypeVarBox4696{T}
    x::T
end

@testset "parametric application preserves runtime TypeVar (Issue #4696)" begin
    T = TypeVar(:T)

    # Vector{T} and Matrix{T} keep T as a TypeVar reference rather than
    # erasing it to Any. (Full identity (`===`) preservation between the
    # original fresh TypeVar and the projected parameter is not yet
    # modeled in sjulia — see follow-up below — so we assert kind + name.)
    @test isa(Vector{T}.parameters[1], TypeVar)
    @test Vector{T}.parameters[1].name === :T
    @test isa(Matrix{T}.parameters[1], TypeVar)
    @test Matrix{T}.parameters[1].name === :T

    # Multi-parameter cases also preserve the TypeVar name.
    @test isa(Dict{T, T}.parameters[1], TypeVar)
    @test Dict{T, T}.parameters[1].name === :T
    @test isa(Dict{T, T}.parameters[2], TypeVar)
    @test Dict{T, T}.parameters[2].name === :T

    # User parametric structs accept runtime TypeVars too.
    @test isa(ParamTypeVarBox4696{T}.parameters[1], TypeVar)
    @test ParamTypeVarBox4696{T}.parameters[1].name === :T
end

@testset "UnionAll wraps when body references runtime TypeVar (Issue #4696)" begin
    S = TypeVar(:S)
    ua = UnionAll(S, Vector{S})
    @test isa(ua, UnionAll)
    @test ua.var.name === :S
    # And the smart-wrap from #4694 still kicks in when body doesn't
    # mention the bound variable.
    @test UnionAll(S, Vector{Int64}) === Vector{Int64}
end

true
