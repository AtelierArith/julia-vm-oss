using Test

# Issue #4269: nested PartialStruct field facts. When an immutable struct field
# is itself built by an analyzable immutable constructor, upstream Julia's
# `PartialStruct` is recursive — the field carries a nested `PartialStruct`, so
# a `getfield`/dot chain through the outer struct recovers the inner field's
# precise type rather than widening to `Any`.
struct InnerBox4269
    a
    b
end

struct OuterBox4269
    inner
    tag
end

make_inner4269(flag) = InnerBox4269(flag ? 1 : 2, "x")
make_outer4269(flag) = OuterBox4269(make_inner4269(flag), "t")

# getfield chain through the nested partial-struct field.
use_nested_getfield4269(flag) = getfield(getfield(make_outer4269(flag), :inner), :b)

# dot-access chain.
use_nested_dot4269(flag) = make_outer4269(flag).inner.b

# inner constructed inline (no interprocedural call).
inline_outer4269(flag) =
    getfield(getfield(OuterBox4269(InnerBox4269(flag ? 1 : 2, "x"), "t"), :inner), :b)

# positional integer-index chain.
use_nested_index4269(flag) = getfield(getfield(make_outer4269(flag), 1), 2)

@testset "Nested partial struct field inference (Issue #4269)" begin
    @test use_nested_getfield4269(true) == "x"
    @test use_nested_dot4269(false) == "x"
    @test inline_outer4269(true) == "x"
    @test use_nested_index4269(false) == "x"

    @test Base.infer_return_type(make_outer4269, Tuple{Bool}) == OuterBox4269
    @test Base.return_types(make_outer4269, Tuple{Bool})[1] == OuterBox4269
    @test Base.infer_return_type(use_nested_getfield4269, Tuple{Bool}) == String
    @test Base.return_types(use_nested_getfield4269, Tuple{Bool})[1] == String
    @test Base.infer_return_type(use_nested_dot4269, Tuple{Bool}) == String
    @test Base.infer_return_type(inline_outer4269, Tuple{Bool}) == String
    @test Base.infer_return_type(use_nested_index4269, Tuple{Bool}) == String
end

true
