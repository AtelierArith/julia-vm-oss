module StructPatternMacroArg7560

macro accept(args...)
    true
end

plain = @accept(ex, struct header_ body__ end)
mut = @accept(ex, mutable struct header_ body__ end)

plain === true || error("plain struct macro argument did not expand")
mut === true || error("mutable struct macro argument did not expand")

end

true
