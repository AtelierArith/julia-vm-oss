# Issue #7355 / #7350 (A4): a module-defined macro is callable via a qualified
# `M.@m(...)` call, and its non-esc helper resolves in the defining module.
module M7355Q
    helper(x) = x * 2
    macro m(v)
        return :(helper($v))
    end
end
using .M7355Q
M7355Q.@m(21) == 42
