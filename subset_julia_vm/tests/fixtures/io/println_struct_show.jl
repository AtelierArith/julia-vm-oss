# Test println behavior for user-defined structs with/without Base.show
# Verifies that custom show methods are used when defined

using Test

# Struct without custom show method
struct PointNoShow
    x::Int64
    y::Int64
end

# Struct with custom show method
struct PointWithShow
    x::Int64
    y::Int64
end

function Base.show(io::IO, p::PointWithShow)
    print(io, "(", p.x, ", ", p.y, ")")
end

@testset "println uses Base.show for user-defined structs" begin
    # Test 1: Struct without custom show uses default format
    p1 = PointNoShow(1, 2)
    output1 = sprint(println, p1)
    @test occursin("PointNoShow", output1)
    @test occursin("1", output1)
    @test occursin("2", output1)

    # Test 2: Struct with custom show uses custom format
    p2 = PointWithShow(3, 4)
    output2 = sprint(println, p2)
    @test occursin("(3, 4)", output2)
    @test !occursin("PointWithShow", output2)  # Custom show doesn't include type name

    # Test 3: Verify the custom show output format
    p3 = PointWithShow(10, 20)
    output3 = sprint(println, p3)
    @test occursin("(10, 20)", output3)
end

true
