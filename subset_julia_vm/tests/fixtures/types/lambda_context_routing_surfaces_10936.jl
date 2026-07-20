using Test

# Issue #10936: LambdaContext routing must be exhaustive across every
# function-definition entry surface. Each definition below uses a
# builtin-spelled `where` binder inside a body parametric type — the exact
# pattern that silently froze to the builtin when a surface bypassed the
# context-aware lowering path (Issue #10934).

# Surface 1: short form.
surf_short_10936(x::Float64) where Float64<:Real = Vector{Float64}

# Surface 2: full form.
function surf_full_10936(x::Float64) where Float64<:Real
    Vector{Float64}
end

# Surface 3: block/local definition (definition nested in a block statement).
begin
    surf_block_10936(x::Float64) where Float64<:Real = Vector{Float64}
end

let
    global surf_let_10936
    surf_let_10936(x::Float64) where Float64<:Real = Vector{Float64}
end

# Surface 4: operator definition. (A custom Unicode operator would hit the
# separate call-target gap of Issue #10933, so extend a Base operator.)
/(x::Float64, y::String) where Float64<:Real = Vector{Float64}

# Surface 5: macro/eval-generated definition.
@eval surf_eval_10936(x::Float64) where Float64<:Real = Vector{Float64}

@eval function surf_eval_full_10936(x::Float64) where Float64<:Real
    Vector{Float64}
end

@testset "LambdaContext routing surfaces (Issue #10936)" begin
    @test surf_short_10936(1) === Vector{Int64}
    @test surf_short_10936(1.0) === Vector{Float64}

    @test surf_full_10936(1) === Vector{Int64}
    @test surf_full_10936(1.0) === Vector{Float64}

    @test surf_block_10936(1) === Vector{Int64}
    @test surf_block_10936(1.0) === Vector{Float64}

    @test surf_let_10936(1) === Vector{Int64}
    @test surf_let_10936(1.0) === Vector{Float64}

    @test (1 / "op") === Vector{Int64}
    @test (1.0 / "op") === Vector{Float64}

    @test surf_eval_10936(1) === Vector{Int64}
    @test surf_eval_10936(1.0) === Vector{Float64}

    @test surf_eval_full_10936(1) === Vector{Int64}
    @test surf_eval_full_10936(1.0) === Vector{Float64}
end

true
