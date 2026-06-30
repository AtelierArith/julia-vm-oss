using Test

mutable struct NestedInner5862
    val::Union{Int64,Nothing}
end

mutable struct NestedOuter5862
    inner::NestedInner5862
end

function nested_field_guard_refinement_5862(o::NestedOuter5862)
    if o.inner.val !== nothing
        return o.inner.val + 1
    end
    return 0
end

@test Base.infer_return_type(
    nested_field_guard_refinement_5862,
    Tuple{NestedOuter5862},
) == Int64
@test Core.Compiler.return_type(
    nested_field_guard_refinement_5862,
    Tuple{NestedOuter5862},
) == Int64

@test nested_field_guard_refinement_5862(NestedOuter5862(NestedInner5862(2))) == 3
@test nested_field_guard_refinement_5862(NestedOuter5862(NestedInner5862(nothing))) == 0

true
