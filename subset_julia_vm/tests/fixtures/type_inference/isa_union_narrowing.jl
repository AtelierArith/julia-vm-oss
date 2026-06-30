# Issue #3523: isa narrowing should handle Union types
function f(x)
    if x isa Union{Int64, Nothing}
        if x === nothing
            return 0
        else
            return x + 1
        end
    end
    return -1
end

@assert f(41) == 42
@assert f(nothing) == 0
@assert f("x") == -1

true
