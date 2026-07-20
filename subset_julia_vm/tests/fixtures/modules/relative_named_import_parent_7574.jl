using Test

module RelativeParentImport7574
x() = 1

module Child
import ..RelativeParentImport7574: x

println(x())
child_value() = x() + 41
end
end

module RelativeSiblingImport7574
module Source
source_value() = 99
end

module Sink
import ..Source: source_value

println(source_value())
sink_value() = source_value()
end

# A non-selective relative import binds the source module name (Issue #11137).
module QualifiedSink
import ..Source
qualified_sink_value() = Source.source_value()
end
end

# Module-valued import bindings are qualified roots in every supported import
# shape: selective import, exported name through `using`, and rename.
module ModuleImportShapes11157
module Source
module SelectedChild
value() = 1
end

module ExportedChild
value() = 2
end

module AliasedChild
value() = 3
end

selected_function() = 41
selected_value = 42
struct SelectedType
    x::Int
end
struct SelectedParam{T}
    x::T
end

export ExportedChild
end

module SelectiveSink
import ..Source: SelectedChild
value() = SelectedChild.value()
end

module UsingSink
using ..Source
value() = ExportedChild.value()
end

module AliasSink
import ..Source: AliasedChild as A
value() = A.value()
end

module SelectiveRenameSink
import ..Source: selected_function as renamed_function, selected_value as renamed_value, SelectedType as RenamedType, SelectedParam as RenamedParam
value() = (renamed_function(), renamed_value)
shadow_callable(renamed_function) = renamed_function()
renamed_type = RenamedType
construct_type() = RenamedType(3).x
typed(x::RenamedType) = x.x
subtype_result = RenamedType <: Any
shadow_type(RenamedType) = identity(RenamedType)
renamed_param = RenamedParam
construct_param() = RenamedParam{Int}(5).x
typed_param(x::RenamedParam{Int}) = x.x
end

module WholeAliasSink
import ..Source as D
value() = D.SelectedChild.value()
shadow_parameter(D) = identity(D)
shadow_assignment() = begin
    D = 42
    identity(D)
end
alias_module_name = string(D)
source_name_result = try
    Source
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
source_call_result = try
    Source.SelectedChild.value()
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

module UnrelatedSink
exported_child_result = try
    ExportedChild.value()
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
renamed_type_result = try
    RenamedType
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

module BuiltinTypeRenameSink
import Base: Int as I
f(x::I) = x + 1
value() = (I(3), f(4), I === Int)
end

parent_renamed_type_result = try
    RenamedType
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

# A nested module has its own lexical global scope.  The parent module name and
# unrelated top-level module names are not implicitly bound there (Issue #11132).
module UnrelatedModule11132
unrelated_value() = 11132
end

main_root_function11132() = UnrelatedModule11132.unrelated_value()

module LexicalParentVisibility11132
parent_value() = 11132
self_value = LexicalParentVisibility11132.parent_value()

module Child
parent_binding_result = try
    LexicalParentVisibility11132
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

parent_name_result = try
    LexicalParentVisibility11132.parent_value()
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

unrelated_name_result = try
    UnrelatedModule11132.unrelated_value()
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

# Ordinary modules implicitly use Base exports, including module-valued ones,
# but an unrelated stdlib root still requires an explicit using/import.
implicit_base_module_result = MathConstants.golden == Base.MathConstants.golden
random_name_result = try
    Random.seed!(1)
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

import ..LexicalParentVisibility11132: parent_value
selective_import_result = parent_value()
main_qualified_result = Main.LexicalParentVisibility11132.parent_value()
end
end

# Type-object short-name tables are identity caches, not inherited lexical
# bindings (Issue #11168).
module ParentTypeVisibility11168
struct ParentType end
module Child
parent_type_result = try
    ParentType
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
parent_ctor_result = try
    ParentType()
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end
end

# A local binding wins over a same-named module root, including before the
# small pure-function inliner (Issue #11165).
module LocalModuleShadow11165
f(x) = x + 100
g(LocalModuleShadow11165) = LocalModuleShadow11165.f(1)
end

# A baremodule gets Core/Main but no implicit Base binding (Issue #11162).
baremodule BareModuleVisibility11162
core_ok = Core isa Module
main_ok = Main isa Module
base_result = try
    Base
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
base_constant_result = try
    Base.pi
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
base_submodule_result = try
    Base.MathConstants.golden
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
sys_result = try
    Sys.WORD_SIZE
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
mathconstants_result = try
    MathConstants.golden
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

# Loading/knowing a stdlib module does not by itself create a Main binding.
# The failed callee lookup precedes argument evaluation (Issue #11158).
main_random_counter = [0]
main_random_result = try
    Random.seed!(main_random_counter[1] += 1)
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

stdlib_alias_result = try
    R11158 = Random
    R11158
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

# Even a never-known PascalCase root is a runtime UndefVarError, and its
# lookup happens before arguments (Issue #11161).
unknown_root_counter = [0]
unknown_root_result = try
    NeverDefined11161.f(unknown_root_counter[1] += 1)
catch err
    err isa UndefVarError ? :undef : :wrong_error
end

# Builtin type spellings still obey their Core/Base lexical owners (Issue #11419).
# The implicit Core spelling is intentional; qualified Core.UndefVarError is #11451.
baremodule BareBuiltinTypeNegative11419
isa_result = try
    1 isa BigInt
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
subtype_result = try
    BigInt <: Number
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
parametric_result = try
    Vector{Int64}
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

baremodule BareImportBaseOnly11419
import Base
result = try
    1 isa BigInt
