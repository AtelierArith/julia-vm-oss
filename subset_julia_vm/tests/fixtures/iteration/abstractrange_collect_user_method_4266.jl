using Test

import Base: collect

collect(r::AbstractRange) = :abstract_override

runtime_abstract_range_collect_4266(x::Any) = collect(x)

@testset "AbstractRange collect user method dispatch (Issue #4266)" begin
    @test collect(1:3) == :abstract_override
    @test runtime_abstract_range_collect_4266(1:3) == :abstract_override
    @test collect(1:2:7) == :abstract_override
    @test runtime_abstract_range_collect_4266(1:2:7) == :abstract_override
    @test collect(1.0:0.5:2.0) == :abstract_override
    @test runtime_abstract_range_collect_4266(1.0:0.5:2.0) == :abstract_override
end

true
