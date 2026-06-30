module MacroBodyMacro7569

macro inner(ex)
    esc(ex)
end

macro outer(ex)
    @inner ex
end

end

MacroBodyMacro7569.@outer(21 + 21) == 42
