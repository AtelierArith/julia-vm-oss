# Test user-defined promote_rule for custom types
# Issue #2557

struct MyReal
    value::Float64
end

function promote_rule(::Type{MyReal}, ::Type{Float64})
    MyReal
end

# Test promote_type dispatches through user-defined promote_rule
result = promote_type(MyReal, Float64)
result === MyReal
