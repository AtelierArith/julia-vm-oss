# Test basic module __init__() call semantics (Issue #8994)
# __init__ should be called after the module body finishes evaluating.
module M
    const LOG = String[]
    __init__() = push!(LOG, "initialized")
end
println(M.LOG)
M.LOG == ["initialized"]
