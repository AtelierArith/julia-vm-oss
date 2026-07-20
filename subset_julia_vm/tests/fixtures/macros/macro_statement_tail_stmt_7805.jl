using Test

# Issue #7805: a macro expanded in statement position may return an outer block
# whose final form is statement-only (`Expr(:function, ...)` or `Expr(:+=, ...)`).
# The value-preserving statement-position path must fall back to statement
# lowering for these tails instead of forcing them through value lowering.

macro stmt_tail_defadd_7805()
    fname = esc(:adder_7805)
    quote
        function $fname(a, b)
            a + b
        end
    end
end

macro stmt_tail_incr_7805(x)
    quote
        $(esc(x)) += 10
    end
end

@stmt_tail_defadd_7805
y_7805 = 5
@stmt_tail_incr_7805 y_7805

@testset "macro statement block ending in stmt-only head (Issue #7805)" begin
    @test adder_7805(2, 3) == 5
    @test y_7805 == 15
end

adder_7805(2, 3) == 5 && y_7805 == 15
