# User-defined macros execute their body at expansion time. This mirrors the
# Symbolics @variables shape: the macro delegates AST construction to a helper.

function parse_vars(macroname, typ, xs)
    if macroname != :variables
        return :(0)
    end
    if length(xs) != 1
        return :(0)
    end
    if xs[1] != :x
        return :(0)
    end
    :(42)
end

macro variables(xs...)
    parse_vars(:variables, Real, xs)
end

result = @variables x
result == 42
