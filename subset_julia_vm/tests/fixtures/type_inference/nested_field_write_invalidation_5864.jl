using Test

mutable struct NestedWriteInner5864
    val::Union{Int64,Nothing}
end

mutable struct NestedWriteOuter5864
    inner::NestedWriteInner5864
end

function nested_field_write_invalidation_5864(o::NestedWriteOuter5864)
    if o.inner.val !== nothing
        o.inner.val = nothing
        return o.inner.val
    end
    return 0
end

@test Base.infer_return_type(
    nested_field_write_invalidation_5864,
    Tuple{NestedWriteOuter5864},
) == Union{Nothing,Int64}
@test Core.Compiler.return_type(
    nested_field_write_invalidation_5864,
    Tuple{NestedWriteOuter5864},
) == Union{Nothing,Int64}

@test nested_field_write_invalidation_5864(NestedWriteOuter5864(NestedWriteInner5864(2))) === nothing
@test nested_field_write_invalidation_5864(NestedWriteOuter5864(NestedWriteInner5864(nothing))) == 0

true
