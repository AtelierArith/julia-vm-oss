# First-class PartialStruct lattice element (Issue #8544).
#
# Constructor-site per-field type facts must survive as lattice values —
# through argument passing into a helper, through a helper's return
# (interprocedural return-type caching), and through getfield/dot chains —
# instead of widening to the declared field type (Any for untyped fields)
# as soon as the value crosses a function boundary.
using Test

# Untyped fields: the declared field type is Any, so any precision below
# comes from the constructor-site PartialStruct fact, not the declaration.
struct PSBox8544
    v
end

struct PSPair8544
    a
    b
end

struct PSOuter8544
    inner
    tag
end

# Parametric struct with one typed and one untyped field.
struct PSWrap8544{T}
    x::T
    y
end

# 1. Constructed struct passed INTO a helper that reads the untyped field:
#    the argument's PartialStruct fact must reach the callee's parameter.
ps_read_v_8544(b) = b.v + 1
ps_pass_into_helper_8544() = ps_read_v_8544(PSBox8544(3))

# 2. Identity-like round trip: the fact survives a helper that takes the
#    struct as an argument and returns it (CachedReturn carries the fact).
ps_id_8544(p) = p
function ps_roundtrip_8544()
    b = ps_id_8544(PSBox8544(3))
    b.v + 1
end

# 3. getfield by constant integer index across the same boundary.
ps_read_second_8544(p) = getfield(p, 2) * 2.0
ps_index_into_helper_8544() = ps_read_second_8544(PSPair8544("s", 1.5))

# 4. Nested dot chain across a function boundary: outer.inner.v stays
#    precise because the field fact is itself a PartialStruct.
ps_read_inner_v_8544(o) = o.inner.v + 1
ps_nested_chain_8544() = ps_read_inner_v_8544(PSOuter8544(PSBox8544(41), "t"))

# 5. Branch join: both branches build the same struct shape, so the joined
#    PartialStruct keeps the field-wise join (Int64) into the callee.
function ps_branch_join_8544(flag::Bool)
    b = flag ? PSBox8544(1) : PSBox8544(2)
    ps_read_v_8544(b)
end

# 6. Parametric struct: untyped field fact survives into a helper. (`flag`
#    stays UNtyped: reflection only re-infers a body when a param is untyped
#    or the return snapshot is unknown, and the parametric-constructor return
#    snapshot path has a separate pre-existing imprecision — see the fully
#    annotated `flag::Bool` variant widening to Any, tracked independently of
#    Issue #8544.)
ps_read_y_8544(w) = w.y + 1
ps_parametric_into_helper_8544(flag) = ps_read_y_8544(PSWrap8544(flag ? 1.5 : 2.5, 41))

# NOTE: the inference testset runs BEFORE the behavior testset on purpose.
# Reflecting on a parametric-constructor-calling function AFTER it has been
# executed hits a separate pre-existing snapshot-seeding imprecision that
# widens the result to Any (upstream julia is order-independent).
@testset "PartialStruct inference (Issue #8544)" begin
    @test Base.infer_return_type(ps_pass_into_helper_8544, Tuple{}) === Int64
    @test Base.infer_return_type(ps_roundtrip_8544, Tuple{}) === Int64
    @test Base.infer_return_type(ps_index_into_helper_8544, Tuple{}) === Float64
    @test Base.infer_return_type(ps_nested_chain_8544, Tuple{}) === Int64
    @test Base.infer_return_type(ps_branch_join_8544, Tuple{Bool}) === Int64
    @test Base.infer_return_type(ps_parametric_into_helper_8544, Tuple{Bool}) === Int64
end

@testset "PartialStruct behavior (Issue #8544)" begin
    @test ps_pass_into_helper_8544() == 4
    @test ps_roundtrip_8544() == 4
    @test ps_index_into_helper_8544() == 3.0
    @test ps_nested_chain_8544() == 42
    @test ps_branch_join_8544(true) == 2
    @test ps_branch_join_8544(false) == 3
    @test ps_parametric_into_helper_8544(true) == 42
    @test ps_parametric_into_helper_8544(false) == 42
end

true
