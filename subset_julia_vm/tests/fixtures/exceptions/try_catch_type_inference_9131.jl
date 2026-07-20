# Regression test for Issue #9131: try/catch type inference shared env by mutable
# reference, causing catch-branch assignments to overwrite try-branch inferred types.
# The function return type was collapsed to the catch type only, causing a runtime
# slot-type mismatch when the try branch actually ran.

# MWE from the issue: function returns Int64 (try) or String (catch).
function f_try(cond)
    local x
    try
        x = 42        # Int64 assignment
        cond && error()
    catch
        x = "error"   # String assignment
    end
    return x          # Union{Int64, String} — inferred type must cover BOTH arms
end

function caller(cond)
    r = f_try(cond)
    println(typeof(r))
end

caller(true)    # exception path: x = "error" → String
caller(false)   # normal path:    x = 42      → Int64 (crashed before fix)

# Verify values, not just types
@assert f_try(true) == "error"
@assert f_try(false) == 42

# Non-last try/catch (exercises infer_stmt path, also fixed in Issue #9131).
# x is used after the try/catch — its inferred type must cover both branches.
function f_nontail(cond)
    local x
    try
        x = 1
        cond && error()
    catch
        x = "fallback"
    end
    return string(x)
end

@assert f_nontail(false) == "1"
@assert f_nontail(true) == "fallback"

# Bool vs String: different non-numeric types, same shape as original MWE.
function f_bool_str(cond)
    local v
    try
        v = true
        cond && error("bang")
    catch
        v = "oops"
    end
    return v
end

@assert f_bool_str(false) === true
@assert f_bool_str(true) == "oops"

# try/catch/else (Julia-specific else = no-exception path).
function f_else(cond)
    local r
    try
        cond && error()
        r = :ok
    catch
        r = :err
    else
        r = :else_ok
    end
    return r
end

@assert f_else(false) == :else_ok
@assert f_else(true) == :err

println("try_catch_type_inference_9131: all assertions passed")
true
