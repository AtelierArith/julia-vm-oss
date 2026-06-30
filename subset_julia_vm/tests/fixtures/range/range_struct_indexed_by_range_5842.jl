using Test

# Issue #5842: indexing a range *struct* (e.g. `OneTo`) by a range dispatches to
# `getindex(::AbstractRange, ::AbstractRange)`. The base value is a struct, not a
# native `Value::Range`, so coercing it to the abstract param's `ValueType::Range`
# errored at compile time ("Cannot convert Struct(..) to Range"). This broke the
# eager full-Base compile (`integration_compile_sample_tests`) because Base itself
# contains `view(r::OneTo, indices::UnitRange) = r[indices]`.
@testset "Range struct indexed by a range (Issue #5842)" begin
    @test Base.OneTo(10)[2:4] == 2:4
    @test typeof(Base.OneTo(10)[2:4]) == UnitRange{Int64}

    # The same `getindex(::AbstractRange, ::AbstractRange)` dispatch through a
    # typed parameter, where the argument is a range struct.
    slice(r::AbstractRange, inds::AbstractRange) = r[inds]
    @test slice(Base.OneTo(10), 2:4) == 2:4
    @test slice(1:2:15, 2:4) == 3:2:7

    # `view` over range structs returns the range slice (Issue #5137 surface).
    @test view(Base.OneTo(8), 3:5) == 3:5
    @test collect(view(Base.OneTo(8), 3:5)) == [3, 4, 5]

    # Empty range indices preserve Julia's empty bounds, even when the empty
    # index range is outside the indexed range (Issue #5847).
    @test Base.OneTo(5)[2:1] == 2:1
    @test first(Base.OneTo(5)[2:1]) == 2
    @test last(Base.OneTo(5)[2:1]) == 1
    @test Base.OneTo(5)[6:5] == 6:5
    @test first(Base.OneTo(5)[6:5]) == 6
    @test last(Base.OneTo(5)[6:5]) == 5
    @test view(Base.OneTo(5), 2:1) == 2:1
    @test view(Base.OneTo(5), 6:5) == 6:5
    @test (1:2:19)[2:1] == 3:2:2
    @test (10:-2:2)[2:1] == 8:-2:9
end

true
