# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: type_inference/branch_local_widening.jl =====
module Agg_branch_local_widening
# Branches that assign different incompatible types must produce a value
# whose runtime type matches the branch taken (not the last syntactic branch).
# Issues #3535 and #3536

using Test

function f3535(c)
    x = 1
    if c
        x = "one"
    end
    return x
end

function f3536(c)
    if c
        x = 1
    else
        x = "s"
    end
    return x
end

@testset "Branch-local type widening preserves both branches" begin
    @test f3535(false) == 1
    @test f3535(true) == "one"

    @test f3536(true) == 1
    @test f3536(false) == "s"
end
end # module Agg_branch_local_widening

# ===== source: type_inference/conditional_narrowing.jl =====
module Agg_conditional_narrowing
# Test: Conditional type narrowing
# Tests that type information flows through conditional branches
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function numeric_check(x)
    # Test conditional narrowing with numeric comparison
    if x > 0
        x * 2  # x is known to be positive numeric here
    else
        0
    end
end

function string_length_check(s)
    # Test conditional with string length
    if length(s) > 0
        length(s)
    else
        -1
    end
end

function bool_check(flag)
    # Test boolean conditional
    if flag
        1
    else
        0
    end
end

function comparison_narrowing(x, y)
    # Test narrowing from comparison
    if x > y
        x - y
    else
        y - x
    end
end

@testset "Conditional type narrowing" begin
    # Numeric conditional tests
    @test numeric_check(5) == 10
    @test numeric_check(-3) == 0
    @test numeric_check(0) == 0
    
    # String length conditional tests
    @test string_length_check("hello") == 5
    @test string_length_check("") == -1
    
    # Boolean conditional tests
    @test bool_check(true) == 1
    @test bool_check(false) == 0
    
    # Comparison narrowing tests
    @test comparison_narrowing(10, 3) == 7
    @test comparison_narrowing(3, 10) == 7
end
end # module Agg_conditional_narrowing

# ===== source: type_inference/isa_flow_narrowing_codegen_5181.jl =====
module Agg_isa_flow_narrowing_codegen_5181
# Issue #5181: flow-sensitive `isa` narrowing wired into branch codegen.
# Inside an `isa`-guarded then-branch the guarded variable is refined to a
# concrete type so typed loads / arithmetic specialize. These tests lock in
# that the optimization preserves Julia semantics across the tricky cases:
# reassignment inside the branch, `&&` chains, struct guards, and the negation
# (else) path that must NOT be narrowed.
using Test

# A union-ish `Any` argument narrowed to Int64 in the then-branch.
function add_if_int(x)
    if x isa Int64
        return x + x
    end
    return -1
end

# Float64 narrowing keeps float arithmetic / typeof.
function double_if_float(x)
    if x isa Float64
        return x * 2.0
    end
    return 0.0
end

# Reassigning the narrowed variable inside the branch must persist past it
# (Julia variables are function-scoped). The narrowed type must not clobber the
# reassignment's type.
function reassign_in_branch(x)
    if x isa Int64
        x = "now a string"
    end
    return x
end

# `&&` chain narrows both operands; the second guard re-narrows the same var.
function chained_guard(x)
    if x isa Int64 && x > 0
        return x * 10
    end
    return 0
end

# The else branch must keep dynamic behavior (no narrowing leaks).
function classify(x)
    if x isa Int64
        return 1
    else
        # x is still `Any` here — calling a generic op must dispatch at runtime.
        return string(x)
    end
end

# String guard narrows to a String slot.
function shout_if_string(x)
    if x isa String
        return x * "!"
    end
    return "?"
end

@testset "isa flow narrowing codegen (Issue #5181)" begin
    @test add_if_int(21) == 42
    @test add_if_int(3.5) == -1
    @test add_if_int("x") == -1

    @test double_if_float(2.5) == 5.0
    @test typeof(double_if_float(2.5)) === Float64
    @test double_if_float(3) == 0.0

    @test reassign_in_branch(7) == "now a string"
    @test reassign_in_branch(2.0) == 2.0
    @test reassign_in_branch("keep") == "keep"

    @test chained_guard(5) == 50
    @test chained_guard(-5) == 0
    @test chained_guard("x") == 0

    @test classify(9) == 1
    @test classify(2.0) == "2.0"

    @test shout_if_string("hi") == "hi!"
    @test shout_if_string(3) == "?"
