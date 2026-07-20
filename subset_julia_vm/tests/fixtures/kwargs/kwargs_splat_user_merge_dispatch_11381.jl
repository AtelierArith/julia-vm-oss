using Test

# Keyword splatting must dispatch through real `merge(::NamedTuple, source)`
# multiple dispatch, so a user-defined `Base.merge` extension actually runs
# instead of being silently ignored (Issue #11381).

struct K11381 end
Base.merge(a::NamedTuple, ::K11381) = merge(a, (hacked = 1,))

f(; options...) = options

@testset "kwargs splat user-defined merge dispatch (Issue #11381)" begin
    kw = f(; K11381()...)
    @test collect(keys(kw)) == [:hacked]
    @test kw[:hacked] == 1
end

true
