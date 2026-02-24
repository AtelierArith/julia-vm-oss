# Test user-defined promote_rule extension
# Users can add new promotion rules for their types

using Test

# Define a custom type
struct MyNumber
    value::Float64
end

# Define promotion rule: MyNumber + Int64 -> MyNumber
function promote_rule(::Type{MyNumber}, ::Type{Int64})
    MyNumber
end

# Define conversion
function convert(::Type{MyNumber}, x::Int64)
    MyNumber(Float64(x))
end

function convert(::Type{MyNumber}, x::MyNumber)
    x
end

@testset "user-defined promote_rule" begin
    # The custom promote_rule should work
    @test promote_rule(MyNumber, Int64) == MyNumber

    # promote_type should find our rule
    @test promote_type(MyNumber, Int64) == MyNumber
    @test promote_type(Int64, MyNumber) == MyNumber
end

true
