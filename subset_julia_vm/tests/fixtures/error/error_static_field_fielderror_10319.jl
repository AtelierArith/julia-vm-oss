using Test

# Issue #10319: when the receiver type is *statically known* at compile time
# (a concrete struct type, a `DataType` receiver, or any other primitive
# whose static `ValueType` is known to the compiler), sjulia used to reject a
# nonexistent-field access at COMPILE TIME (aborting the whole program)
# instead of deferring to the catchable runtime `FieldError` upstream Julia
# always raises. The dynamic (`Any`-typed) path already got this right in
# Issue #10212; this fixture exercises the static path, which reaches the
# exact same runtime `Instr::GetFieldByName` / `Instr::SetFieldByName`
# machinery after the fix.

struct EmptyFoo10319 end

struct PointFoo10319
    x::Int
    y::Int
end

mutable struct MutFoo10319
    a::Int
end

function caught_10319(f)
    try
        f()
        return nothing
    catch e
        return e
    end
end

@testset "struct field read: statically-known bogus field (Issue #10319)" begin
    # A struct with NO fields at all — exact MWE from the issue.
    e = caught_10319(() -> EmptyFoo10319().nope)
    @test typeof(e) === FieldError
    @test e isa FieldError
    @test e.field === :nope
    @test startswith(
        sprint(showerror, e),
        "FieldError: type EmptyFoo10319 has no field `nope`",
    )

    # A struct WITH fields, accessed with a name that isn't one of them.
    e2 = caught_10319(() -> PointFoo10319(1, 2).bogus)
    @test typeof(e2) === FieldError
    @test e2.field === :bogus
    @test startswith(
        sprint(showerror, e2),
        "FieldError: type PointFoo10319 has no field `bogus`",
    )

    # Valid field access on the same statically-known struct type must keep
    # working (regression guard for the fast `Instr::GetField` path).
    p = PointFoo10319(3, 4)
    @test p.x == 3
    @test p.y == 4
end

@testset "DataType field read: statically-known bogus field (Issue #10319)" begin
    # Exact MWE from the issue: a statically-known `DataType` receiver.
    e = caught_10319(() -> Int.bogus)
    @test typeof(e) === FieldError
    @test e.field === :bogus
    @test startswith(
        sprint(showerror, e),
        "FieldError: type DataType has no field `bogus`",
    )

    # Recognized DataType fields must keep working (regression guard).
    @test Int.name !== nothing
    @test Vector{Int}.parameters[1] === Int
end

function static_int_bogus_field_10319()
    x = 5
    try
        x.foo
        return nothing
    catch e
        return e
    end
end

function static_tuple_bogus_field_10319()
    t = (1, 2, 3)
    try
        t.foo
        return nothing
    catch e
        return e
    end
end

@testset "primitive receiver field read: statically-known bogus field (Issue #10319)" begin
    # Any statically-known non-struct value (Int64, Tuple, ...) with a field
    # name that matches no struct in the program is the same bug class —
    # verified against upstream `julia` 1.12 for Int64/Tuple (both raise
    # FieldError, not a compile-time abort). `x`/`t` are plain locals (not
    # closure-captured) so the compiler sees their concrete static type,
    # exercising the same "primitive receiver" catch-all fixed by #10319
    # rather than the already-working (#10212) `Any`-typed dynamic path.
    e = static_int_bogus_field_10319()
    @test typeof(e) === FieldError
    @test startswith(sprint(showerror, e), "FieldError: type Int64 has no field `foo`")

    e2 = static_tuple_bogus_field_10319()
    @test typeof(e2) === FieldError
    # Only assert the error class here, not the exact type-name spelling
    # inside the message (Tuple{Int64,Int64,Int64} vs upstream's bare
    # `Tuple` — display-hint gap tracked by Issue #8664, same allowance
    # `error_fielderror_parity_10212.jl` makes for WeakRef/Array/etc.).
end

function static_struct_bogus_field_write_10319()
    m = MutFoo10319(1)
    try
        m.bogus = 2
        return nothing
    catch e
        return e
    end
end

@testset "struct field write: statically-known bogus field (Issue #10319)" begin
    e = static_struct_bogus_field_write_10319()
    @test typeof(e) === FieldError
    @test e.field === :bogus
    @test startswith(
        sprint(showerror, e),
        "FieldError: type MutFoo10319 has no field `bogus`",
    )

    # Valid field assignment on the same statically-known struct type must
    # keep working (regression guard for the fast `Instr::SetField` path).
    m2 = MutFoo10319(1)
    m2.a = 42
    @test m2.a == 42
end

true
