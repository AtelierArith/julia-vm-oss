# Issue #11147: constructor values and runtime type arguments must preserve the
# same field conversion and catchable exception behavior as direct calls.

struct RuntimeBox11147{T}
    value::T
end

struct RuntimePlain11147
    value::Int64
end

struct RuntimeTarget11147
    value::String
end

Base.convert(::Type{RuntimeTarget11147}, value::String) = RuntimeTarget11147(value)

struct RuntimeHolder11147
    value::RuntimeTarget11147
end

struct RuntimeConvertible11147
end

Base.convert(::Type{Int64}, ::RuntimeConvertible11147) = 42

struct RuntimeIntHolder11147
    value::Int64
end

struct RuntimePair11147{T}
    x::T
    n::Int64
end

function bound_float_11147()
    g = RuntimeBox11147{Float64}
    g("abc")
end

function bound_int_11147()
    g = RuntimeBox11147{Int64}
    g(1.5)
end

function bound_plain_11147()
    g = RuntimePlain11147
    g(1.5)
end

function bound_holder_11147()
    g = RuntimeHolder11147
    g("abc")
end

function runtime_custom_convert_11147(value)
    RuntimeIntHolder11147(value)
end

function runtime_type_argument_11147(t)
    RuntimeBox11147{t}("abc")
end

function check_bound_method_11147()
    try
        bound_float_11147()
        false
    catch e
        e isa MethodError
    end
end

function check_bound_inexact_11147()
    try
        bound_int_11147()
        false
    catch e
        e isa InexactError
    end
end

function check_plain_inexact_11147()
    try
        bound_plain_11147()
        false
    catch e
        e isa InexactError
    end
end

function check_runtime_holder_convert_11147()
    try
        holder = bound_holder_11147()
        holder.value isa RuntimeTarget11147 && holder.value.value == "abc"
    catch
        false
    end
end

function check_runtime_custom_convert_11147()
    try
        holder = runtime_custom_convert_11147(RuntimeConvertible11147())
        holder.value == 42 && typeof(holder.value) == Int64
    catch
        false
    end
end

function check_runtime_type_method_11147()
    try
        runtime_type_argument_11147(Float64)
        false
    catch e
        e isa MethodError
    end
end

function check_map_method_11147()
    try
        map(RuntimeBox11147{Float64}, ["abc"])
        false
    catch e
        e isa MethodError
    end
end

function check_bare_outer_method_11147()
    try
        RuntimePair11147(1.0, 1.5)
        false
    catch e
        e isa MethodError
    end
end

function check_explicit_convert_11147()
    try
        pair = RuntimePair11147{Float64}(1, 2)
        pair.x == 1.0 && typeof(pair.x) == Float64 && pair.n == 2 && typeof(pair.n) == Int64
    catch
        false
    end
end

bound_method = check_bound_method_11147()
bound_inexact = check_bound_inexact_11147()
plain_inexact = check_plain_inexact_11147()
holder_convert = check_runtime_holder_convert_11147()
custom_convert = check_runtime_custom_convert_11147()
runtime_type_method = check_runtime_type_method_11147()
map_method = check_map_method_11147()
bare_outer_method = check_bare_outer_method_11147()
explicit_convert = check_explicit_convert_11147()

println("bound_method=", bound_method)
println("bound_inexact=", bound_inexact)
println("plain_inexact=", plain_inexact)
println("holder_convert=", holder_convert)
println("custom_convert=", custom_convert)
println("runtime_type_method=", runtime_type_method)
println("map_method=", map_method)
println("bare_outer_method=", bare_outer_method)
println("explicit_convert=", explicit_convert)

runtime_ok = bound_method && bound_inexact && plain_inexact && holder_convert && custom_convert &&
             runtime_type_method && map_method && bare_outer_method && explicit_convert
if !runtime_ok
    error("default constructor runtime conversion contract failed")
end

true
