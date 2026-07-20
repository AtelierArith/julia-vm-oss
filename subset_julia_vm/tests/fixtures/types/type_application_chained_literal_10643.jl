# Chained type application `T{a}{b}` on a user parametric type binds the
# remaining trailing UnionAll binders with the outer argument group — in the
# direct chained-literal spelling, through a top-level type-valued binding
# (`w = T{a}; w{b}`, which registers a static alias whose target is an APPLIED
# type), and through Core.apply_type. The alias-mediated spelling used to
# expand statically and silently DROP the outer arguments (`w{Float64}` ->
# `Plain{Int64}`); it now routes through the same runtime ApplyTypeDynamic
# appending/validation as Core.apply_type. Issue #10643 (residual leg of the
# #10556 parity matrix; partial application itself is Issue #10192).

using Test

struct Plain10643{A,B} end
struct Trio10643{A,B,C} end

w10643 = Plain10643{Int64}
chained_in_fn_10643() = w10643{Float64}

@testset "chained literal type application T{a}{b} (Issue #10643)" begin
    # Direct chained literal.
    @test Plain10643{Int64}{Float64} === Plain10643{Int64,Float64}
    @test Trio10643{Int64}{Float64,String} === Trio10643{Int64,Float64,String}

    # Variable-mediated chaining (top-level type-valued binding).
    @test w10643{Float64} === Plain10643{Int64,Float64}
    @test w10643{String} === Plain10643{Int64,String}

    # The same binding applied inside a function body.
    @test chained_in_fn_10643() === Plain10643{Int64,Float64}

    # Core.apply_type on the partial UnionAll agrees (#10192).
    @test Core.apply_type(Plain10643{Int64}, Float64) ===
          Plain10643{Int64,Float64}

    # Over-application of the partial UnionAll still errors like upstream
    # ("too many parameters" ErrorException, not a silent rebind).
    @test_throws ErrorException w10643{Float64,String}

    # A fully-applied alias target accepts no further parameters (TypeError,
    # matching upstream jl_apply_type's UnionAll-base requirement).
    v10643 = Vector{Int64}
    @test_throws TypeError v10643{Float64}
end

true
