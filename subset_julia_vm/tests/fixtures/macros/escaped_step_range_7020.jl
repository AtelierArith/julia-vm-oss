macro macros_passthrough_7020(e)
    quote
        $(esc(e))
    end
end

r = @macros_passthrough_7020(0:0.1:0.3)
length(r) == 4 &&
    r[1] == 0.0 &&
    r[2] == 0.1 &&
    r[3] == 0.2 &&
    abs(r[4] - 0.3) < 1.0e-12
