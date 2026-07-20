# Test @macroexpand macro
# Shows the expansion of a macro call

macro double(x)
    quote
        2 * $x
    end
end

# @macroexpand returns the expanded expression without evaluating it.
result = @macroexpand @double 5

result isa Expr && eval(result) == 10
