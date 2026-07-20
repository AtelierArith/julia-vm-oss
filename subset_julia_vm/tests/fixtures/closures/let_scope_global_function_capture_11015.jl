using Test

# Issue #11015: a `global function` defined inside a `let` must capture the
# let-local state (the Base bootstrap pattern — `Base.include` closes over
# `SOURCE_PATH` exactly this way). The method binds at module scope but its body
# reads/writes the enclosing `let` scope's boxed locals.
# Issue #11015 (same root cause): a plain named function defined inside a `let`
# must capture the let-locals too.
# Verified against `julia --startup-file=no` (1.12.6).

# Long form: `global function` mutating a let-local across calls.
let counter = 0
    global function inc11015()
        counter += 1
    end
end

# Short form.
let c = 0
    global next_id11015() = (c += 1)
end

# Several global methods over ONE let binding (the `Base.include` shape).
let store = Dict{String,Int}()
    global function setk11015(k, v)
        store[k] = v
    end
    global function getk11015(k)
        store[k]
    end
end

# Multi-method dispatch over a shared let-local.
let path = "root"
    global function withpath11015(x::String)
        path = x
        return path
    end
    global function withpath11015(n::Int)
        return string(path, n)
    end
end

# A plain (non-`global`) named function in a `let` captures the same way.
function let_local_named_capture()
    let c = 0
        function bump()
            c += 1
        end
        return (bump(), bump())
    end
end

# The captured global method is callable from inside another function.
function call_inc11015()
    return inc11015()
end

@test inc11015() == 1
@test inc11015() == 2
@test call_inc11015() == 3

@test next_id11015() == 1
@test next_id11015() == 2

setk11015("a", 5)
setk11015("b", 7)
@test getk11015("a") == 5
@test getk11015("b") == 7

@test withpath11015(5) == "root5"
@test withpath11015("abc") == "abc"
@test withpath11015(7) == "abc7"

@test let_local_named_capture() == (1, 2)

true
