using Test

# Issue #10999: a `catch e` binding whose name collides with a differently-typed
# outer local must permanently OVERWRITE that local with the caught exception
# (upstream does NOT shadow/restore the catch variable), instead of crashing with
# an internal type-check error when the outer local's static type is not `Any`.
# Verified against `julia --startup-file=no` (1.12.6).

# String outer local (the reported MWE).
function catch_shadow_str()
    e = "outer_e"
    try
        error("boom")
    catch e
    end
    return e
end

# Int outer local.
function catch_shadow_int()
    e = 42
    try
        error("boom")
    catch e
    end
    return e
end

# Float64 outer local.
function catch_shadow_float()
    e = 1.5
    try
        error("boom")
    catch e
    end
    return e
end

# A parameter is a local too: it is overwritten just the same.
function catch_shadow_param(e::Int)
    try
        error("boom")
    catch e
    end
    return e
end

# Nested try/catch reusing the same name: the outer catch wins last.
function catch_shadow_nested()
    e = 1
    try
        try
            error("inner")
        catch e
        end
        error("outer")
    catch e
    end
    return e
end

# The catch variable stays bound after the try statement.
function catch_used_after()
    e = 3
    try
        error("boom")
    catch e
    end
    x = e
    return x isa ErrorException
end

# Control: a catch variable that collides with nothing leaves the outer local alone.
function catch_no_collision()
    y = "keep"
    try
        error("boom")
    catch e
        @test e isa ErrorException
    end
    return y
end

@test catch_shadow_str() == ErrorException("boom")
@test catch_shadow_int() == ErrorException("boom")
@test catch_shadow_float() == ErrorException("boom")
@test catch_shadow_param(3) == ErrorException("boom")
@test catch_shadow_nested() == ErrorException("outer")
@test catch_used_after()
@test catch_no_collision() == "keep"

true