end
end # module Agg_isa_flow_narrowing_codegen_5181

# ===== source: type_inference/nothing_narrowing.jl =====
module Agg_nothing_narrowing
# Test: Type narrowing with === nothing checks
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function safe_increment(x)
    if x === nothing
        return 0  # x is Nothing here
    else
        return x + 1  # x is non-Nothing here
    end
end

@testset "Nothing narrowing" begin
    @test safe_increment(nothing) == 0
    @test safe_increment(5) == 6
end
end # module Agg_nothing_narrowing

# ===== source: type_inference/number_isa_narrowing_5941.jl =====
module Agg_number_isa_narrowing_5941
using Test

# Issue #5941: a value passed to an abstract-numeric parameter (`x::Number`,
# `x::Real`) is statically represented as `ValueType::F64` in the compiler's
# locals (`type_helpers.rs`: `JuliaType::Number => ValueType::F64`), while an
# Int64-valued `x::Integer` becomes `ValueType::I64`. The compile-time `isa`
# folding (`compile_time_isa_result`) treated that static F64 as an exact
# runtime type and folded `x isa Int64` to a constant `false`, so an Int64
# argument bound to `x::Number` / `x::Real` never narrowed and the guarded
# branch was skipped (returning the fallthrough value).
#
# The runtime value is still an Int64 (`typeof` is correct), so for
# abstract-numeric params `isa` must defer to the runtime check instead of
# folding on the representational static type.

function number_isa_int64_5941(x::Number)
    if x isa Int64
        return x * x
    end
    return -1
end

function real_isa_int64_5941(x::Real)
    if x isa Int64
        return x + 100
    end
    return -1
end

# `x::Integer` already worked (static I64); keep it as a guard against
# regressing the working path.
function integer_isa_int64_5941(x::Integer)
    if x isa Int64
        return x * x
    end
    return -1
end

@testset "isa narrowing on abstract-numeric params (Issue #5941)" begin
    # An Int64 argument must narrow through Number / Real / Integer.
    @test number_isa_int64_5941(6) == 36
    @test real_isa_int64_5941(7) == 107
    @test integer_isa_int64_5941(6) == 36

    # A non-Int64 numeric must NOT match — the guarded branch is skipped.
    @test number_isa_int64_5941(3.5) == -1
    @test real_isa_int64_5941(2.5) == -1
end
end # module Agg_number_isa_narrowing_5941

# ===== source: type_inference/reflection_predicates_pure_julia_6738.jl =====
module Agg_reflection_predicates_pure_julia_6738
# Issue #6738: the reflection predicates isbits / ismutable / hasfield are now
# pure-Julia public wrappers (base/reflection.jl) over the VM-metadata
# primitives isbitstype (type-flag query) / ismutabletype (over _ismutabletype)
# and _fieldnames. Matches upstream julia 1.12 and works as first-class values.
# Migrating ismutable also fixes the prior String divergence (the old Rust
# ismutable returned false for String; upstream and now sjulia return true).

using Test

struct P6738
    x::Int
    y::Float64
end
mutable struct M6738
    a::Int
end

@testset "isbits / isbitstype (Issue #6738)" begin
    @test isbits(5) === true
    @test isbits(2.0) === true
    @test isbits(P6738(1, 2.0)) === true
    @test isbits([1]) === false
    @test isbits("s") === false
    @test isbitstype(Int) === true
    @test isbitstype(P6738) === true
    @test isbitstype(Array) === false
    @test isbitstype(String) === false
end

@testset "ismutable (Issue #6738)" begin
    @test ismutable([1]) === true
    @test ismutable(M6738(1)) === true
    @test ismutable(5) === false
    @test ismutable((1, 2)) === false
    # ismutable(String) is true upstream (was false in the old Rust builtin)
    @test ismutable("s") === true
end

@testset "hasfield (Issue #6738)" begin
    @test hasfield(P6738, :x) === true
    @test hasfield(P6738, :y) === true
    @test hasfield(P6738, :z) === false
    @test hasfield(M6738, :a) === true
    @test hasfield(Int, :x) === false
end

@testset "reflection predicates as first-class values (Issue #6738)" begin
    @test map(isbits, Any[1, [1], 2.0]) == [true, false, true]
    @test map(ismutable, Any[[1], 5]) == [true, false]
    f = isbits
    @test f(5) === true
    g = ismutable
    @test g([1]) === true
end
end # module Agg_reflection_predicates_pure_julia_6738

