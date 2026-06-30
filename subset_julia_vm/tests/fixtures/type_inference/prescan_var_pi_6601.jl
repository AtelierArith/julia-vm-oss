# Issue #6601: the function-body slot-typing pre-scan types a bare `Var`
# RHS (`p = pi`, `g = r`) through the shared inference engine, while the
# bare `pi`/`π` Var keeps the legacy F64 special-case the empty-table engine
# lacks. `area(r) = (p = pi; g = r; p * g * g)` must compute pi * r * r.
function area(r)
    p = pi
    g = r
    return p * g * g
end

@assert area(2.0) == 12.566370614359172
@assert typeof(area(2.0)) === Float64

# `π` (U+03C0) takes the same bare-pi slot-typing path as `pi`.
function area_pi(r)
    p = π
    g = r
    return p * g * g
end

@assert area_pi(2.0) == 12.566370614359172
@assert typeof(area_pi(2.0)) === Float64

println(area(2.0))

true
