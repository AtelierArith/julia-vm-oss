# Explicit globals inside module-owned functions and try clauses resolve to
# the owning module's frame-0 binding (Issue #11312).

using Test

module QualifiedGlobal11312
x = 0
y = 0

function set_x_11312()
    global x
    x = 1
end

function set_y_11312()
    try
        error("route through catch")
    catch
        global y
        y = 2
    end
end

set_x_11312()
set_y_11312()
end

@test QualifiedGlobal11312.x == 1
@test QualifiedGlobal11312.y == 2

true
