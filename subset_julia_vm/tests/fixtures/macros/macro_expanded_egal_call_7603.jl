macro expanded_egal_call_7603(a, b)
    :($(esc(a)) === $(esc(b)))
end

x = nothing
@expanded_egal_call_7603(x, nothing)
