# Issue #5600: a user function's inferred exception type must be COMPOSED from
# the exceptions thrown by the operations and callees in its body, instead of
# the name-table miss that widened every user function to `Union{}`. The
# composition is type-aware and conservative — it only reports an exception when
# the operand types make it certain upstream throws (so a constant tuple index,
# a Complex `sqrt`, or pure arithmetic stay `Union{}`), and a `try` with a
# handler suppresses the protected block.

using Test

f_sqrt(x) = sqrt(x)
m_multi(a, b, x) = div(a, b) + sqrt(x)
g_gcd(a, b) = gcd(a, b)
k_index(v, i) = v[i]
h_pure(x) = x + 1
leaf(x) = sqrt(x)
caller(x) = leaf(x) + 1            # user -> user composition
deep_a(x) = sqrt(x)
deep_b(x) = deep_a(x)
deep_c(x) = deep_b(x)              # 2-level recursion
safe(x) = try sqrt(x) catch; 0.0 end   # try/catch suppression
t_index(t) = t[1]                  # constant tuple index does NOT throw

@testset "interprocedural exception-type composition (Issue #5600)" begin
    @test Base.infer_exception_type(f_sqrt, Tuple{Float64}) == DomainError
    @test Base.infer_exception_type(m_multi, Tuple{Int64,Int64,Float64}) ==
          Union{DivideError,DomainError}
    @test Base.infer_exception_type(g_gcd, Tuple{Int64,Int64}) == OverflowError
    @test Base.infer_exception_type(k_index, Tuple{Vector{Int64},Int64}) == BoundsError

    # composes through user callees, recursively
    @test Base.infer_exception_type(caller, Tuple{Float64}) == DomainError
    @test Base.infer_exception_type(deep_c, Tuple{Float64}) == DomainError

    # provably-total bodies, suppressed handlers and static-size indexing stay Union{}
    @test Base.infer_exception_type(h_pure, Tuple{Int64}) == Union{}
    @test Base.infer_exception_type(safe, Tuple{Float64}) == Union{}
    @test Base.infer_exception_type(t_index, Tuple{Tuple{Int64,Int64}}) == Union{}

    # the `nothrow` effect is cleared for a throwing body and kept for a total one
    @test Base.infer_effects(f_sqrt, Tuple{Float64}).nothrow == false
    @test Base.infer_effects(h_pure, Tuple{Int64}).nothrow == true
    @test Base.infer_effects(m_multi, Tuple{Int64,Int64,Float64}).nothrow == false
end

true
