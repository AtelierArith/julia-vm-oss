# Issue #7856: a nested macrocall expanded during macro-result conversion
# (`evaluate_macro_from_value_args`) ran its own VM but returned the value
# without dereferencing `StructRef` heap handles. MacroTools' `splitdef`,
# `isshortdef` and `splitarg` use OR-pattern `@capture`/`@match`
# (e.g. `function (fcall_ | fcall_) body_ end`); the OR alternative is an
# `OrBind` struct that surfaced as an unresolved `StructRef`, tripping
# `value_to_literal` with "macro expansion cannot quote value type Any" while
# loading `utils.jl`. With the struct-heap resolution restored on the nested
# path, the whole MacroTools package loads and these helpers run.
using MacroTools: splitdef, splitarg, isshortdef

d = splitdef(:(f(x, y::Int) = x + y))
ok1 = d[:name] === :f && length(d[:args]) == 2

ok2 = isshortdef(:(g(z) = z)) === true
ok3 = isshortdef(:(h)) === false

(name, typ, splat, default) = splitarg(:(a::Int = 3))
ok4 = name === :a && typ === :Int && splat === false && default == 3

ok1 && ok2 && ok3 && ok4
