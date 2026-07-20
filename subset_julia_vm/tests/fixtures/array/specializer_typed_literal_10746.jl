# Issue #10746: the runtime specializer has real codegen for type-object-
# prefixed array literals (`Any[x]`, `Float64[...]`) inside specializable
# functions (functions taking array arguments), instead of bailing the whole
# specialization. Output must be identical to upstream either way; this pins
# the literal-build correctness for the shapes the shared element-type map
# covers, including the per-element convert routing (ComplexF64 / Int with
# unsigned literals).

function boxed_first_10746(x)
    boxed = Any[x]
    return boxed[1]
end
@assert boxed_first_10746([1, 2, 3]) == [1, 2, 3]

function float_pair_10746(a)
    v = Float64[a[1], 2]
    return v
end
@assert float_pair_10746([1.5]) == [1.5, 2.0]
@assert typeof(float_pair_10746([1.5])) == Vector{Float64}

function int_from_hex_10746(a)
    v = Int[0x30, a[1]]
    return v
end
@assert int_from_hex_10746([9]) == [48, 9]
@assert typeof(int_from_hex_10746([9])) == Vector{Int64}

function complex_pair_10746(a)
    v = ComplexF64[a[1], 2]
    return v
end
@assert complex_pair_10746([1.0]) == [1.0 + 0.0im, 2.0 + 0.0im]
@assert typeof(complex_pair_10746([1.0])) == Vector{ComplexF64}

function string_pick_10746(a)
    v = String["x", a[1]]
    return v[2]
end
@assert string_pick_10746(["y"]) == "y"

function real_sum_10746(a)
    v = Real[a[1], 2.5]
    return v[1] + v[2]
end
@assert real_sum_10746([1]) == 3.5

# Repeated calls keep hitting the (now cached) specialization.
total = 0
for i in 1:5
    global total += int_from_hex_10746([i])[2]
end
@assert total == 15

println("All specializer typed-literal tests passed")
true
