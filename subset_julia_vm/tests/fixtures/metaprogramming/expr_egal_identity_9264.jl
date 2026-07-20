# Expr `===` (egal) is object identity; `==` / `isequal` stay structural (Issue #9264)
#
# Upstream `Expr` is a mutable object, so `===` compares object identity: two
# independently-built `:(x + 1)` are NOT egal, but the same object (`a === a`,
# an alias `c = a`) is. Structural `==` / `isequal` remain field-based and stay
# true across independent builds (Issue #9183). This is the regression guard for
# the buggy structural egal that made `:(x + 1) === :(x + 1)` return `true`.
#
# Fixing egal to identity also requires `isequal(::Expr, ::Expr)` to recurse via
# `==` (not `===`) so that a nested Expr element inside another Expr's args
# (reached through `isequal(x.args, y.args)`) keeps structural equality.

# === (egal): object identity ===
a = :(x + 1)
b = :(x + 1)
c = a                      # alias: same object
e1 = (a === b)             # false: independently built
e2 = (a !== b)             # true
e3 = (a === a)             # true: same object
e4 = (a === c)             # true: alias shares the object

# nested Expr in call args
p = :(f(x + 1))
q = :(f(x + 1))

# structural `==` / `isequal` stay true even for independent builds
s1 = (a == b)              # true
s2 = (p == q)              # true: nested structural
s3 = isequal(p, q)         # true: nested structural (via `==` recursion)
s4 = (:(x + 1) == :(x + 2))  # false: differing literal
s5 = isequal(a, b)         # true

# QuoteNode `===` is unaffected (wrapped Symbol is interned)
n1 = (QuoteNode(:x) === QuoteNode(:x))  # true

!e1 && e2 && e3 && e4 && s1 && s2 && s3 && !s4 && s5 && n1
