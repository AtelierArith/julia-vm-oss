# Issue #7992: a method with BOTH an optional positional argument and a keyword
# argument must thread an explicitly-passed keyword through the auto-generated
# reduced-arity forwarder.
#
# Root cause: the optional-positional desugaring synthesizes a lower-arity stub
# (`h(a; c=...) = h(a, 10; c=...)`) that re-passes the positional defaults but
# dropped the keyword arguments, so the full method re-applied the keyword's
# *default* instead of the value the caller actually passed:
#   h(a, b=10; c=100) = a + b + c
#   h(5; c=0)  # upstream: 15   sjulia (before fix): 115  <- c=0 dropped
#
# Verified against upstream Julia 1.12 before implementation.

using Test

# --- the reported MWE: one optional positional + one keyword -----------------
h_7992(a, b=10; c=100) = a + b + c

@testset "kwargs_optional_positional_keyword_thread_7992: single optional + kw" begin
    @test h_7992(5) == 115            # both positional default + kw default
    @test h_7992(5, 1) == 106         # positional supplied, kw default
    @test h_7992(5; c=0) == 15        # reduced arity, kw explicitly passed
    @test h_7992(5, 1; c=0) == 6      # full arity, kw explicitly passed
end

# --- two optional positionals + multiple keywords ----------------------------
g_7992(a, b=2, d=3; c=100, e=5) = a + b + d + c + e

@testset "kwargs_optional_positional_keyword_thread_7992: multi optional + multi kw" begin
    @test g_7992(1) == 111            # 1+2+3+100+5
    @test g_7992(1; c=0) == 11        # 1+2+3+0+5
    @test g_7992(1; c=0, e=0) == 6    # 1+2+3+0+0
    @test g_7992(1, 20; c=0) == 29    # 1+20+3+0+5
    @test g_7992(1, 20, 30; e=50) == 201 # 1+20+30+100+50
end

# --- varargs keyword forwarding through the reduced-arity stub ----------------
k_7992(a, b=10; kw...) = (a, b, length(kw))

@testset "kwargs_optional_positional_keyword_thread_7992: varargs kw forwarding" begin
    @test k_7992(3) == (3, 10, 0)
    @test k_7992(3; x=1, y=2) == (3, 10, 2)
    @test k_7992(3, 20; x=1) == (3, 20, 1)
end

# --- side-effecting keyword default interacts with the stub (Issue #5121) -----
# When the keyword is omitted the default must still be re-evaluated per call;
# when supplied, the explicit value wins.
counter_7992 = Ref(0)
next_7992!() = (counter_7992[] += 1; counter_7992[])
m_7992(a, b=10; c=next_7992!()) = a + b + c

@testset "kwargs_optional_positional_keyword_thread_7992: side-effecting default" begin
    @test m_7992(1) == 12             # c=1
    @test m_7992(1) == 13             # c=2
    @test m_7992(1; c=100) == 111     # explicit value wins, default not run
    @test m_7992(1) == 14             # c=3 (counter only advanced on omitted calls)
end

true
