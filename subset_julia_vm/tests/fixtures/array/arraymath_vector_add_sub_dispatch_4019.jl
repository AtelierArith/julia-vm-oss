# Kept standalone: adds a method to Base.:+ on a Base argument type
# (`+(::Vector{Int64}, ::Vector{Int64})`), i.e. method piracy. Method-table
# extension is process-global, not module-scoped (even inside a @testset), so
# wrapping this in a module inside an aggregate would leak the pirated `+` to
# every later member that adds integer vectors — and to other aggregates in the
# same test process. Same #5966 class as the dispatch/*_user_method_* fixtures;
# excluded from Issue #10238 module-wrap aggregation.
using Test

@testset "Vector arraymath dispatch-first (Issue #4019)" begin
    @test [1.0, 2.0, 3.0] + [10.0, 20.0, 30.0] == [11.0, 22.0, 33.0]
    @test [10, 20, 30] - [1, 2, 3] == [9, 18, 27]

    Base.:+(a::Vector{Int64}, b::Vector{Int64}) = [4019, length(a), length(b)]
    @test [1, 2] + [3, 4] == [4019, 2, 2]
end

true
