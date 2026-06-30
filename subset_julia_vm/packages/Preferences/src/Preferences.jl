module Preferences

export @load_preference, @set_preferences!, load_preference, set_preferences!

load_preference(args...; kwargs...) = nothing
set_preferences!(args...; kwargs...) = nothing

macro load_preference(args...)
    nothing
end

macro set_preferences!(args...)
    nothing
end

end # module Preferences
