inc_field_collect_4265(x) = x + 1

function generator_field_collect_4265()
    g = Base.Generator(inc_field_collect_4265, [1, 2, 3])

    if g.iter != [1, 2, 3]
        error("g.iter did not project the wrapped iterator")
    end
    if getfield(g, :iter) != [1, 2, 3]
        error("getfield(g, :iter) did not project the wrapped iterator")
    end
    if getfield(g, 2) != [1, 2, 3]
        error("getfield(g, 2) did not project the wrapped iterator")
    end

    f = g.f
    if f(41) != 42
        error("g.f did not project a callable function")
    end
    if getfield(g, :f)(9) != 10
        error("getfield(g, :f) did not project a callable function")
    end
    if getfield(g, 1)(10) != 11
        error("getfield(g, 1) did not project a callable function")
    end

    pure_values = Base._collect(1:1, g, Base.IteratorEltype(g), Base.IteratorSize(g))
    if pure_values != [2, 3, 4]
        error("Base._collect(::Generator) did not apply the projected callable")
    end
    if typeof(pure_values) != Vector{Int64}
        error("Base._collect(::Generator) did not preserve Vector{Int64}")
    end

    values = collect(g)
    if values != [2, 3, 4]
        error("collect(::Base.Generator) did not apply the projected callable")
    end
    if typeof(values) != Vector{Int64}
        error("collect(::Base.Generator) did not preserve Vector{Int64}")
    end

    return true
end

generator_field_collect_4265()