# ===== source: type_inference/slot_widening_4688.jl =====
module Agg_slot_widening_4688
using Test

function for_int_overwrite_4688(n)
    x = "init"
    for i in 1:n
        x = 42
    end
    x
end

function for_str_overwrite_4688(n)
    x = 1
    for i in 1:n
        x = "s"
    end
    x
end

function for_float_overwrite_4688(n)
    x = "f"
    for i in 1:n
        x = 1.5
    end
    x
end

function if_mixed_assign_4688(b)
    x = "init"
    if b
        x = 42
    end
    x
end

@testset "slot widening for Union locals across mixed-type assignments (Issue #4688)" begin
    # The for-loop body's `x = 42` previously latched the specializer's
    # slot type tracking on `I64`, even though the pre-loop `x = "init"`
    # bound x to a String. When the loop ran zero iterations the slot
    # still held the surviving String, and the final `LoadSlotI64`
    # crashed with `expected numeric in x, got Str("init")`. The fix
    # widens the recorded slot type to `Any` whenever an assignment
    # rebinds the variable to a different concrete type, so subsequent
    # loads emit `LoadAny` / `LoadSlot` and survive either value at
    # runtime.
    @test for_int_overwrite_4688(3) == 42
    @test for_int_overwrite_4688(0) == "init"
    @test for_int_overwrite_4688(1) == 42

    @test for_str_overwrite_4688(3) == "s"
    @test for_str_overwrite_4688(0) == 1
    @test for_str_overwrite_4688(1) == "s"

    @test for_float_overwrite_4688(3) == 1.5
    @test for_float_overwrite_4688(0) == "f"

    # Reassignment inside an `if` exhibits the same shape (one branch
    # rebinds to a new concrete type, the other preserves the initial
    # binding). Without the widening fix, the `else` (no-rebind) path
    # would mis-load through the body's typed slot.
    @test if_mixed_assign_4688(true) == 42
    @test if_mixed_assign_4688(false) == "init"
end
end # module Agg_slot_widening_4688

# ===== source: type_inference/ternary_float_promotion.jl =====
module Agg_ternary_float_promotion
# Test: Ternary/ifelse type inference with Float32/Float16 promotion
# Ensures that conditional branches with mixed float types are correctly promoted
# Regression test for Issue #1892

using Test

function ternary_f32_i64(flag)
    flag ? Float32(1.5) : 0
end

function ternary_f64_f32(flag)
    flag ? 1.5 : Float32(1.0)
end

function ternary_same_f32(flag)
    flag ? Float32(1.5) : Float32(2.5)
end

@testset "Ternary Float32 promotion" begin
    @test ternary_f32_i64(true) == Float32(1.5)
    @test ternary_f32_i64(false) == 0

    @test ternary_f64_f32(true) == 1.5
    @test ternary_f64_f32(false) == 1.0

    @test ternary_same_f32(true) == Float32(1.5)
    @test ternary_same_f32(false) == Float32(2.5)
end

@testset "ifelse Float32 promotion" begin
    @test ifelse(true, Float32(1.5), 0) == Float32(1.5)
    @test ifelse(false, Float32(1.5), 0) == 0
    @test ifelse(true, 1.5, Float32(1.0)) == 1.5
    @test ifelse(false, 1.5, Float32(1.0)) == 1.0
end
end # module Agg_ternary_float_promotion

# ===== source: type_inference/ternary_letblock_value_join_5180.jl =====
module Agg_ternary_letblock_value_join_5180
# Issue #5180: value-position if/ternary/begin should type-join their branch
# expressions instead of widening to Any.
#
# `infer_expr_type` previously fell through to ValueType::Any for Expr::Ternary
# and Expr::LetBlock (value-position `if`/`begin` are lowered to Ternary/LetBlock).
# That dropped slot typing when such an expression is used directly as an array
# index or operand. These fixtures lock in correctness + upstream-Julia parity:
# same-typed branches keep their concrete type, mixed I64/F64 branches stay
# correct per the branch taken, and incompatible branches stay dynamic.

using Test

# Ternary used directly as an index into a typed Int64 array. When both branches
# infer the same concrete I64 type the index/result stay I64-typed.
function ternary_index_same_type(cond, a, b, c, d)
    arr = [10, 20, 30, 40, 50]
    return arr[cond ? a + b : c + d]
end

