using Test

# Issue #9400: a top-level call must resolve against the methods visible at
# that source point, not against the final method-table state after later
# definitions in the same script.

source_redef_9400(x) = x + 1
first_redef_9400 = source_redef_9400(1)
source_redef_9400(x) = x + 100
second_redef_9400 = source_redef_9400(1)

source_specific_9400(x::Real) = 1
first_specific_9400 = source_specific_9400(1)
source_specific_9400(x::Int64) = 2
second_specific_9400 = source_specific_9400(1)

@testset "source-order direct call redefinition (#9400)" begin
    @test first_redef_9400 == 2
    @test second_redef_9400 == 101
    @test first_specific_9400 == 1
    @test second_specific_9400 == 2
end

true
