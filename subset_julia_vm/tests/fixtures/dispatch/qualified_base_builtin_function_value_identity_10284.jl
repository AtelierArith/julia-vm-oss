# Regression coverage for Issue #10284 (residual of #10077): a qualified
# `Base.<fn>` value for the SMALL ALLOWLIST of Base functions still backed by
# a direct Rust builtin/intrinsic (`is_base_function` in
# `subset_julia_vm_compile/src/compile/base_functions.rs` — `sqrt`, `floor`, `ceil`,
# `round`, `println`, operators via `function_name_to_binary_op`, etc.) must
# carry the SAME runtime type identity as the unqualified `<fn>` — upstream
# Julia's generic function has ONE canonical name (`nameof`) regardless of
# the access path used to reach it, matching the general fix already applied
# for user modules and general `Base.<fn>`-as-value access (Issue #10077,
# `qualified_function_value_identity_10077.jl`). Before this fix,
# `Base.sqrt`/`Base.:+` baked the qualified `"Base.sqrt"`/`"Base.+"` spelling
# into the captured `FunctionValue`'s name (because these BuiltinOp-backed
# names have no genuine `"Base.<fn>"` entry in `method_tables` to resolve
# candidates through, unlike Pure Julia Base functions unaffected by this bug
# such as `Base.map`/`Base.sin`), making `isa Function` false and `typeof`
# diverge from the bare-access identity.
#
# Covers, for each allowlisted name and a representative operator: `isa
# Function` (true), `typeof(...)` identity vs. the bare/unqualified access
# (equal), direct calling correctness, and calling as an HOF callback
# (`map`/`filter`) — the practical reason `isa Function` matters (Issue
# #10255).
#
# Also guards a regression the fix itself could introduce: `is_base_function`
# is heterogeneous — a handful of its entries (`Int`, `String`, `Char`, `Ref`,
# `IOBuffer`) are TYPE/constructor names needed for their CALL-path conversion
# behavior (`Int(3.0)`), not generic functions. As a bare VALUE (not called),
# `Base.Int isa Function` must stay `false` (it is a `DataType`/`UnionAll`),
# matching the unqualified `Int` value path in `compile/expr/mod.rs` (which
# checks `is_builtin_type_name` before `is_base_function`). The type-name
# check in `compile_module_function_ref` must run BEFORE the `is_base_function`
# allowlist branch for the same reason, or emitting a clean bare
# function-value name for these (this fix's very mechanism) would wrongly
# flip `isa Function` to `true` for them.

using Test

@testset "qualified Base.<allowlisted-fn> value identity (Issue #10284)" begin
    # `sqrt`/`floor`/`ceil`/`round`: is_base_function allowlist, BuiltinOp-backed.
    @test Base.sqrt isa Function
    @test typeof(Base.sqrt) == typeof(sqrt)
    @test Base.sqrt(4.0) == 2.0

    @test Base.floor isa Function
    @test typeof(Base.floor) == typeof(floor)
    @test Base.floor(3.7) == 3.0

    @test Base.ceil isa Function
    @test typeof(Base.ceil) == typeof(ceil)
    @test Base.ceil(3.2) == 4.0

    @test Base.round isa Function
    @test typeof(Base.round) == typeof(round)
    @test Base.round(3.5) == 4.0

    # `println`: is_base_function allowlist, I/O BuiltinOp with no method-table
    # entry at all (not even under the bare name).
    @test Base.println isa Function
    @test typeof(Base.println) == typeof(println)

    # Operators reached via `function_name_to_binary_op`, not `is_base_function`.
    @test Base.:+ isa Function
    @test typeof(Base.:+) == typeof(+)
    @test Base.:+(2, 3) == 5

    @test Base.:- isa Function
    @test typeof(Base.:-) == typeof(-)
    @test Base.:-(5, 2) == 3

    @test Base.:* isa Function
    @test typeof(Base.:*) == typeof(*)
    @test Base.:*(4, 3) == 12

    # HOF-callback correctness (Issue #10255's practical motivation): a
    # qualified `Base.<fn>` value must dispatch through `map`/`filter` exactly
    # like the bare `<fn>` value.
    @test map(Base.sqrt, [1.0, 4.0, 9.0]) == [1.0, 2.0, 3.0]
    @test map(Base.floor, [1.5, 2.7]) == [1.0, 2.0]
    @test map(Base.:+, [1, 2, 3], [10, 20, 30]) == [11, 22, 33]
    @test filter(x -> Base.sqrt(x) > 1.5, [1.0, 4.0, 9.0]) == [4.0, 9.0]

    # A qualified reference used directly (without an intervening variable)
    # also resolves and calls correctly.
    h = Base.sqrt
    @test h(9.0) == 3.0
    @test h isa Function

    # Type/constructor names that ALSO appear in the `is_base_function`
    # allowlist (for their call-path conversion behavior) must still resolve
    # to the type object as a bare value, not a Function (regression guard
    # for this fix's own mechanism — see module-level comment above).
    @test !(Base.Int isa Function)
    @test typeof(Base.Int) == typeof(Int)
    @test Base.Int(3.0) == 3

    @test !(Base.String isa Function)
    @test typeof(Base.String) == typeof(String)

    @test !(Base.Char isa Function)
    @test typeof(Base.Char) == typeof(Char)

    @test !(Base.Ref isa Function)
    @test typeof(Base.Ref) == typeof(Ref)

    @test !(Base.IOBuffer isa Function)
    @test typeof(Base.IOBuffer) == typeof(IOBuffer)
end

true  # Test passed
