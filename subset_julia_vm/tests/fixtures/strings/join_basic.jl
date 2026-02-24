# Test join - concatenate collection elements with delimiter

using Test

@testset "join(arr, delim) - Pure Julia (Issue #684)" begin
    @test (join(["a", "b", "c"], ",") == "a,b,c" && join([1, 2, 3], "-") == "1-2-3" && join(["x"], ":") == "x" && join(String[], ",") == "")
end

true  # Test passed
