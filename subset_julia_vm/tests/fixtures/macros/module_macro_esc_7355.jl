# Issue #7355 / #7350 (A4): inside a module-defined macro, `esc(...)` arguments
# resolve in the CALLER scope while non-esc identifiers resolve in the defining
# module. `helper` (non-esc) -> M7355E.helper; `esc(v)` -> caller's `y`.
module M7355E
    export @m
    helper(x) = x * 2
    macro m(v)
        return :(helper($(esc(v))))
    end
end
using .M7355E
y = 21
@m(y) == 42
