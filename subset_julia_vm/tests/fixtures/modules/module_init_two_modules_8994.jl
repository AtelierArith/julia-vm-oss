# Test that __init__ is called for multiple modules, each independently (Issue #8994)
module A
    const LOG = String[]
    __init__() = push!(LOG, "A_init")
end

module B
    const LOG = String[]
    __init__() = push!(LOG, "B_init")
end

println(A.LOG)
println(B.LOG)
A.LOG == ["A_init"] && B.LOG == ["B_init"]
