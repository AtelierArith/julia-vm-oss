# Issue #6685: `==` on tuples / named tuples must compare struct elements by
# value, not by heap identity. `OneTo` (and other immutable structs) are stored
# as heap struct references (`Value::StructRef`); the native tuple-`==` fold
# previously compared two separately constructed but equal structs by their heap
# index, so `(OneTo(3),) == (OneTo(3),)` was `false` even though
# `OneTo(3) == OneTo(3)` (direct dispatch) was `true`. `isequal` was unaffected
# (it folds `isequal` through real dispatch, not the native `==` fold).
# All assertions match upstream Julia 1.12.

checks = Bool[]

# --- OneTo struct elements in tuples -------------------------------------
push!(checks, (Base.OneTo(3),) == (Base.OneTo(3),))
push!(checks, (Base.OneTo(2), Base.OneTo(2)) == (Base.OneTo(2), Base.OneTo(2)))
push!(checks, !((Base.OneTo(3),) == (Base.OneTo(4),)))
push!(checks, !((Base.OneTo(3), Base.OneTo(2)) == (Base.OneTo(3), Base.OneTo(9))))

# --- mixed struct + primitive elements -----------------------------------
push!(checks, (Base.OneTo(3), 5) == (Base.OneTo(3), 5))
push!(checks, !((Base.OneTo(3), 5) == (Base.OneTo(3), 6)))

# --- nested tuples of structs fold `==` recursively ----------------------
push!(checks, ((Base.OneTo(3),),) == ((Base.OneTo(3),),))
push!(checks, !(((Base.OneTo(3),),) == ((Base.OneTo(4),),)))

# --- UnitRange struct elements -------------------------------------------
push!(checks, (1:3,) == (1:3,))
push!(checks, !((1:3,) == (1:4,)))

# --- named tuples of structs ---------------------------------------------
push!(checks, (a = Base.OneTo(3),) == (a = Base.OneTo(3),))
push!(checks, !((a = Base.OneTo(3),) == (a = Base.OneTo(4),)))
push!(checks, (a = Base.OneTo(3), b = 1:2) == (a = Base.OneTo(3), b = 1:2))

# --- Complex (immutable struct) elements remain correct ------------------
push!(checks, (1 + 2im,) == (1 + 2im,))
push!(checks, !((1 + 2im,) == (1 + 3im,)))

# --- the `axes(A)` use-case from the issue -------------------------------
push!(checks, axes([10, 20, 30]) == (Base.OneTo(3),))

# --- `!=` mirrors `==` ----------------------------------------------------
push!(checks, !((Base.OneTo(3),) != (Base.OneTo(3),)))
push!(checks, (Base.OneTo(3),) != (Base.OneTo(4),))

all(checks)
