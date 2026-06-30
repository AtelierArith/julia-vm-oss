# Issue #7870 regression guard: a macro returning a STANDALONE subtype/supertype
# constraint. After #7863 the quote path emits the operator head
# (`Expr(:<:, a, b)` / `Expr(:>:, a, b)`) instead of `Expr(:call, :<:, ...)`, so
# the runtime macro-return converter must lower these like source `a <: b` (a
# `BinaryOp::Subtype`, with `>:` swapped) — not as an unknown `<:`/`>:` function or
# an "unsupported Expr head" lowering error. Verified against upstream julia 1.12.
macro issub(a, b)
    :($a <: $b)
end
macro issuper(a, b)
    :($a >: $b)
end

@assert (@issub Int Real) == true
@assert (@issub Real Int) == false
@assert (@issub Int Integer) == true
@assert (@issuper Real Int) == true
@assert (@issuper Int Real) == false

# also exercise it in value position assigned to a variable
r = @issub Float64 Real
@assert r == true

(@issub Int Real) && !(@issub Real Int) && (@issuper Real Int)
