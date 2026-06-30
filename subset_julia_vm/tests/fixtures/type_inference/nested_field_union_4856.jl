# Issue #4856: Repeated nested immutable field reads (`x.inner.value`,
# `getfield(getfield(x, :inner), :value)`, and the equivalent
# triple-nest) widened to `Any` instead of preserving the inner
# field's declared `Union{Int64, Nothing}` type. The single-level case
# (Issue #4270) already worked.
#
# Root cause: `StructField::as_julia_type()` only returns `Some` for
# `TypeExpr::Concrete`, so a user-struct-typed field like
# `inner::InnerProbe4856` (parsed as `TypeExpr::Named`) fell through
# to `ValueType::Any` while the surrounding compile/mod.rs code only
# treated the `TypeExpr::Parameterized { base: "Union", .. }` case
# specially. The lattice struct table therefore stored
# `OuterProbe4856.inner` as `Any`, and the abstract-interp engine
# could not chain into `InnerProbe4856.value`'s declared union type.
#
# Fix: in compile/mod.rs, fall back to
# `TypeExpr::to_julia_type_lossy` for any typed field
# (not just Union) so struct-typed fields land as
# `ValueType::Struct(type_id)` whenever the field's struct is already
# registered in `struct_table` (i.e. defined earlier in source order).

using Test

struct InnerProbe4856
    value::Union{Int64,Nothing}
end

struct OuterProbe4856
    inner::InnerProbe4856
end

struct TripleProbe4856
    outer::OuterProbe4856
end

function nested_field_guard_4856(x::OuterProbe4856)
    if x.inner.value !== nothing
        return x.inner.value
    end
    return 0
end

function nested_getfield_guard_4856(x::OuterProbe4856)
    if getfield(getfield(x, :inner), :value) !== nothing
        return getfield(getfield(x, :inner), :value)
    end
    return 0
end

get_inner_4856(x::OuterProbe4856) = x.inner

triple_nested_4856(t::TripleProbe4856) = t.outer.inner.value

@testset "nested immutable field guard preserves declared union (Issue #4856)" begin
    @test Base.infer_return_type(nested_field_guard_4856, Tuple{OuterProbe4856}) ==
        Union{Nothing,Int64}
end

@testset "nested getfield chain preserves declared union (Issue #4856)" begin
    @test Base.infer_return_type(nested_getfield_guard_4856, Tuple{OuterProbe4856}) ==
        Union{Nothing,Int64}
end

@testset "single-level struct-typed field reads preserve struct identity (Issue #4856)" begin
    # Without the fix this was `Any`. Confirms `OuterProbe4856.inner` is
    # stored as the proper struct type, not the synthetic-name fallback.
    @test Base.infer_return_type(get_inner_4856, Tuple{OuterProbe4856}) == InnerProbe4856
end

@testset "triple-nested field chain still resolves (Issue #4856)" begin
    @test Base.infer_return_type(triple_nested_4856, Tuple{TripleProbe4856}) ==
        Union{Nothing,Int64}
end

@testset "behavioral check on actual values (Issue #4856)" begin
    @test nested_field_guard_4856(OuterProbe4856(InnerProbe4856(42))) == 42
    @test nested_field_guard_4856(OuterProbe4856(InnerProbe4856(nothing))) == 0
    @test triple_nested_4856(TripleProbe4856(OuterProbe4856(InnerProbe4856(7)))) == 7
    @test triple_nested_4856(TripleProbe4856(OuterProbe4856(InnerProbe4856(nothing)))) === nothing
end

true
