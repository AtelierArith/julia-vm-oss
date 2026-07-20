# Issues #10945 / #11008 / #11009: upstream's `(global local)` parser arm
# routes through `parse-eq`, so a full expression follows `global`/`local`.
# These are the VALID runtime forms; invalid scoped forms (module/control
# flow/jump statements after the keyword) now parse and are rejected at
# lowering with upstream's `invalid syntax in "global" declaration` error
# (covered by parser/lowering unit tests, not runnable fixtures).

# Short-form method definition after `global` (Issue #11008).
global scope_short_global_10945(x) = 2x
@assert scope_short_global_10945(3) == 6

# Trailing `= rhs` distributes over the comma list (Issue #11009).
global scope_tuple_x_10945, scope_tuple_y_10945 = 1, 2
@assert scope_tuple_x_10945 == 1
@assert scope_tuple_y_10945 == 2

# Parenthesized destructuring form.
global (scope_pair_a_10945, scope_pair_b_10945) = (3, 4)
@assert scope_pair_a_10945 == 3
@assert scope_pair_b_10945 == 4

# RHS precedence tiers survive the declaration wrapper: ternary, nested
# assignment, and pair RHS.
global scope_ternary_10945 = true ? 1 : 2
@assert scope_ternary_10945 == 1

global scope_nested_lhs_10945 = scope_nested_rhs_10945 = 5
@assert scope_nested_lhs_10945 == 5
@assert scope_nested_rhs_10945 == 5

global scope_pair_10945 = :a => 1
@assert scope_pair_10945 == (:a => 1)

# Typed declared name with initializer.
global scope_typed_10945::Int = 7
@assert scope_typed_10945 == 7

# Bare declarations still work, including the multi-name form.
global scope_bare_a_10945, scope_bare_b_10945
scope_bare_a_10945 = 8
scope_bare_b_10945 = 9
@assert scope_bare_a_10945 + scope_bare_b_10945 == 17

# `local` inside a block distributes the comma list the same way.
let
    local la, lb = 10, 11
    @assert la == 10
    @assert lb == 11
end

true
