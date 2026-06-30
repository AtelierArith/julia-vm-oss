module ModuleDeepClosure7591

hidden(x) = x + 1

function f(xs)
    map(xs) do x
        thunk = () -> hidden(x)
        thunk()
    end
end

end

ModuleDeepClosure7591.f([1, 2]) == [2, 3]
