# Type narrowing tests for field access in conditional branches
# Issue #1740: Verify type inference correctly narrows field access types
#
# This tests the feature implemented in Issue #1641:
# - Field access type narrowing with isa()
# - Field access type narrowing with nothing checks

using Test

# Test struct with Union type field
struct Container
    value::Union{Int64, Nothing}
end

struct StringContainer
    data::Union{String, Nothing}
end

# Helper: field access with isa() narrowing
function process_field_isa(c::Container)
    if isa(c.value, Int64)
        # In this branch, c.value should be narrowed to Int64
        return c.value + 1
    end
    return 0
end

# Helper: field access with !== nothing narrowing
function process_field_not_nothing(c::Container)
    if c.value !== nothing
        # In this branch, c.value should be narrowed to Int64
        return c.value * 2
    end
    return -1
end

# Helper: field access with === nothing check
function process_field_is_nothing(c::Container)
    if c.value === nothing
        return "nothing"
    else
        # In else branch, c.value should be narrowed to Int64
        return string(c.value)
    end
end

# Helper: using != instead of !==
function process_field_neq_nothing(c::Container)
    if c.value != nothing
        return c.value + 10
    end
    return 0
end

@testset "Field Access Type Narrowing" begin
    @testset "isa narrowing" begin
        c1 = Container(42)
        c2 = Container(nothing)

        @test process_field_isa(c1) == 43
        @test process_field_isa(c2) == 0
    end

    @testset "!== nothing narrowing" begin
        c1 = Container(10)
        c2 = Container(nothing)

        @test process_field_not_nothing(c1) == 20
        @test process_field_not_nothing(c2) == -1
    end

    @testset "=== nothing narrowing (else branch)" begin
        c1 = Container(5)
        c2 = Container(nothing)

        @test process_field_is_nothing(c2) == "nothing"
        @test process_field_is_nothing(c1) == "5"
    end

    @testset "!= nothing narrowing" begin
        c1 = Container(7)
        c2 = Container(nothing)

        @test process_field_neq_nothing(c1) == 17
        @test process_field_neq_nothing(c2) == 0
    end
end

true
