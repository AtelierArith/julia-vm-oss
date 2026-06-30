using Test

# Issue #4269: integer-index field access on a PartialStruct.
#
# Upstream `getfield_tfunc` resolves `getfield(s, i::Int)` on a `Core.PartialStruct`
# positionally (via `_getfield_fieldindex`), giving the same field-type precision
# as the named form `getfield(s, :field)`. These fixtures exercise that index path
# for local bindings, returned structs, and inline-branch constructors.

struct IdxBox4269
    x
    y
end

# Local immutable binding: integer-index access selects the precise field type.
function local_idx4269()
    s = IdxBox4269(1, 2.0)
    return (getfield(s, 1), getfield(s, 2))
end

# Returned struct: field facts survive the call boundary, so index access on the
# returned value still resolves precisely.
make_idx_box4269(flag) = IdxBox4269(flag ? 1 : 2, 2.0)
use_idx_x4269(flag) = getfield(make_idx_box4269(flag), 1)
use_idx_y4269(flag) = getfield(make_idx_box4269(flag), 2)

# Inline ternary constructor: both branches build the same struct shape, so the
# joined PartialStruct keeps positional field order for index access.
use_ternary_idx4269(flag) = getfield(flag ? IdxBox4269(1, 2.0) : IdxBox4269(3, 4.0), 1)

# Named and indexed access of the same field must agree.
use_named_x4269(flag) = getfield(make_idx_box4269(flag), :x)

@testset "PartialStruct integer-index field inference (Issue #4269)" begin
    @test local_idx4269() == (1, 2.0)
    @test use_idx_x4269(true) == 1
    @test use_idx_y4269(false) == 2.0
    @test use_ternary_idx4269(true) == 1

    @test Base.infer_return_type(local_idx4269, Tuple{}) == Tuple{Int64,Float64}
    @test Base.infer_return_type(use_idx_x4269, Tuple{Bool}) == Int64
    @test Base.return_types(use_idx_x4269, Tuple{Bool})[1] == Int64
    @test Base.infer_return_type(use_idx_y4269, Tuple{Bool}) == Float64
    @test Base.return_types(use_idx_y4269, Tuple{Bool})[1] == Float64
    @test Base.infer_return_type(use_ternary_idx4269, Tuple{Bool}) == Int64

    # Index access is exactly as precise as named access for the same field.
    @test Base.infer_return_type(use_idx_x4269, Tuple{Bool}) ==
          Base.infer_return_type(use_named_x4269, Tuple{Bool})
end

true
