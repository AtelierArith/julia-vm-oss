# Issue #5014: eval-initiated VM dispatch is depth-bounded so that an
# eval-driven self-recursion fails safely with a StackOverflowError instead of
# crashing the host process. This fixture is the positive regression guard:
# a *bounded* eval that dispatches into a user function through the real VM
# call path must still work after the depth guard was added.
#
# `g(5)` returns `eval(Meta.parse("g(0)")) + 5`, i.e. 42 + 5 = 47. The inner
# `eval` re-enters the interpreter on a fresh `g` frame (the same code slot as
# the enclosing call), exercising the re-entrancy-safe dispatch path.
function g(n)
    if n <= 0
        return 42
    end
    return eval(Meta.parse("g(0)")) + n
end

g(5)
