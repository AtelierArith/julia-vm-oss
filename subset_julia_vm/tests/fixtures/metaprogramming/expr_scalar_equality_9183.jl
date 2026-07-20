# Scalar Expr / QuoteNode structural equality (Issue #9183)
#
# Regression: `:(x::Int) == :(x::Int)` previously failed to COMPILE with
# "Cannot convert Expr to I64" because a statically-typed `Expr` scalar fell
# through the binary compiler's numeric fast path instead of routing to the
# structural `==(::Expr, ::Expr)` / `==(::QuoteNode, ::QuoteNode)` method
# dispatch. Array-of-Expr `==` and field-wise comparison already worked; only
# the scalar static-type path was mis-gated.

e = :(x::Int)

# Expr ==: field-structural, true even for independently built quotes.
r1 = (:(x::Int) == :(x::Int))
r2 = (:(x + 1) == :(x + 1))
r3 = (e == e)
r4 = (:(x + 1) == :(x + 2))                # false: differing literal
r5 = (:(x + 1) != :(x + 2))                # true
r6 = (:(x + 1) != :(x + 1))                # false
r7 = (:(f(x + 1, y)) == :(f(x + 1, y)))    # nested Expr in args
r8 = (:(f(x + 1, y)) == :(f(x + 1, z)))    # false: nested differs

# QuoteNode ==: compares the wrapped value.
q1 = (QuoteNode(:x) == QuoteNode(:x))
q2 = (QuoteNode(:x) == QuoteNode(:y))      # false

# Mixed-type `==`/`!=`: an AST datatype compared against a value with no
# specific `==` method falls back to upstream's `==(x, y) = x === y`, so the
# result is `false`, NOT a "Cannot convert Expr to I64" compile error. These
# are the sibling cases of the scalar-Expr gap (same numeric-coercion root
# cause reachable through a mixed operand pair). Verified against upstream
# julia 1.12: every m* below matches.
m1 = (:(x + 1) == 5)                       # false: Expr vs Int
m2 = (5 == :(x + 1))                       # false: Int vs Expr
m3 = (:(x + 1) == QuoteNode(:x))           # false: Expr vs QuoteNode (no method)
m4 = (QuoteNode(:x) == :(x + 1))           # false: QuoteNode vs Expr
m5 = (:(x + 1) != 5)                       # true
m6 = (:(x + 1) == 5.0)                     # false: Expr vs Float64
m7 = (:(x + 1) == :sym)                    # false: Expr vs Symbol
m8 = (:(x + 1) == "s")                     # false: Expr vs String

# hash / === sanity.
h1 = (hash(:(x + 1)) == hash(:(x + 1)))
s1 = (QuoteNode(:x) === QuoteNode(:x))

r1 && r2 && r3 && !r4 && r5 && !r6 && r7 && !r8 && q1 && !q2 &&
    !m1 && !m2 && !m3 && !m4 && m5 && !m6 && !m7 && !m8 && h1 && s1
