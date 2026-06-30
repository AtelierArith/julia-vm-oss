# Issue #7355 / #7350 (A4): a macro defined inside a module is resolvable after
# `using .M`, and its non-esc identifiers resolve in the macro's DEFINING module,
# so it may call an *unexported* helper (`helper` is not exported here).
module M7355U
    export @m
    helper(x) = x * 2     # unexported
    macro m(v)
        return :(helper($v))   # non-esc -> must resolve to M7355U.helper
    end
end
using .M7355U
@m(21) == 42
