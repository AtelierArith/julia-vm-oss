using MacroTools: striplines

ex = quote
    f()
end

out = striplines(ex)
out isa Expr || error("striplines should return an Expr")
out.head == :block || error("striplines should preserve the block")
length(out.args) == 1 || error("striplines should remove LineNumberNode entries")

true
