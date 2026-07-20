# Macro quote bodies: filtered and multi-binding generators/comprehensions
# (Issue #10923, dynamic macro path; follow-up to #10626)
#
# The quote constructor builds upstream's Expr(:filter, cond, binding...)
# and Expr(:flatten, nested-generator) shapes, and the dynamic macro
# expansion maps them onto the IR's filter slot and MultiComprehension
# (comma = cartesian, whitespace = flatten).

using Test

macro filt()
    quote
        [x for x in 1:6 if isodd(x)]
    end
end

macro multi_flatten()
    quote
        [x * y for x in 1:2 for y in 1:2]
    end
end

macro multi_comma()
    quote
        [10x + y for x in 1:2, y in 1:3]
    end
end

macro flatten_filtered()
    quote
        [x * y for x in 1:3 for y in 1:3 if x != y]
    end
end

macro lazy_filtered()
    quote
        sum(x for x in 1:10 if iseven(x))
    end
end

@testset "macro-returned generator forms" begin
    @test @filt() == [1, 3, 5]
    @test @multi_flatten() == [1, 2, 2, 4]
    r = @multi_comma()
    @test size(r) == (2, 3)
    @test r[2, 3] == 23
    @test @flatten_filtered() == [2, 3, 2, 6, 3, 6]
    @test @lazy_filtered() == 30
end

true
