# Test __init__ side effects visible after module definition (Issue #8994)
# __init__ should run after module body and before println is reached.
module M
    const LOG = String[]

    function __init__()
        push!(LOG, "init_called")
        push!(LOG, "second_entry")
    end
end

println(M.LOG)
M.LOG == ["init_called", "second_entry"]
