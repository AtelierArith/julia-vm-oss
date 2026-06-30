# Issue #5120: @NamedTuple{...} and @NamedTuple begin...end macro syntax.
# The macro expands to the NamedTuple type and produces the canonical
# @NamedTuple{a::Int64, b::String} form, interchangeable with the runtime
# named-tuple type (matching upstream Julia 1.12).

# Braces form, with and without explicit field types.
T1 = @NamedTuple{a::Int, b::String}
T2 = @NamedTuple{a::Int, b}
T3 = @NamedTuple{a, b}

# begin ... end block form is equivalent to the braces form.
T4 = @NamedTuple begin
    a::Int
    b::String
end

# Canonical printing matches upstream (Int -> Int64, Any field shown bare).
checks = Bool[]
push!(checks, string(T1) == "@NamedTuple{a::Int64, b::String}")
push!(checks, string(T2) == "@NamedTuple{a::Int64, b}")
push!(checks, string(T3) == "@NamedTuple{a, b}")
push!(checks, string(T4) == "@NamedTuple{a::Int64, b::String}")

# The macro result equals the type of a matching named-tuple value.
push!(checks, T1 === typeof((a = 1, b = "hi")))
push!(checks, T4 === typeof((a = 1, b = "hi")))

# isa against the macro-produced type.
push!(checks, (a = 1, b = "hi") isa @NamedTuple{a::Int, b::String})

# An empty @NamedTuple{} is also a valid type literal.
push!(checks, string(@NamedTuple{}) == "@NamedTuple{}")

# Full type-level dispatch on NamedTuple{names, T} (method matching,
# T(...) construction, `<: NamedTuple` on the concrete form) is tracked by
# Issue #5063 and intentionally not asserted here.

all(checks)
