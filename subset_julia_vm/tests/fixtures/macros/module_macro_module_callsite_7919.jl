# Issue #7919: a macro expanded inside `module M ... end` must receive the
# call-site module as `__module__`, not the hard-coded `Main`. Upstream Julia
# binds `__module__` to the module where the macro is expanded.
#
# Top-level expansion still resolves to `Main`; expansion inside module `M`
# resolves to `M`. Verified against upstream julia 1.12:
#   top === Main       -> true
#   M.owner === M      -> true
#   M.owner === Main   -> false

macro whichmod()
    return :( $__module__ )
end

module M
    macro whichmod_m()
        return :( $__module__ )
    end
    const owner = @whichmod_m
end

const top = @whichmod

# The user-facing `@__MODULE__` macro must resolve the same way (Issue #7919):
# `M2` inside a module, `Main` at top level.
module M2
    const here = @__MODULE__
end
const top_mod = (@__MODULE__)

(top === Main) && (M.owner === M) && !(M.owner === Main) &&
    (M2.here === M2) && (top_mod === Main)
