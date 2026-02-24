# Test user-defined convert and promote_rule methods
# These are critical for custom type interoperability (Issue #2587)
using Test

# Define a custom numeric wrapper type
struct MyNum
    value::Int64
end

# User-defined convert: Int64 → MyNum
function convert(::Type{MyNum}, x::Int64)
    return MyNum(x)
end

# User-defined convert: MyNum → Int64
function convert(::Type{Int64}, m::MyNum)
    return m.value
end

# User-defined promote_rule
function promote_rule(::Type{MyNum}, ::Type{Int64})
    return MyNum
end

@testset "User-defined convert" begin
    # Convert Int64 to MyNum
    result = convert(MyNum, 42)
    @test result isa MyNum
    @test result.value == 42

    # Convert MyNum to Int64
    m = MyNum(99)
    @test convert(Int64, m) == 99
end

@testset "User-defined promote_rule" begin
    # Direct promote_rule call works
    @test promote_rule(MyNum, Int64) == MyNum
end

# Note: promote_type(MyNum, Int64) does NOT work yet because
# promote_type internally uses CallTypedDispatch with a frozen
# candidate list that misses user-defined promote_rule methods.
# See Issue #2587 for details.

true
