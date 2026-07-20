using Test

# Issue #10212: getfield / field access on a nonexistent field raises a
# catchable FieldError (Julia 1.12 shape), not the pre-1.12 TypeError, for
# Expr / GlobalRef / QuoteNode / LineNumberNode / TypeVar and the other
# native-value receivers that share the same Rust arms.
# Issue #10318: a missing module binding is an UndefVarError, not a field error.

# Dynamic dot access through an Any-typed parameter (exercises the
# Instr::GetFieldByName path, which must raise catchably, not abort).
getprop_10212(x) = x.bogus
# Valid-field dynamic dot access on a LineNumberNode (same GetFieldByName path).
getprop_line_10212(x) = x.line
getprop_file_10212(x) = x.file

function caught_10212(f)
    try
        f()
        return nothing
    catch e
        return e
    end
end

@testset "getfield FieldError parity (Issue #10212)" begin
    ex = :(x + 1)
    gr = GlobalRef(Main, :sin)
    qn = QuoteNode(:x)
    ln = LineNumberNode(1)
    tv = TypeVar(:T)

    for (val, tname) in (
        (ex, "Expr"),
        (gr, "GlobalRef"),
        (qn, "QuoteNode"),
        (ln, "LineNumberNode"),
        (tv, "TypeVar"),
    )
        e = caught_10212(() -> getfield(val, :bogus))
        @test typeof(e) === FieldError
        @test e isa FieldError
        @test e.field === :bogus
        @test startswith(
            sprint(showerror, e),
            "FieldError: type $tname has no field `bogus`",
        )
    end

    # Same receivers through the dynamic dot path (GetFieldByName).
    for (val, tname) in (
        (ex, "Expr"),
        (gr, "GlobalRef"),
        (qn, "QuoteNode"),
        (ln, "LineNumberNode"),
        (tv, "TypeVar"),
    )
        e = caught_10212(() -> getprop_10212(val))
        @test typeof(e) === FieldError
        @test startswith(
            sprint(showerror, e),
            "FieldError: type $tname has no field `bogus`",
        )
    end

    # LineNumberNode: valid fields project through the dynamic dot path (not
    # just the getfield builtin), a bogus field raises FieldError. This arm was
    # missing from the Any-typed GetFieldByName chain in the original #10212 fix.
    ln_full = LineNumberNode(42, :myfile)
    ln_nofile = LineNumberNode(7)
    @test getprop_line_10212(ln_full) == 42
    @test getprop_file_10212(ln_full) === :myfile
    @test getprop_file_10212(ln_nofile) === nothing
end

struct ErrFieldEmpty10212 end
mutable struct ErrFieldMut10212
    a::Int
end

@testset "struct field FieldError parity (Issue #10212)" begin
    # getfield on a struct with a bogus field name.
    e = caught_10212(() -> getfield(ErrFieldEmpty10212(), :nope))
    @test typeof(e) === FieldError
    @test e.field === :nope
    @test startswith(
        sprint(showerror, e),
        "FieldError: type ErrFieldEmpty10212 has no field `nope`",
    )

    # Dynamic dot access on a struct (catchable, not a VM abort).
    e = caught_10212(() -> getprop_10212(ErrFieldMut10212(1)))
    @test typeof(e) === FieldError
    @test startswith(
        sprint(showerror, e),
        "FieldError: type ErrFieldMut10212 has no field `bogus`",
    )

    # setfield! on a bogus field is also FieldError upstream.
    e = caught_10212(() -> setfield!(ErrFieldMut10212(1), :b, 2))
    @test typeof(e) === FieldError
    @test e.field === :b
    @test startswith(
        sprint(showerror, e),
        "FieldError: type ErrFieldMut10212 has no field `b`",
    )
end

@testset "other native receivers raise FieldError (Issue #10212)" begin
    # getfield by name on other native-backed receivers.
    e = caught_10212(() -> getfield((x = 1,), :bogus))
    @test typeof(e) === FieldError
    @test startswith(
        sprint(showerror, e),
        "FieldError: type NamedTuple has no field `bogus`",
    )

    e = caught_10212(() -> getfield(Ref(1), :bogus))
    @test typeof(e) === FieldError
    @test startswith(
        sprint(showerror, e),
        "FieldError: type Base.RefValue has no field `bogus`",
    )

    e = caught_10212(() -> getfield(Int, :bogus))
    @test typeof(e) === FieldError
    @test startswith(
        sprint(showerror, e),
        "FieldError: type DataType has no field `bogus`",
    )

    # Dynamic dot path on the remaining receivers; only assert the error type
    # here (the type name inside the message may be spelled differently,
    # e.g. Vector{Int64} vs Array — display hint gap tracked by Issue #8664).
    @test typeof(caught_10212(() -> getprop_10212(WeakRef(1)))) === FieldError
    @test typeof(caught_10212(() -> getprop_10212([1, 2]))) === FieldError
    @test typeof(caught_10212(() -> getprop_10212(match(r"a", "a")))) === FieldError
    @test typeof(caught_10212(() -> getprop_10212(Base.Generator(identity, 1:3)))) ===
          FieldError
    @test typeof(caught_10212(() -> getprop_10212(pairs((a = 1,))))) === FieldError
end

@testset "missing module binding is UndefVarError (Issue #10318)" begin
    e = caught_10212(() -> getfield(Main, :bogus_undefined_10318))
    @test typeof(e) === UndefVarError
    @test startswith(
        sprint(showerror, e),
        "UndefVarError: `bogus_undefined_10318` not defined",
    )
end

true
