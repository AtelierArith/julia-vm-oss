# Issue #6688: `Memory{T}(undef, dims::Tuple)` must accept a 1-tuple of
# dimensions, matching upstream `base/genericmemory.jl`. `Memory` is
# one-dimensional, so `dims` is a 1-tuple; previously sjulia compiled the size
# argument directly to `I64` and failed with "Cannot convert Tuple to I64".
# Both a literal tuple `(n,)` and a value dynamically typed as a tuple are
# supported. All assertions match upstream Julia 1.12.

checks = Bool[]

# --- literal 1-tuple dims (the issue's MWE) -------------------------------
function fillmem(::Type{T}, values) where {T}
    m = Memory{T}(undef, (length(values),))
    for i in 1:length(values)
        m[i] = values[i]
    end
    return m
end
m1 = fillmem(Int64, [10, 20, 30])
push!(checks, length(m1) == 3)
push!(checks, m1[1] == 10 && m1[2] == 20 && m1[3] == 30)

# --- dynamic tuple dims (a `dims::Tuple` variable) -----------------------
function makemem(::Type{T}, n) where {T}
    dims = (n,)
    m = Memory{T}(undef, dims)
    for i in 1:n
        m[i] = T(i)
    end
    return m
end
m2 = makemem(Float64, 4)
push!(checks, length(m2) == 4)
push!(checks, m2[4] == 4.0)

# --- literal tuple wrapping a literal integer ----------------------------
push!(checks, length(Memory{Int64}(undef, (5,))) == 5)

# --- scalar form still works (control) -----------------------------------
push!(checks, length(Memory{Int64}(undef, 6)) == 6)

# --- tuple and scalar forms agree ----------------------------------------
push!(checks, length(Memory{Int64}(undef, (3,))) == length(Memory{Int64}(undef, 3)))

all(checks)
