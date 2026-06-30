using Test

import Base: collect

collect(r::UnitRange{Int64}) = 99

runtime_range_collect_4266(x::Any) = collect(x)

@testset "range collect user method dispatch (Issue #4266)" begin
    @test collect(1:3) == 99
    @test runtime_range_collect_4266(1:3) == 99
end

true
