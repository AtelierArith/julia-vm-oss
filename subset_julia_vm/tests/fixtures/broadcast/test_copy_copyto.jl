using Test

# Test copy and copyto! for Broadcasted (Issue #2541)

@testset "copy for Broadcasted" begin
    # Basic copy: creates new array from Broadcasted
    bc = Broadcasted(+, ([1, 2, 3], [10, 20, 30]))
    result = copy(bc)
    @test length(result) == 3
    @test result[1] == 11
    @test result[2] == 22
    @test result[3] == 33
end

@testset "copyto! for Broadcasted" begin
    # copyto! fills existing array
    bc = Broadcasted(-, ([10, 20, 30], [1, 2, 3]))
    dest = zeros(Int64, 3)
    copyto!(dest, bc)
    @test dest[1] == 9
    @test dest[2] == 18
    @test dest[3] == 27

    # Return value is the destination
    bc2 = Broadcasted(+, ([1, 1, 1], [2, 2, 2]))
    dest2 = zeros(Int64, 3)
    ret = copyto!(dest2, bc2)
    @test ret === dest2
    @test ret[1] == 3
end

true
