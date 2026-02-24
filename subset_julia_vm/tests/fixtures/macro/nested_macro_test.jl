# Test nested macro calls
# A macro that calls another macro in its expansion

macro inner(x)
    quote
        $x + 1
    end
end

macro outer(x)
    quote
        # @inner($x) * 2 is now parsed correctly as (@inner($x)) * 2
        # thanks to the parser fix for parenthesized macro call syntax
        @inner($x) * 2
    end
end

# @outer(5) should expand to:
# (@inner(5)) * 2 = (5 + 1) * 2 = 12
result = @outer 5

Float64(result)
