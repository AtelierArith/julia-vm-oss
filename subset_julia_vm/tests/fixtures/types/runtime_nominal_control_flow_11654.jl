if true
    struct IfStruct11654
        x::Int
    end
end
if true
    abstract type IfAbstract11654 end
end
if true
    primitive type IfPrimitive11654 8 end
end
if true
    @enum IfEnum11654 if_a11654 if_b11654
end

for struct_iteration11654 in 1:1
    struct ForStruct11654
        x::Int
    end
end
for abstract_iteration11654 in 1:1
    abstract type ForAbstract11654 end
end
for primitive_iteration11654 in 1:1
    primitive type ForPrimitive11654 8 end
end
for enum_iteration11654 in 1:1
    @enum ForEnum11654 for_a11654 for_b11654
end

struct_ran11654 = true
while struct_ran11654
    global struct_ran11654 = false
    struct WhileStruct11654
        x::Int
    end
end
abstract_ran11654 = true
while abstract_ran11654
    global abstract_ran11654 = false
    abstract type WhileAbstract11654 end
end
primitive_ran11654 = true
while primitive_ran11654
    global primitive_ran11654 = false
    primitive type WhilePrimitive11654 8 end
end
enum_ran11654 = true
while enum_ran11654
    global enum_ran11654 = false
    @enum WhileEnum11654 while_a11654 while_b11654
end

try
    struct TryStruct11654
        x::Int
    end
catch
end
try
    abstract type TryAbstract11654 end
catch
end
try
    primitive type TryPrimitive11654 8 end
catch
end
try
    @enum TryEnum11654 try_a11654 try_b11654
catch
end

matrix11654 = (
    (@isdefined IfStruct11654),
    (@isdefined IfAbstract11654),
    (@isdefined IfPrimitive11654),
    (@isdefined IfEnum11654),
    (@isdefined ForStruct11654),
    (@isdefined ForAbstract11654),
    (@isdefined ForPrimitive11654),
    (@isdefined ForEnum11654),
    (@isdefined WhileStruct11654),
    (@isdefined WhileAbstract11654),
    (@isdefined WhilePrimitive11654),
    (@isdefined WhileEnum11654),
    (@isdefined TryStruct11654),
    (@isdefined TryAbstract11654),
    (@isdefined TryPrimitive11654),
    (@isdefined TryEnum11654),
)

println(matrix11654)
# Workaround: evaluate @isdefined outside @assert until nested macro expansion is supported (Issue #11677)
for_a_defined11654 = @isdefined for_a11654
for_b_defined11654 = @isdefined for_b11654
@assert matrix11654 == (
    true, true, true, true,
    true, true, true, false,
    true, true, true, true,
    true, true, true, true,
)
@assert !for_a_defined11654
@assert !for_b_defined11654
@assert IfStruct11654(7).x == 7
@assert IfPrimitive11654 <: Any
@assert instances(IfEnum11654) == (if_a11654, if_b11654)

true
