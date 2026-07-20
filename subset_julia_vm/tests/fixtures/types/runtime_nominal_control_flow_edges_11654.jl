if false
    struct SkippedIfStruct11654
        x::Int
    end
    abstract type SkippedIfAbstract11654 end
    primitive type SkippedIfPrimitive11654 8 end
    @enum SkippedIfEnum11654 skipped_if_a11654 skipped_if_b11654
end

for skipped_iteration11654 in 1:0
    struct SkippedForStruct11654
        x::Int
    end
    abstract type SkippedForAbstract11654 end
    primitive type SkippedForPrimitive11654 8 end
    @enum SkippedForEnum11654 skipped_for_a11654 skipped_for_b11654
end

expr_try_value11654 = try
    if true
        struct ExprTryStruct11654
            x::Int
        end
    end
    41
catch
    -1
end

missing_parent_caught11654 = try
    abstract type MissingParentChild11654 <: LaterParent11654 end
    false
catch e
    e isa UndefVarError
end
abstract type LaterParent11654 end

if true
    struct RuntimeBeforeRootStructEdge11654
        x::Int
    end
    abstract type RuntimeBeforeRootAbstractEdge11654 end
    primitive type RuntimeBeforeRootPrimitiveEdge11654 16 end
    @enum RuntimeBeforeRootEnumEdge11654 runtime_edge_a11654 runtime_edge_b11654
end
struct RootAfterRuntimeStructEdge11654
    x::Int
end
abstract type RootAfterRuntimeAbstractEdge11654 end
primitive type RootAfterRuntimePrimitiveEdge11654 32 end
@enum RootAfterRuntimeEnumEdge11654 root_edge_a11654 root_edge_b11654

collision_member11654 = 99
enum_collision_caught11654 = try
    if true
        @enum RuntimeCollisionEnum11654 first_collision_member11654 collision_member11654 after_collision_member11654
    end
    false
catch e
    e isa ErrorException
end

ParentValue11687 = 1
invalid_parent_caught11687 = try
    abstract type InvalidParent11687 <: ParentValue11687 end
    false
catch e
    e isa ErrorException
end
missing_field_caught11687 = try
    struct MissingFieldStruct11687
        x::MissingFieldType11687
    end
    false
catch e
    e isa UndefVarError
end
invalid_width_caught11687 = try
    primitive type InvalidWidth11687 7 end
    false
catch e
    e isa ErrorException
end

if true
    struct RuntimeSignatureType11688
        x::Int
    end
end
runtime_signature_value11688(x::RuntimeSignatureType11688) = x.x

for iteration11684 in 1:2
    struct LoopStruct11684
        x::Int
    end
end
for iteration11684 in 1:2
    abstract type LoopAbstract11684 end
end
for iteration11684 in 1:2
    primitive type LoopPrimitive11684 8 end
end

if true
    struct MixedStruct11684
        x::Int
    end
end
mixed_struct_before11684 = MixedStruct11684(19)
struct MixedStruct11684
    x::Int
end
if true
    abstract type MixedAbstract11684 end
end
abstract type MixedAbstract11684 end
if true
    primitive type MixedPrimitive11684 8 end
end
primitive type MixedPrimitive11684 8 end
if true
    @enum MixedEnum11684 mixed_enum_a11684 mixed_enum_b11684
end
mixed_enum_before11684 = mixed_enum_a11684
@enum MixedEnum11684 mixed_enum_a11684 mixed_enum_b11684

struct RootFirstStruct11684
    x::Int
end
root_first_struct_before11684 = RootFirstStruct11684(23)
if true
    struct RootFirstStruct11684
        x::Int
    end
end
abstract type RootFirstAbstract11684 end
if true
    abstract type RootFirstAbstract11684 end
end
primitive type RootFirstPrimitive11684 8 end
if true
    primitive type RootFirstPrimitive11684 8 end
end
@enum RootFirstEnum11684 root_first_a11684 root_first_b11684
root_first_enum_before11684 = root_first_a11684
if true
    @enum RootFirstEnum11684 root_first_a11684 root_first_b11684
end

module RuntimeOwnerModule11686
if true
    abstract type RuntimeOwnedAbstract11686 end
end
if true
    struct RuntimeOwnedConcrete11686
        x::Int
    end
end
if true
    primitive type RuntimeOwnedPrimitive11686 8 end
end
if true
    @enum RuntimeOwnedEnum11686 runtime_owned_a11686 runtime_owned_b11686
end
struct RuntimeOwnedStruct11686 <: RuntimeOwnedAbstract11686 end
end

if true
    struct RuntimeInnerCtor11679
        x::Int
        RuntimeInnerCtor11679(x) = new(x + 1)
    end
