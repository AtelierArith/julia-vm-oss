# Docstring storage and retrieval through @doc (Issue #8997)

using Test

"mydoc for f"
f(x) = x

@testset "docstring retrieval" begin
    @test occursin("mydoc for f", string(@doc(f)))
end

true