catch err
    err isa UndefVarError ? :undef : :wrong_error
end
end

baremodule BareCoreTypes11419
f(x::Int64) = x
annotation_result = f(1)
isa_result = 1 isa Int64
subtype_result = Int64 <: Any
parametric_result = Type{Int64} <: Type
end

baremodule BareUsingBaseTypes11419
using Base
f(x::BigInt) = x
result = f(BigInt(1)) == BigInt(1) &&
         BigInt(1) isa BigInt &&
         BigInt <: Number &&
         Vector{Int64} <: AbstractVector{Int64}
end

baremodule BareNamedBaseType11419
import Base: BigInt
f(x::BigInt) = x
annotation_result = f(BigInt(1))
isa_result = BigInt(1) isa BigInt
subtype_result = BigInt <: Number
end

module OrdinaryBaseTypes11419
f(x::BigInt) = x
result = f(BigInt(1)) == BigInt(1) && BigInt(1) isa BigInt && BigInt <: Number
end

@test RelativeParentImport7574.Child.child_value() == 42
@test RelativeSiblingImport7574.Sink.sink_value() == 99
@test RelativeSiblingImport7574.QualifiedSink.qualified_sink_value() == 99
@test ModuleImportShapes11157.SelectiveSink.value() == 1
@test ModuleImportShapes11157.UsingSink.value() == 2
@test ModuleImportShapes11157.AliasSink.value() == 3
@test ModuleImportShapes11157.SelectiveRenameSink.value() == (41, 42)
@test ModuleImportShapes11157.SelectiveRenameSink.shadow_callable(() -> 9) == 9
@test ModuleImportShapes11157.SelectiveRenameSink.renamed_type === ModuleImportShapes11157.Source.SelectedType
@test ModuleImportShapes11157.SelectiveRenameSink.construct_type() == 3
@test ModuleImportShapes11157.SelectiveRenameSink.typed(ModuleImportShapes11157.Source.SelectedType(4)) == 4
@test ModuleImportShapes11157.SelectiveRenameSink.subtype_result
@test ModuleImportShapes11157.SelectiveRenameSink.shadow_type(7) == 7
@test ModuleImportShapes11157.SelectiveRenameSink.renamed_param === ModuleImportShapes11157.Source.SelectedParam
@test ModuleImportShapes11157.SelectiveRenameSink.construct_param() == 5
@test ModuleImportShapes11157.SelectiveRenameSink.typed_param(ModuleImportShapes11157.Source.SelectedParam{Int}(6)) == 6
@test ModuleImportShapes11157.WholeAliasSink.value() == 1
@test ModuleImportShapes11157.WholeAliasSink.shadow_parameter(42) == 42
@test ModuleImportShapes11157.WholeAliasSink.shadow_assignment() == 42
@test endswith(ModuleImportShapes11157.WholeAliasSink.alias_module_name, ".Source")
@test ModuleImportShapes11157.WholeAliasSink.source_name_result === :undef
@test ModuleImportShapes11157.WholeAliasSink.source_call_result === :undef
@test ModuleImportShapes11157.UnrelatedSink.exported_child_result === :undef
@test ModuleImportShapes11157.UnrelatedSink.renamed_type_result === :undef
@test ModuleImportShapes11157.BuiltinTypeRenameSink.value() == (3, 5, true)
@test ModuleImportShapes11157.parent_renamed_type_result === :undef
@test main_root_function11132() == 11132
@test LexicalParentVisibility11132.self_value == 11132
@test LexicalParentVisibility11132.Child.parent_binding_result === :undef
@test LexicalParentVisibility11132.Child.parent_name_result === :undef
@test LexicalParentVisibility11132.Child.unrelated_name_result === :undef
@test LexicalParentVisibility11132.Child.implicit_base_module_result
@test LexicalParentVisibility11132.Child.random_name_result === :undef
@test LexicalParentVisibility11132.Child.selective_import_result == 11132
@test LexicalParentVisibility11132.Child.main_qualified_result == 11132
@test ParentTypeVisibility11168.Child.parent_type_result === :undef
@test ParentTypeVisibility11168.Child.parent_ctor_result === :undef
@test LocalModuleShadow11165.g((; f = x -> x + 1)) == 2
@test BareModuleVisibility11162.core_ok
@test BareModuleVisibility11162.main_ok
@test BareModuleVisibility11162.base_result === :undef
@test BareModuleVisibility11162.base_constant_result === :undef
@test BareModuleVisibility11162.base_submodule_result === :undef
@test BareModuleVisibility11162.sys_result === :undef
@test BareModuleVisibility11162.mathconstants_result === :undef
@test main_random_result === :undef
@test main_random_counter[1] == 0
@test stdlib_alias_result === :undef
@test unknown_root_result === :undef
@test unknown_root_counter[1] == 0
@test BareBuiltinTypeNegative11419.isa_result === :undef
@test BareBuiltinTypeNegative11419.subtype_result === :undef
@test BareBuiltinTypeNegative11419.parametric_result === :undef
@test BareImportBaseOnly11419.result === :undef
@test BareCoreTypes11419.annotation_result == 1
@test BareCoreTypes11419.isa_result
@test BareCoreTypes11419.subtype_result
@test BareCoreTypes11419.parametric_result
@test BareUsingBaseTypes11419.result
@test BareNamedBaseType11419.annotation_result == BigInt(1)
@test BareNamedBaseType11419.isa_result
@test BareNamedBaseType11419.subtype_result
@test OrdinaryBaseTypes11419.result

true