# if/else in value position (lowered to Ternary) used directly as an index.
function if_value_index(cond)
    arr = [7, 8, 9]
    return arr[cond ? 1 : 3]
end

# begin/end block (lowered to LetBlock) in operand position.
function begin_block_operand(n)
    y = (begin
        t = n + 1
        t
    end) + 100
    return y
end

# if/else value-position block as an operand (lowered to Ternary).
function if_block_operand(cond)
    y = (cond ? 10 : 20) * 2
    return y
end

# Same-typed ternary as an arithmetic operand keeps I64 result.
function ternary_operand_same_type(cond)
    x = (cond ? 10 : 20) + 5
    return x
end

# Mixed I64/F64 branches: the ternary returns whichever branch is taken (no
# runtime promotion). typeof tracks the branch.
function ternary_mixed(cond)
    return cond ? 1 : 2.0
end

# Incompatible branches (Int vs String): value semantics must follow the branch.
function ternary_incompatible(cond)
    return cond ? 1 : "s"
end

@testset "ternary/if value-position used as typed index" begin
    @test ternary_index_same_type(true, 1, 1, 5, 5) == 20
    @test ternary_index_same_type(false, 1, 1, 2, 1) == 30
    @test if_value_index(true) == 7
    @test if_value_index(false) == 9
end

@testset "begin/if/ternary value-position as operand" begin
    @test begin_block_operand(0) == 101
    @test begin_block_operand(2) == 103
    @test if_block_operand(true) == 20
    @test if_block_operand(false) == 40
    @test ternary_operand_same_type(true) == 15
    @test ternary_operand_same_type(false) == 25
    @test ternary_operand_same_type(true) isa Int
end

@testset "ternary mixed/incompatible branches match Julia" begin
    @test ternary_mixed(true) === 1
    @test ternary_mixed(false) === 2.0
    @test typeof(ternary_mixed(true)) === Int
    @test typeof(ternary_mixed(false)) === Float64
    @test ternary_incompatible(true) === 1
    @test ternary_incompatible(false) === "s"
end
end # module Agg_ternary_letblock_value_join_5180

# ===== source: type_inference/typeof_codegen_narrowing_5077.jl =====
module Agg_typeof_codegen_narrowing_5077
using Test

function typeof_then_codegen_narrowing_5077(x::Union{Int64,String})
    if typeof(x) === Int64
        return x + 1
    else
        return length(x)
    end
end

function typeof_reversed_codegen_narrowing_5077(x::Union{Int64,String})
    if Int64 == typeof(x)
        return x + 2
    else
        return length(x) + 10
    end
end

function typeof_not_else_codegen_narrowing_5077(x::Union{Int64,String})
    if typeof(x) !== Int64
        return length(x)
    else
        return x + 3
    end
end

@testset "typeof guard codegen narrowing (Issue #5077)" begin
    @test typeof_then_codegen_narrowing_5077(41) == 42
    @test typeof_then_codegen_narrowing_5077("abcd") == 4
    @test typeof_reversed_codegen_narrowing_5077(40) == 42
    @test typeof_reversed_codegen_narrowing_5077("abc") == 13
    @test typeof_not_else_codegen_narrowing_5077(39) == 42
    @test typeof_not_else_codegen_narrowing_5077("abcd") == 4
end
end # module Agg_typeof_codegen_narrowing_5077

# ===== source: type_inference/union_preservation.jl =====
module Agg_union_preservation
# Test Union type preservation in codegen (Issue #1682)
# Ensures Union types don't collapse to Any during compilation
# These tests complement union_types.jl with additional patterns

using Test

# Function returning Union{Int64, Float64} based on condition
function get_union_numeric(flag)
    if flag == true
        42        # Int64
    else
        3.14      # Float64
    end
end

# Function returning Union{Nothing, Int64} (common pattern with iterate)
function maybe_int(flag)
    if flag == true
        100       # Int64
    else
        nothing   # Nothing
    end
end

# Function that uses iterate (returns Union{Nothing, Tuple})
function first_element(arr)
    iter = iterate(arr)
    if iter === nothing
        return nothing
    else
        return iter[1]
    end
end

# Function with nested conditionals returning Union{Int64, Float64}
function nested_numeric(a, b)
    if a == true
        if b == true
            1         # Int64
        else
            1.0       # Float64
        end
    else
        2.5           # Float64
    end
end

