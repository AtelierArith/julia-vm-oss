# Issue #7029: the quote→code path must round-trip broadcast calls (`f.(x)`,
# `x .- y`), keyword arguments (`f(x; k=v)`), and string interpolation (`"…$x…"`).
# Previously each raised `quote for {broadcast_call,keyword_argument} not yet
# supported` / silently froze the interpolation as literal text. Surfaced by
# `@gif`/`@animate` over `plot(x, sin.(x .- t), title="t=$t")` (Issue #7030).

# A macro that re-emits its argument via esc forces the body through the quote path.
macro passthru(e)
    quote
        $(esc(e))
    end
end

# Broadcast call + broadcast binary round-trip and evaluate element-wise.
v = @passthru(sin.([0.0, 0.0] .+ 0.0))
ok_broadcast = v == [0.0, 0.0]

# Dotted short-circuit broadcasts (`.&&` / `.||`) must map to the andand/oror
# wrappers (`&&`/`||` are syntax, not functions), matching the binary lowering path.
ok_sc = (@passthru([true, false, true] .&& [true, true, false])) == [true, false, false]
ok_or = (@passthru([true, false, false] .|| [false, false, true])) == [true, false, true]

# String interpolation round-trips (not frozen as the literal "t=$t").
t = 3
s = @passthru("t=$t")
ok_interp = s == "t=3"

# Keyword argument round-trips through a quoted call, in both the comma form
# (`f(a, b=v)`, what `plot(x, y, title=…)` uses) and the semicolon form (`f(a; b=v)`).
kwfun(a; b = 0) = a + b
ok_kwarg_comma = (@passthru(kwfun(10, b = 5))) == 15
ok_kwarg_semi = (@passthru(kwfun(10; b = 5))) == 15

ok_broadcast && ok_sc && ok_or && ok_interp && ok_kwarg_comma && ok_kwarg_semi