end

if false
    struct SkippedRuntimeInnerCtor11679
        x::Int
        SkippedRuntimeInnerCtor11679(x) = new(x + 1)
    end
end

runtime_inner_value11679 = RuntimeInnerCtor11679(7).x
runtime_inner_default_suppressed11679 = try
    RuntimeInnerCtor11679(1, 2)
    false
catch e
    e isa MethodError
end

@assert !isdefined(Main, :SkippedIfStruct11654)
@assert !isdefined(Main, :SkippedIfAbstract11654)
@assert !isdefined(Main, :SkippedIfPrimitive11654)
@assert !isdefined(Main, :SkippedIfEnum11654)
@assert !isdefined(Main, :SkippedForStruct11654)
@assert !isdefined(Main, :SkippedForAbstract11654)
@assert !isdefined(Main, :SkippedForPrimitive11654)
@assert !isdefined(Main, :SkippedForEnum11654)
@assert expr_try_value11654 == 41
@assert ExprTryStruct11654(7).x == 7
@assert missing_parent_caught11654
@assert !isdefined(Main, :MissingParentChild11654)
@assert isdefined(Main, :LaterParent11654)
@assert RuntimeBeforeRootStructEdge11654(19).x == 19
@assert RootAfterRuntimeStructEdge11654(23).x == 23
@assert RuntimeBeforeRootAbstractEdge11654 <: Any
@assert RootAfterRuntimeAbstractEdge11654 <: Any
@assert RuntimeBeforeRootPrimitiveEdge11654 <: Any
@assert RootAfterRuntimePrimitiveEdge11654 <: Any
@assert instances(RuntimeBeforeRootEnumEdge11654) == (runtime_edge_a11654, runtime_edge_b11654)
@assert instances(RootAfterRuntimeEnumEdge11654) == (root_edge_a11654, root_edge_b11654)
@assert enum_collision_caught11654
@assert isdefined(Main, :RuntimeCollisionEnum11654)
@assert isdefined(Main, :first_collision_member11654)
@assert collision_member11654 == 99
@assert isdefined(Main, :after_collision_member11654)
@assert invalid_parent_caught11687
@assert missing_field_caught11687
@assert invalid_width_caught11687
@assert !isdefined(Main, :InvalidParent11687)
@assert !isdefined(Main, :MissingFieldStruct11687)
@assert !isdefined(Main, :InvalidWidth11687)
@assert runtime_signature_value11688(RuntimeSignatureType11688(17)) == 17
@assert LoopStruct11684(11).x == 11
@assert LoopAbstract11684 <: Any
@assert LoopPrimitive11684 <: Any
@assert mixed_struct_before11684 isa MixedStruct11684
@assert mixed_struct_before11684.x == 19
@assert MixedAbstract11684 === MixedAbstract11684
@assert MixedPrimitive11684 === MixedPrimitive11684
@assert mixed_enum_before11684 === mixed_enum_a11684
@assert instances(MixedEnum11684) == (mixed_enum_a11684, mixed_enum_b11684)
@assert root_first_struct_before11684 isa RootFirstStruct11684
@assert root_first_struct_before11684.x == 23
@assert RootFirstAbstract11684 === RootFirstAbstract11684
@assert RootFirstPrimitive11684 === RootFirstPrimitive11684
@assert root_first_enum_before11684 === root_first_a11684
@assert instances(RootFirstEnum11684) == (root_first_a11684, root_first_b11684)
@assert RuntimeOwnerModule11686.RuntimeOwnedStruct11686 <: RuntimeOwnerModule11686.RuntimeOwnedAbstract11686
@assert RuntimeOwnerModule11686.RuntimeOwnedConcrete11686(5).x == 5
@assert RuntimeOwnerModule11686.RuntimeOwnedPrimitive11686 <: Any
@assert instances(RuntimeOwnerModule11686.RuntimeOwnedEnum11686) == (
    RuntimeOwnerModule11686.runtime_owned_a11686,
    RuntimeOwnerModule11686.runtime_owned_b11686,
)
@assert runtime_inner_value11679 == 8
@assert runtime_inner_default_suppressed11679
@assert !isdefined(Main, :SkippedRuntimeInnerCtor11679)

println((
    expr_try_value11654,
    missing_parent_caught11654,
    RuntimeBeforeRootStructEdge11654(19).x + RootAfterRuntimeStructEdge11654(23).x,
    enum_collision_caught11654,
    invalid_parent_caught11687 && missing_field_caught11687 && invalid_width_caught11687,
    runtime_signature_value11688(RuntimeSignatureType11688(17)),
    mixed_struct_before11684.x,
    mixed_enum_before11684,
    runtime_inner_value11679,
))

true
