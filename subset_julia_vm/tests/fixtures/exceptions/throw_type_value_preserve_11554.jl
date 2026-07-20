# throw(value) preserves the exact thrown value for ANY value (not just
# Exception subtypes) — matching upstream, where `catch` binds the original
# value verbatim instead of coercing it into ErrorException(string(value))
# (Issue #11554).

# Shape 1: throwing a bare Type preserves the DataType identity.
T1 = nothing
try
    throw(Int32)
catch T
    global T1 = T
end
@assert typeof(T1) == DataType "expected DataType, got $(typeof(T1))"
@assert T1 === Int32 "expected Int32, got $T1"

# Shape 2: downstream use — the caught Type value is usable to build a real
# parametric type, matching upstream (Vector{T} builds Vector{Int32}, and an
# Int32 vector isa that type). NOTE: defining a *method* whose signature
# references this local-scope T (e.g. `f(x::Vector{T}) = 1`) hits a separate,
# pre-existing local-scope parametric-dispatch gap — not this throw/catch
# value-preservation bug — tracked as Issue #11574; that dispatch shape is
# intentionally NOT exercised here.
VT = nothing
isa_result = nothing
try
    throw(Int32)
catch T
    global VT = Vector{T}
    global isa_result = Int32[1, 2] isa VT
end
@assert VT === Vector{Int32} "expected Vector{Int32}, got $VT"
@assert isa_result == true "expected Int32[1,2] isa Vector{T} to be true"

# Non-Type values are also preserved verbatim (general fix, not type-specific).
for v in (42, 3.14, :sym, (1, 2), [1, 2, 3])
    caught = nothing
    try
        throw(v)
    catch e
        caught = e
    end
    @assert caught === v || caught == v "expected $(typeof(v)) value $v preserved, got $(typeof(caught)) $caught"
    @assert typeof(caught) == typeof(v) "expected typeof $(typeof(v)), got $(typeof(caught))"
end

# Normal Exception throwing is unchanged: a struct exception subtype is still
# caught as itself, not double-wrapped.
caught_struct = nothing
try
    throw(ErrorException("boom"))
catch e
    global caught_struct = e
end
@assert caught_struct isa ErrorException "expected ErrorException, got $(typeof(caught_struct))"
@assert caught_struct.msg == "boom" "expected msg boom, got $(caught_struct.msg)"

# error(msg) still produces ErrorException (unchanged from throw(value)).
caught_err = nothing
try
    error("classic error")
catch e
    global caught_err = e
end
@assert caught_err isa ErrorException "expected ErrorException, got $(typeof(caught_err))"
@assert caught_err.msg == "classic error" "expected msg classic error, got $(caught_err.msg)"

println("throw_type_value_preserve_11554: all assertions passed")
true
