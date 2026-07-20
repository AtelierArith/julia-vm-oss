module MacroToolsMatcherClient7451
using MacroTools: @capture

function capture_args()
    ex = :(f(10, 20))
    ok = @capture(ex, f(args__))
    ok && length(args) == 2 && args[1] == 10 && args[2] == 20
end
end

using MacroTools: @match

matched_7451 = @match :(2 + 3) begin
    a_ + b_ => a + b
    _ => 0
end

# Hygiene for a selectively imported macro defined in a nested module uses the
# complete defining path, not only its leaf module name (Issue #11240 review
# regression).
module NestedMacroOwner11240
module Inner
private_helper() = 42
macro nested_value()
    :(private_helper())
end
end
end

module NestedMacroClient11240
using ..NestedMacroOwner11240.Inner: @nested_value
value = @nested_value
end

MacroToolsMatcherClient7451.capture_args() && matched_7451 == 5 &&
    NestedMacroClient11240.value == 42
