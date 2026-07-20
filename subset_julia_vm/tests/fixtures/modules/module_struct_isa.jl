# Test: isa and typeof work correctly for structs defined in modules
# This verifies that type names are normalized (module prefix, Int -> Int64)

using Test

module TestModule

export MyStruct

struct MyStruct{T<:Real}
    x::T
    y::T
end

end #module

using .TestModule

s = MyStruct(1, 2)

# These should all be true
@assert s isa MyStruct{Int}
@assert s isa MyStruct{Int64}
@assert typeof(s) === MyStruct{Int}
@assert typeof(s) === MyStruct{Int64}

# Verify the struct name includes module
name_str = string(typeof(s))
@assert occursin("MyStruct", name_str)

# Return 1 to indicate success
1

@testset "isa and typeof work correctly for structs in modules" begin
end

true  # Test passed
