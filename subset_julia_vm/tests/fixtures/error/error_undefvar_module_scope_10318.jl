using Test

# Issue #10318: accessing a binding that does not exist in a specific module
# raises a catchable UndefVarError (NOT a FieldError / TypeError), and the
# message keeps the module scope, matching upstream Julia 1.12's
# `UndefVarError: `T` not defined in `Main.<Module>``.
#
# Upstream appends a "Suggestion: ..." line to the module/global-scope message;
# the subset omits that cosmetic line (its bare-global UndefVarError also has no
# suggestion), so these assertions use `occursin` on the load-bearing parts
# (type, var, scope) rather than exact-string equality, which holds under both
# `julia` and `sjulia`.

module UndefScopeOwner10318
    const T = Int64
end

module UndefScopeEmpty10318
end

function caught_10318(f)
    try
        f()
        return nothing
    catch e
        return e
    end
end

getprop_10318(x) = x.nope10318

@testset "module-scoped UndefVarError (Issue #10318)" begin
    # Qualified access to a binding missing from the (empty) module.
    e = caught_10318(() -> UndefScopeEmpty10318.T)
    @test typeof(e) === UndefVarError
    @test e isa UndefVarError
    @test e.var === :T
    msg = sprint(showerror, e)
    @test occursin("UndefVarError", msg)
    @test occursin("`T`", msg)
    @test occursin("not defined in", msg)
    @test occursin("UndefScopeEmpty10318", msg)

    # getfield form reaches the same module arm.
    e2 = caught_10318(() -> getfield(UndefScopeEmpty10318, :T))
    @test typeof(e2) === UndefVarError
    @test e2.var === :T
    @test occursin("UndefScopeEmpty10318", sprint(showerror, e2))

    # Dynamic dot access through an Any-typed parameter (GetFieldByName path).
    e3 = caught_10318(() -> getprop_10318(UndefScopeEmpty10318))
    @test typeof(e3) === UndefVarError
    @test e3.var === :nope10318
    @test occursin("UndefScopeEmpty10318", sprint(showerror, e3))

    # A binding that DOES exist in another module is not visible here: the
    # message must name THIS module, not fall back to an unrelated bare global
    # (Issue #7955 anti-pattern).
    @test occursin("UndefScopeEmpty10318", sprint(showerror, e))
    @test !occursin("UndefScopeOwner10318", sprint(showerror, e))

    # getproperty on the Main module for a missing binding is UndefVarError too,
    # scoped to `Main` (not `Main.Main`).
    e4 = caught_10318(() -> getfield(Main, :undefined_main_binding_10318))
    @test typeof(e4) === UndefVarError
    @test occursin("not defined in `Main`", sprint(showerror, e4))
end

using Printf

@testset "root/stdlib module scope prints bare (Issue #10318)" begin
    # Root and stdlib modules render WITHOUT a `Main.` qualification, matching
    # upstream (`not defined in `Base``, not `Main.Base`). A user module under
    # Main keeps the `Main.` prefix. Reached via dynamic dot access through an
    # Any-typed parameter so the runtime module arm (not compile-time
    # resolution) handles it.
    for (mod, scope) in (
        (Base, "Base"),
        (Core, "Core"),
        (Printf, "Printf"),
    )
        e = caught_10318(() -> getprop_10318(mod))
        @test typeof(e) === UndefVarError
        @test occursin("not defined in `$scope`", sprint(showerror, e))
        # A root/stdlib module must NOT be `Main.`-qualified.
        @test !occursin("Main.$scope", sprint(showerror, e))
    end

    # getfield form on a root module.
    e = caught_10318(() -> getfield(Base, :nope10318))
    @test typeof(e) === UndefVarError
    @test occursin("not defined in `Base`", sprint(showerror, e))

    # A user module under Main is still qualified with `Main.`.
    eu = caught_10318(() -> getprop_10318(UndefScopeEmpty10318))
    @test occursin("not defined in `Main.UndefScopeEmpty10318`", sprint(showerror, eu))
end

# The owning module still resolves its own const correctly (no regression).
@testset "module const still resolves (Issue #10318)" begin
    @test UndefScopeOwner10318.T === Int64
end

true
