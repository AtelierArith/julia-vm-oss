# Issue #7350 (A3): a module-qualified call `Mod.f(...)` as the call target of
# macro-expanded code must lower (it errored "unsupported call target Expr").
module M7350
    helper(x) = x * 2
end

macro doubled()
    return :(M7350.helper(21))
end

@doubled() == 42
