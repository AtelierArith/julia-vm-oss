# Test map! function (in-place map)
# map!(f, dest, src) applies f to each element of src and stores in dest

using Test

@testset "map! - in-place map operation (Issue #351)" begin

    # Basic map!
    src = [1.0, 2.0, 3.0]
    dest = [0.0, 0.0, 0.0]
    map!(x -> x * 2, dest, src)
    check1 = dest[1] == 2.0 && dest[2] == 4.0 && dest[3] == 6.0

    # map! with squares
    src2 = [2.0, 3.0, 4.0]
    dest2 = [0.0, 0.0, 0.0]
    map!(x -> x^2, dest2, src2)
    check2 = dest2[1] == 4.0 && dest2[2] == 9.0 && dest2[3] == 16.0

    @test (check1 && check2)
end

true  # Test passed
