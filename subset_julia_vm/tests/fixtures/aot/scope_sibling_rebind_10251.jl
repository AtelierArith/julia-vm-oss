# Issue #10251 / #10111: same-named locals in sibling lexical scopes (`let`
# blocks and `for` loop bodies) are INDEPENDENT bindings and must each keep
# their own concrete type — they must NOT be unified under the first-seen type.
#
# Upstream `julia` and the sjulia VM both agree here; this fixture locks the VM
# behavior and documents the expected output. The AoT backend regression for
# the `let`-block case is covered by
# `aot_e2e_tests::test_aot_sibling_let_same_name_distinct_types_10251`; the
# AoT `for`-loop case is tracked separately (name-keyed hoisting analysis).

# Two sibling `let` blocks rebinding `r` to different concrete numeric types.
let
    r = Int8(3) + Int8(3)
    println(typeof(r), " ", r)
end
let
    r = UInt8(200) + UInt8(200)
    println(typeof(r), " ", r)
end

# Two sibling `for` loops rebinding `s` to different concrete numeric types.
for _ in 1:1
    s = Int16(5)
    println(typeof(s), " ", s)
end
for _ in 1:1
    s = Float64(2.5)
    println(typeof(s), " ", s)
end

# Self-check: each block observed its own type, not the first-seen one.
checks = String[]
let
    r = Int8(3) + Int8(3)
    push!(checks, string(typeof(r)))
end
let
    r = UInt8(200) + UInt8(200)
    push!(checks, string(typeof(r)))
end
checks == ["Int8", "UInt8"]
