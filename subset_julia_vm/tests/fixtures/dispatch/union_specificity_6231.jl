# Issue #6231: finite Union methods are more specific than broader supertypes
# for arguments covered by the Union.

using Test

union_specificity_6231(::Union{Int64,String}) = :union_int_string
union_specificity_6231(::Integer) = :integer

union_super_specificity_6231(::Union{Integer,String}) = :union_integer_string
union_super_specificity_6231(::Real) = :real

function union_specificity_via_any_6231(x)
    a::Any = x
    union_specificity_6231(a)
end

function union_super_specificity_via_any_6231(x)
    a::Any = x
    union_super_specificity_6231(a)
end

@testset "Union specificity (Issue #6231)" begin
    @test union_specificity_6231(1) === :union_int_string
    @test union_specificity_6231("x") === :union_int_string
    @test union_super_specificity_6231(1) === :union_integer_string
    @test union_super_specificity_6231(1.0) === :real
    @test union_super_specificity_6231("x") === :union_integer_string

    @test union_specificity_via_any_6231(1) === :union_int_string
    @test union_specificity_via_any_6231("x") === :union_int_string
    @test union_super_specificity_via_any_6231(1) === :union_integer_string
    @test union_super_specificity_via_any_6231(1.0) === :real
    @test union_super_specificity_via_any_6231("x") === :union_integer_string
end

union_specificity_6231(1) === :union_int_string &&
    union_specificity_6231("x") === :union_int_string &&
    union_super_specificity_6231(1) === :union_integer_string &&
    union_super_specificity_6231(1.0) === :real &&
    union_super_specificity_6231("x") === :union_integer_string &&
    union_specificity_via_any_6231(1) === :union_int_string &&
    union_specificity_via_any_6231("x") === :union_int_string &&
    union_super_specificity_via_any_6231(1) === :union_integer_string &&
    union_super_specificity_via_any_6231(1.0) === :real &&
    union_super_specificity_via_any_6231("x") === :union_integer_string
