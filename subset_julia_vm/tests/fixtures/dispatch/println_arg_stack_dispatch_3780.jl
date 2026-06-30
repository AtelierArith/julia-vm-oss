using Test

function dispatch_stack_side_effect()
    println("a")
    return 1
end

function dispatch_stack_ifelse(c, x, y)
    if c
        return x
    else
        return y
    end
end

@test dispatch_stack_ifelse(true, 1, dispatch_stack_side_effect()) == 1
@test dispatch_stack_ifelse(false, 1, dispatch_stack_side_effect()) == 1

true
