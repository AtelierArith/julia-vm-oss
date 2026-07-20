# Issue #11147: default constructors must use Julia's field conversion rules,
# and conversion failures must remain catchable with the upstream exception type.

struct StaticBox11147{T}
    value::T
end

struct StaticTarget11147
    value::String
end

Base.convert(::Type{StaticTarget11147}, value::String) = StaticTarget11147(value)

struct StaticHolder11147
    value::StaticTarget11147
end

function check_direct_method_11147()
    try
        StaticBox11147{Float64}("abc")
        false
    catch e
        e isa MethodError
    end
end

function check_direct_inexact_11147()
    try
        StaticBox11147{Int64}(1.5)
        false
    catch e
        e isa InexactError
    end
end

function check_direct_convert_11147()
    try
        box = StaticBox11147{Float64}(1)
        box.value == 1.0 && typeof(box.value) == Float64
    catch
        false
    end
end

function check_convert_method_11147()
    try
        convert(Float64, "1.5")
        false
    catch e
        e isa MethodError
    end
end

function check_static_holder_convert_11147()
    try
        holder = StaticHolder11147("abc")
        holder.value isa StaticTarget11147 && holder.value.value == "abc"
    catch
        false
    end
end

direct_method = check_direct_method_11147()
direct_inexact = check_direct_inexact_11147()
direct_convert = check_direct_convert_11147()
convert_method = check_convert_method_11147()
holder_convert = check_static_holder_convert_11147()

println("direct_method=", direct_method)
println("direct_inexact=", direct_inexact)
println("direct_convert=", direct_convert)
println("convert_method=", convert_method)
println("holder_convert=", holder_convert)

static_ok = direct_method && direct_inexact && direct_convert && convert_method && holder_convert
if !static_ok
    error("default constructor static conversion contract failed")
end

true
