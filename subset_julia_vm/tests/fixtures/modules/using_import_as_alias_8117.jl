# Issue #8117: `import/using X: y as z` and `import X as Y` must bind the alias
# name to the imported entity (previously the rename was a no-op and the alias
# resolved to nothing — calling it errored "Unknown function").

module Mz
export q, r
q() = 5
r(x) = x * 2
end

module Mw
export w
w() = 99
end

# Selective symbol alias via `using`: binds `qq`/`rr`, not `q`/`r`.
using .Mz: q as qq, r as rr
# Selective symbol alias via `import`.
import .Mw: w as ww
# Whole-module alias via `import`: binds `LA` to the module.
import LinearAlgebra as LA
# Selective stdlib symbol alias.
using LinearAlgebra: dot as ddot
# Base symbol alias: `mysin` resolves to the global `sin`.
import Base: sin as mysin

results = (
    qq(),                        # 5
    rr(10),                      # 20
    ww(),                        # 99
    LA.dot([1, 2, 3], [4, 5, 6]),  # 32
    ddot([1, 2, 3], [4, 5, 6]),    # 32
    mysin(0.0),                  # 0.0
)

println(results)

results == (5, 20, 99, 32, 32, 0.0)
