# Test user-defined promote_rule for custom types
# Issues #2557, #4048

import Base: promote_rule

struct MyReal
    value::Float64
end

function promote_rule(::Type{MyReal}, ::Type{Float64})
    MyReal
end

# Test promote_type dispatches through user-defined promote_rule
result = promote_type(MyReal, Float64)
reverse_result = promote_type(Float64, MyReal)
result === MyReal && reverse_result === MyReal
