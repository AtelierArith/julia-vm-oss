# Issue #6697: `Memory{T}` compact display (`print` / `string` / `repr` /
# 2-arg `show(io, ::Memory)`) must render as `[a, b, c]` / `T[]`, matching
# upstream `show(io, ::GenericMemory)`. Previously `print` produced the
# multi-line "N-element Memory{T}:" verbose form and `repr` rendered the empty
# struct-style `Memory{T}()`. Non-implicit eltypes carry the typeinfo prefix
# (`Bool[1, 0]`, `Any[...]`). All assertions match upstream Julia 1.12.

checks = Bool[]

m = Memory{Int64}(undef, 3)
m[1] = 1
m[2] = 2
m[3] = 3
push!(checks, repr(m) == "[1, 2, 3]")
push!(checks, string(m) == "[1, 2, 3]")

# 2-arg show(io, ::Memory) is the compact form
io = IOBuffer()
show(io, m)
push!(checks, String(take!(io)) == "[1, 2, 3]")

# empty memory renders like an empty array
e = Memory{Int64}(undef, 0)
push!(checks, repr(e) == "Int64[]")
push!(checks, string(e) == "Int64[]")

# Bool eltype: typeinfo prefix + 1/0 elements
b = Memory{Bool}(undef, 2)
b[1] = true
b[2] = false
push!(checks, repr(b) == "Bool[1, 0]")

# Float64 eltype is implicit (no prefix)
f = Memory{Float64}(undef, 2)
f[1] = 1.5
f[2] = 2.5
push!(checks, repr(f) == "[1.5, 2.5]")

# Any eltype: explicit prefix, element show-fields (string is quoted)
a = Memory{Any}(undef, 2)
a[1] = 1
a[2] = "x"
push!(checks, repr(a) == "Any[1, \"x\"]")

all(checks)
