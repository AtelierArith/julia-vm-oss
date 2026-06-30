# Issue #7915: an imported macro can return an upstream-style Expr(:struct, ...)
# definition, and the expanded struct must be registered in the call-site module.

module Provider
export @m

macro m(ex)
    esc(ex)
end

end

module Consumer
using ..Provider: @m

@m struct Box
    x::Int
end

end

box = Consumer.Box(7)
isdefined(Consumer, :Box) && (box.x == 7)
