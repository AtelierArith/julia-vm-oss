using Test

import Base: *, ==, +

@testset "operator import list accepts comparison operators mid-list (Issue #6544)" begin
    @test 2 * 3 == 6
    @test 2 + 3 == 5
    @test true
end

true
