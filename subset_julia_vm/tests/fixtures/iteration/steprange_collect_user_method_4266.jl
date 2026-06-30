using Test

import Base: collect

collect(r::StepRange{Int64, Int64}) = :step_override

runtime_step_collect_4266(x::Any) = collect(x)

@testset "StepRange collect user method dispatch (Issue #4266)" begin
    @test collect(1:2:7) == :step_override
    @test runtime_step_collect_4266(1:2:7) == :step_override
end

true
