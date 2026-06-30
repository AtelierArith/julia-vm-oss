# Issue #7774: a keyword-argument default that references a free name (a global
# / const-global) must resolve to that binding when the keyword is omitted, even
# when the function ALSO has positional parameters.
#
# Root cause: a function with positional parameters dispatches through the
# specialize path. When the fallback body was selected, that path bound each
# omitted keyword to the baked `kwparam.default` literal (which is `I64(0)` for
# any non-foldable default such as a global reference) instead of evaluating the
# default *expression* in the real call frame. Keyword-only functions (no
# positional parameters) already routed through `bind_kwargs_defaults`, so they
# were correct -- the bug only surfaced once a positional parameter shifted the
# call onto the specialize path.
#
# Verified against upstream Julia 1.12 before implementation.

using Test

# --- non-const global default, positional argument present -------------------
glob_tol_7774 = 1.0e-6
close_7774(a, b; atol=glob_tol_7774) = abs(a - b) < atol

@testset "kwargs_default_global_positional_7774: non-const global default" begin
    @test close_7774(1.0, 1.0)            # atol omitted -> resolves global -> 1e-6
    @test close_7774(1.0, 1.0; atol=1e-6) # atol supplied
    @test !close_7774(1.0, 2.0)           # diff 1.0 not < 1e-6
end

# --- const global default, positional argument present ----------------------
const CONST_G_7774 = 5
g_const_7774(a; x=CONST_G_7774) = (a, x)

@testset "kwargs_default_global_positional_7774: const global default" begin
    @test g_const_7774(1) == (1, 5)       # x omitted -> resolves const global -> 5
    @test g_const_7774(1; x=99) == (1, 99)
end

# --- plain (non-const) global default, single positional --------------------
nonconst_g_7774 = 5
h_nonconst_7774(a; x=nonconst_g_7774) = (a, x)

@testset "kwargs_default_global_positional_7774: plain global default" begin
    @test h_nonconst_7774(1) == (1, 5)
    @test h_nonconst_7774(1; x=7) == (1, 7)
end

# --- default referencing an earlier positional parameter (must still work) ---
shadow_7774(a; x=a + 100) = (a, x)

@testset "kwargs_default_global_positional_7774: default from positional param" begin
    @test shadow_7774(5) == (5, 105)
    @test shadow_7774(5; x=0) == (5, 0)
end

# --- multiple positional + multiple kw, mixing global and literal defaults ---
multi_g_7774 = 8
multi_7774(a, b; x=multi_g_7774, y=2) = (a, b, x, y)

@testset "kwargs_default_global_positional_7774: multi positional + multi kw" begin
    @test multi_7774(1, 2) == (1, 2, 8, 2)
    @test multi_7774(1, 2; x=10) == (1, 2, 10, 2)
    @test multi_7774(1, 2; y=20) == (1, 2, 8, 20)
end

true
