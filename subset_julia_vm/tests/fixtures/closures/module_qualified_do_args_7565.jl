module ModuleQualifiedDoArgs7565

postwalk(f, x) = x

g(ex) = ModuleQualifiedDoArgs7565.postwalk(ex) do y
    y
end

end

ModuleQualifiedDoArgs7565.g(1) == 1 || error("module-qualified do-block call dropped existing arguments")

true