# Compute results outside of testset to avoid potential scoping issues
result_numeric_true = get_union_numeric(true)
result_numeric_false = get_union_numeric(false)
result_maybe_true = maybe_int(true)
result_maybe_false = maybe_int(false)
result_first_nonempty = first_element([1, 2, 3])
result_first_empty = first_element(Int64[])
result_nested_tt = nested_numeric(true, true)
result_nested_tf = nested_numeric(true, false)
result_nested_ft = nested_numeric(false, true)

@testset "Union type preservation in codegen" begin
    # Test basic Union{Int64, Float64}
    @test result_numeric_true == 42
    @test result_numeric_false == 3.14

    # Test Union{Nothing, Int64}
    @test result_maybe_true == 100
    @test result_maybe_false === nothing

    # Test iterate Union preservation
    @test result_first_nonempty == 1
    @test result_first_empty === nothing

    # Test nested Union
    @test result_nested_tt == 1
    @test result_nested_tf == 1.0
    @test result_nested_ft == 2.5
end
end # module Agg_union_preservation

# ===== source: type_inference/union_typed_array_literal_dispatch_5143.jl =====
module Agg_union_typed_array_literal_dispatch_5143
using Test

# Issue #5143: a small-Union-typed array literal (`Union{Int64,Float64}[...]`)
# must store each element verbatim — without coercing `Float64` members to the
# first/`Int64` member — so per-element multiple dispatch picks the correct
# concrete method (the union-splitting correctness goal of #5143).

classify_5143(x::Int64) = "int=$x"
classify_5143(x::Float64) = "float=$x"

@testset "Union-typed array literal preserves each member type" begin
    v = Union{Int64,Float64}[1, 2.5, 3, 4.0]

    # Container stays a Union element type, not a single member. (Compared by
    # rendered name; the `eltype(v) == Union{...}` type-object identity gap is
    # an independent defect tracked in Issue #5335.)
    @test string(typeof(v)) == "Vector{Union{Float64, Int64}}"
    @test string(eltype(v)) == "Union{Float64, Int64}"

    # Each element keeps its own concrete type (no Float64 -> Int64 coercion).
    @test typeof(v[1]) == Int64
    @test typeof(v[2]) == Float64
    @test typeof(v[3]) == Int64
    @test typeof(v[4]) == Float64
    @test v[2] == 2.5
    @test v[4] == 4.0
end

@testset "Union-typed array literal dispatches per element" begin
    v = Union{Int64,Float64}[1, 2.5, 3, 4.0]
    out = String[]
    for x in v
        push!(out, classify_5143(x))
    end
    @test out == ["int=1", "float=2.5", "int=3", "float=4.0"]
end

@testset "single-element Union literal keeps the float member" begin
    a = Union{Int64,Float64}[2.5]
    @test typeof(a[1]) == Float64
    @test a[1] == 2.5
    @test classify_5143(a[1]) == "float=2.5"

    b = Union{Int64,Float64}[2]
    @test typeof(b[1]) == Int64
    @test classify_5143(b[1]) == "int=2"
end
end # module Agg_union_typed_array_literal_dispatch_5143

# ===== source: type_inference/union_types.jl =====
module Agg_union_types
# Test: Union type inference in conditionals
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function mixed_return(flag)
    if flag
        1        # Int64
    else
        2.0      # Float64
    end
    # Return type: Union{Int64, Float64}
end

function conditional_types(x)
    # Returns different types based on input
    if x > 0
        x * 2        # Int64 when x is Int
    else
        0.0          # Float64
    end
end

function nested_conditionals(a, b)
    if a
        if b
            1
        else
            2
        end
    else
        3
    end
end

function multiple_branches(n)
    # Multiple branches returning different types
    if n < 0
        -1
    elseif n == 0
        0
    else
        1
    end
end

@testset "Union type inference" begin
    # Test mixed return types
    result1 = mixed_return(true)
    result2 = mixed_return(false)
    
    @test result1 == 1
    @test result2 == 2.0
    
    # Test conditional type inference
    @test conditional_types(5) == 10
    @test conditional_types(-1) == 0.0
    
    # Test nested conditionals
    @test nested_conditionals(true, true) == 1
    @test nested_conditionals(true, false) == 2
    @test nested_conditionals(false, true) == 3
    
    # Test multiple branches
    @test multiple_branches(-5) == -1
    @test multiple_branches(0) == 0
    @test multiple_branches(5) == 1
end
end # module Agg_union_types

true
