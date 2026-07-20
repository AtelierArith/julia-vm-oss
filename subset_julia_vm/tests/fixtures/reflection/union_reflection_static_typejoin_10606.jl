# Follow-up coverage for Issue #10606. The dynamic `getfield`/`GetFieldByName`
# path and `fieldnames(Union)` are covered by
# `union_reflection_fields_10606.jl`. This fixture pins the two paths that fix
# missed:
#
#   1. STATIC dot access — when the compiler narrows the receiver to
#      `DataType`, `t.parameters`/`t.var`/`t.body`/`t.lb`/`t.ub` route to the
#      dedicated reflection builtins (`_TypeParameters`, `_UnionAllVar`,
#      `_UnionAllBody`, `_TypeVarLowerBound`, `_TypeVarUpperBound`), NOT the
#      field-match. For a Union receiver every one must raise
#      `FieldError(Union, :field)` — not an svec of the branch types, and not a
#      `FieldError(DataType, ...)` with the wrong receiver name.
#   2. `typejoin` over Union operands — a Union operand is collapsed via its
#      `a`/`b` branch fields BEFORE the DataType paths index `.parameters`
#      (which now raises FieldError), mirroring upstream `promotion.jl`.

using Test

# `u` is a local assigned a Union *literal*, so the compiler narrows it to
# `DataType` and the direct dot access below lowers through the STATIC
# reflection builtins. Returning `(e.type, e.field)` pins the receiver type
# name (`Union`), which `@test_throws FieldError` alone would not — the pre-fix
# `.var`/`.body`/`.lb`/`.ub` builtins already threw, but as
# `FieldError(DataType, ...)`.
function static_fe_parts_10606(field::Symbol)
    u = Union{Int64,Float64}
    try
        if field === :parameters
            u.parameters
        elseif field === :var
            u.var
        elseif field === :body
            u.body
        elseif field === :lb
            u.lb
        elseif field === :ub
            u.ub
        elseif field === :name
            u.name
        end
        return (nothing, nothing)
    catch e
        return (e.type, e.field)
    end
end

@testset "Union static-path reflection fields are FieldError(Union) (Issue #10606)" begin
    @test static_fe_parts_10606(:parameters) == (Union, :parameters)
    @test static_fe_parts_10606(:var) == (Union, :var)
    @test static_fe_parts_10606(:body) == (Union, :body)
    @test static_fe_parts_10606(:lb) == (Union, :lb)
    @test static_fe_parts_10606(:ub) == (Union, :ub)
    @test static_fe_parts_10606(:name) == (Union, :name)
end

@testset "Union branch fields a/b still resolve on the static path (Issue #10606)" begin
    u = Union{Int64,Float64}
    @test u.a === Float64
    @test u.b === Int64
end

@testset "typejoin over Union operands (Issue #10606 blast radius)" begin
    @test typejoin(Union{Int64,Float64}, String) === Any
    @test typejoin(Union{Int64,Float64}, Union{Int64,String}) === Any
    @test typejoin(Union{Int8,Int16}, Int32) === Signed
    # A Union fed back through a reduce-style accumulation must not throw.
    @test reduce(typejoin, [Int64, Union{Int64,Float64}, Float32]) === Real
end

true
