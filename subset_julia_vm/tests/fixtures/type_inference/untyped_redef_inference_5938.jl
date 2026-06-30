using Test

# Issue #5938: an untyped, same-signature method redefinition is a REPLACEMENT
# (last-definition-wins), not an ambiguous overload. Caller inference must stay
# precise instead of collapsing to `Any`.

# Case 1: untyped same-sig redefinition keeps the result type precise.
redef1(x) = x + 1
redef1(x) = x + 100
c1() = redef1(1)

# Case 2: a return-type-changing redefinition is last-wins (Float64, not Int64).
redef2(x) = x + 1
redef2(x) = x + 1.0
c2() = redef2(1)

# Guard: typed multiple dispatch must stay precise and is unaffected by the fix
# (typed signatures route through the method tables, not the untyped function
# table touched by `add_function`).
p_disp(::Int) = 1
p_disp(::Float64) = 2.0
cp() = p_disp(1)

@testset "untyped same-signature redefinition inference (#5938)" begin
    @test Base.infer_return_type(c1, Tuple{}) === Int64
    @test Base.infer_return_type(c2, Tuple{}) === Float64
    @test Base.infer_return_type(cp, Tuple{}) === Int64
end

Base.infer_return_type(c1, Tuple{}) === Int64 &&
    Base.infer_return_type(c2, Tuple{}) === Float64 &&
    Base.infer_return_type(cp, Tuple{}) === Int64
