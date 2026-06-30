# Issue #8183: dense Float64 scalar `while` loops (ODE Euler step / iterated map)
# now run on the native typed-loop fast path (`vm::executable`). This exercises
# the new typed-loop ops (`DivF64`, `ModI64`, fused `LoadMulI64Slot`, unary
# `NegF64`) and the raised op/slot caps, in both the type-annotated form (matched
# statically) and the bare form (matched after runtime specialization). All must
# stay bit-identical to upstream Julia.
#
# NB: the final value IS the conjunction of every check, so the nextest fixture
# harness fails on any regression (sjulia's `@testset` does not throw on a failed
# `@test`, so a fixture ending in `true` would pass regardless).

# --- Aizawa attractor (explicit Euler); checksum = Σ(x+y+z) ---
function aizawa_untyped(n)
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    i = 0
    while i < n
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt; y = y + dy * dt; z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
        i = i + 1
    end
    (sx + sy) + sz
end

function aizawa_typed(n::Int64)::Float64
    a::Float64 = 0.95; b::Float64 = 0.7; c::Float64 = 0.6
    d::Float64 = 3.5; e::Float64 = 0.25; g::Float64 = 0.1
    dt::Float64 = 0.01
    x::Float64 = 0.1; y::Float64 = 0.0; z::Float64 = 0.0
    sx::Float64 = 0.0; sy::Float64 = 0.0; sz::Float64 = 0.0
    i::Int64 = 0
    while i < n
        dx::Float64 = (z - b) * x - d * y
        dy::Float64 = d * x + (z - b) * y
        dz::Float64 = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt; y = y + dy * dt; z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
        i = i + 1
    end
    (sx + sy) + sz
end

# --- IFS fractal (Barnsley fern) with a glibc-style LCG; checksum = Σ(x+y) ---
function ifs_untyped(n)
    seed = 1; x = 0.0; y = 0.0; sx = 0.0; sy = 0.0; i = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        r = seed / 2147483648.0
        nx = 0.0; ny = 0.0
        if r < 0.01
            nx = 0.0; ny = 0.16 * y
        elseif r < 0.86
            nx = 0.85 * x + 0.04 * y; ny = (-0.04 * x + 0.85 * y) + 1.6
        elseif r < 0.93
            nx = 0.2 * x - 0.26 * y; ny = (0.23 * x + 0.22 * y) + 1.6
        else
            nx = -0.15 * x + 0.28 * y; ny = (0.26 * x + 0.24 * y) + 0.44
        end
        x = nx; y = ny; sx = sx + x; sy = sy + y; i = i + 1
    end
    sx + sy
end

function ifs_typed(n::Int64)::Float64
    seed::Int64 = 1
    x::Float64 = 0.0; y::Float64 = 0.0; sx::Float64 = 0.0; sy::Float64 = 0.0
    i::Int64 = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        r::Float64 = seed / 2147483648.0
        nx::Float64 = 0.0; ny::Float64 = 0.0
        if r < 0.01
            nx = 0.0; ny = 0.16 * y
        elseif r < 0.86
            nx = 0.85 * x + 0.04 * y; ny = (-0.04 * x + 0.85 * y) + 1.6
        elseif r < 0.93
            nx = 0.2 * x - 0.26 * y; ny = (0.23 * x + 0.22 * y) + 1.6
        else
            nx = -0.15 * x + 0.28 * y; ny = (0.26 * x + 0.24 * y) + 0.44
        end
        x = nx; y = ny; sx = sx + x; sy = sy + y; i = i + 1
    end
    sx + sy
end

# Reference checksums from upstream Julia 1.12. Each check `&&`s into `ok`; the
# final value is the conjunction. (A multi-line typed array literal `Bool[…]`
# would be cleaner but currently fails to parse in sjulia — #8188.)
ok = true
ok = ok && (aizawa_untyped(1000) === 400.67026866608416)
ok = ok && (aizawa_typed(1000) === 400.67026866608416)
ok = ok && (aizawa_untyped(1000) === aizawa_typed(1000))
ok = ok && (ifs_untyped(1000) === 7135.354951472622)
ok = ok && (ifs_typed(1000) === 7135.354951472622)
ok = ok && (ifs_untyped(1000) === ifs_typed(1000))
# A short run exercises the same ops without relying on warm specialization.
ok = ok && (aizawa_typed(7) === aizawa_untyped(7))
ok = ok && (ifs_typed(7) === ifs_untyped(7))
ok
