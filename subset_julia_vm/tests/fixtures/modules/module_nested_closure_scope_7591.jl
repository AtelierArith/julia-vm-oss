using Test

module M7591
hidden(x) = x + 1

function f(xs)
    map(xs) do x
        thunk = () -> hidden(x)
        thunk()
    end
end

function g(xs)
    map(xs) do x
        outer = y -> begin
            inner = () -> hidden(y)
            inner()
        end
        outer(x)
    end
end
end

@test M7591.f([1]) == [2]
@test M7591.g([2]) == [3]

true
