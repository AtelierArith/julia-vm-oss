# Issue #10937 (found via corpus ratchet Issue #10935): long-form `function`
# definitions as `global`/`local` declaration items. This is the julia/base
# bootstrap pattern (Base.jl's relative include, dict.jl, iobuffer.jl,
# mpfr.jl, range.jl, reducedim.jl): a `let` body declaring a global method.
# Before the fix the keyword after `global` was mis-parsed as an identifier
# and the definition's `end` was left dangling (a hard parse error once
# Issue #10927 removed the bare-`end`-as-identifier escape hatch).

# 1. The Base bootstrap pattern: `global function` inside a top-level `let`
#    defines a module-scope method usable after the block.
let state = 10
    global function scope_global_longform_10937(x)
        x + 1
    end
end
@assert scope_global_longform_10937(2) == 3

# 2. Top-level `global function` behaves exactly like a plain definition.
global function scope_toplevel_global_longform_10937(x)
    x * 3
end
@assert scope_toplevel_global_longform_10937(4) == 12

# 3. `local function` inside a `let` is callable within the block.
r = let
    local function scope_local_longform_10937(x)
        x * 2
    end
    scope_local_longform_10937(21)
end
@assert r == 42

# Deferred (split into Issue #11015): the captured-state variant of the
# Base pattern, `let counter = 0; global function f(); counter += 1; end; end`,
# defines and binds the method but the let-local capture is not yet wired
# (UndefVarError at call time in sjulia; works upstream).

true
