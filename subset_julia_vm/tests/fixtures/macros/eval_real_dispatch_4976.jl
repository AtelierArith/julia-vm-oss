# Issue #4976: eval() should dispatch real functions / constructors
# instead of relying on a hand-written mini-interpreter that only knows a
# handful of operators.

# Built-in unary function via real dispatch
@assert eval(Meta.parse("identity(5)")) == 5
@assert eval(Meta.parse("identity(5)")) isa Int

# Val(x) convenience constructor: Val(x) -> Val{x}()
# The callee Symbol :Val resolves through normal dispatch.
@assert eval(Meta.parse("Val(3)")) == Val{3}()
@assert eval(Meta.parse("Val(3)")) isa Val{3}

# Val{N}() — callee is a :curly Expr (the original failing case).
@assert eval(Meta.parse("Val{3}()")) isa Val{3}

# Still works for the original mini-interpreter arithmetic paths.
@assert eval(Meta.parse("1 + 2")) == 3
@assert eval(:(2 * 3)) == 6

# A user-defined function dispatched from eval.
add100(x) = x + 100
@assert eval(Meta.parse("add100(5)")) == 105

42.0
