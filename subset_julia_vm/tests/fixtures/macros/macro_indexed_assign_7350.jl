# Issue #7350 (A2): an indexed assignment `a[i] = v` inside macro-expanded code
# must take effect (it was quoted as a malformed `Symbol("a[i]")` and no-oped).
macro fill_second()
    return quote
        a = [0, 0, 0]
        a[2] = 99
        a
    end
end

@fill_second() == [0, 99, 0]
