using Test

@testset "String index vector uses slice path (Issue #3908)" begin
    indices = Int64[]
    push!(indices, 1)
    push!(indices, 3)

    @test "abcd"[indices] == "ac"
    @test typeof("abcd"[indices]) == String
end

true
