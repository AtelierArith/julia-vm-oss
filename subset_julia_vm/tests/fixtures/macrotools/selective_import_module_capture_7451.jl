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

MacroToolsMatcherClient7451.capture_args() && matched_7451 == 5
