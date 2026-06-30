module DoBlockMacroVisibility7558

macro passthrough(ex)
    esc(ex)
end

function f(xs)
    map(xs) do x
        @passthrough x + 1
    end
end

end

true
