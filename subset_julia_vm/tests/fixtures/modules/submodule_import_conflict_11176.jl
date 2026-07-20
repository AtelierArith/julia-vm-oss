using Test

module ProviderA11176
module Shared11176
winner() = :A
module Deeper11176
winner() = :deep_a
end
end
module OnlyA11176
winner() = :only_a
end
export Shared11176, OnlyA11176
end

module ProviderB11176
module Shared11176
winner() = :B
module Deeper11176
winner() = :deep_b
end
end
module OnlyB11176
winner() = :only_b
end
export Shared11176, OnlyB11176
end

module FunctionProvider11176
shared_function_11176() = :function_binding
Shared11176() = :function_named_like_module
module AliasModule11176
winner() = :alias_module
end
end

module TypeProvider11176
struct ImportedType11176 end
struct Shared11176 end
shared_function_11176() = :type_provider_function
end

module AliasTypeProvider11176
struct OwnedType11176 end
const OwnedAlias11176 = OwnedType11176
end

module NoExportProvider11176
module Hidden11176
winner() = :must_stay_hidden
end
end

# A nonselective export collision is ambiguous across every binding kind, not
# only when both providers export modules.
module ExportedFunctionProvider11176
export CrossKind11176
CrossKind11176() = :function
end

module ExportedValueProvider11176
export CrossKind11176
const CrossKind11176 = :value
end

module ExportedTypeProvider11176
export CrossKind11176
struct CrossKind11176 end
end

module ExportedModuleProvider11176
export CrossKind11176
module CrossKind11176 end
end

module ExportedFunctionModuleConflict11176
using ..ExportedFunctionProvider11176
using ..ExportedModuleProvider11176
binding() = CrossKind11176
end

module ExportedValueModuleConflict11176
using ..ExportedValueProvider11176
using ..ExportedModuleProvider11176
binding() = CrossKind11176
end

module ExportedTypeModuleConflict11176
using ..ExportedTypeProvider11176
using ..ExportedModuleProvider11176
binding() = CrossKind11176
end

module ExplicitIntValueProvider11176
const Int = :explicit_int_value
end

module ExplicitIntValueConsumer11176
import ..ExplicitIntValueProvider11176: Int
value() = Int
end

# A later nonselective `using` binds both the exported names and the module
# root, but neither binding exists before the statement executes (Issue
# #11240 review regression).
module FutureUsingRootOuter11240
module Provider
f() = :future_using_root
end
module Client
function before_using()
    try
        Provider.f()
        false
    catch err
        err isa UndefVarError
    end
end
before = before_using()
using ..Provider
after = Provider.f()
end
end

# Imported/renamed module values retain the authoritative runtime ModuleValue,
# including `public` metadata used by reflection (Issue #11240 review
# regression).
module PublicModuleProvider11240
public visible
visible = 1
end

module PublicModuleConsumer11240
import ..PublicModuleProvider11240 as Provider
is_visible_public = Base.ispublic(Provider, :visible)
end

# A provider-owned constant that happens to share a Base function spelling is
# not a Base generic re-export.
module BaseNameConstantProvider11240
export sin
const sin = 42
end

module BaseNameConstantConsumer11240
using ..BaseNameConstantProvider11240: sin
value = sin
end

# Explicit selection from a facade can still denote an inherited Base binding;
# selection provenance must not prevent canonical Base ownership.
module BaseInheritedFacade11240
export sin
end

module BaseInheritedConsumer11240
using ..BaseInheritedFacade11240: sin
value = sin(0.0)
end

# A nonselective `using` binds its module root only when the destination scope
# does not already own a module with that name.
module UsingRootOther11240
module Provider
value = :other
end
end

module UsingRootLocalOuter11240
module Provider
value = :local
end
using ..UsingRootOther11240.Provider
reflected = getfield(@__MODULE__, :Provider)
end

module LetAliasShadow11176
import ..ProviderA11176 as FutureQ11176
function before_initialization()
    let
        result = try
            FutureQ11176.Shared11176
            false
        catch err
            err isa UndefVarError
        end
        FutureQ11176 = nothing
        result
    end
end
end

module LiveModuleAliasConsumer11176
import ..ProviderA11176 as LiveQ11176
reflected() = getfield(@__MODULE__, :LiveQ11176)
end

module LiveValueProvider11176
live_value_11176 = 1
end

module LiveValueConsumer11176
import ..LiveValueProvider11176: live_value_11176 as live_y_11176
read() = live_y_11176
end

module UndefinedCallableProvider11176
global undefined_callable_11176
end

module UndefinedCallableConsumer11176
import ..UndefinedCallableProvider11176: undefined_callable_11176
effects = Int[]
touch() = (push!(effects, 1); 1)
call() = undefined_callable_11176(touch())
effect_count() = length(effects)
end

module ReflectedAliasConsumer11176
import ..AliasTypeProvider11176: OwnedAlias11176
reflected() = getfield(@__MODULE__, :OwnedAlias11176)
end

# The source order of UsingImport entries is semantic: the first conflicting
# submodule binding wins, while reversing the statements reverses the winner.
module ConflictAB11176
using ..ProviderA11176: Shared11176
using ..ProviderB11176: Shared11176
winner() = Shared11176.winner()
deep_winner() = Shared11176.Deeper11176.winner()
bound() = Shared11176
description() = string(Shared11176)
shadow(Shared11176) = Shared11176.winner()
local_shadow() = begin
    Shared11176 = (; winner = () -> :local_assignment)
    Shared11176.winner()
end
field_shadow(Shared11176::Module) = Shared11176.winner
capture_field(Shared11176) = () -> Shared11176.winner
end

module ConflictBA11176
using ..ProviderB11176: Shared11176
using ..ProviderA11176: Shared11176
winner() = Shared11176.winner()
deep_winner() = Shared11176.Deeper11176.winner()
bound() = Shared11176
description() = string(Shared11176)
end

# Explicit module-valued `as` aliases use the same conflict table.
module ExplicitAB11176
import ..ProviderA11176.Shared11176 as Explicit11176
import ..ProviderB11176.Shared11176 as Explicit11176
winner() = Explicit11176.winner()
bound() = Explicit11176
description() = string(Explicit11176)
end

module ExplicitBA11176
import ..ProviderB11176.Shared11176 as Explicit11176
import ..ProviderA11176.Shared11176 as Explicit11176
winner() = Explicit11176.winner()
bound() = Explicit11176
description() = string(Explicit11176)
end

# Explicit conflicts are source-ordered even when the binding kinds differ.
module FunctionThenModule11176
import ..FunctionProvider11176: shared_function_11176 as Mixed11176
import ..ProviderB11176.Shared11176 as Mixed11176
winner() = Mixed11176()
end

module ModuleThenFunction11176
import ..ProviderA11176.Shared11176 as Mixed11176
import ..FunctionProvider11176: shared_function_11176 as Mixed11176
winner() = Mixed11176.winner()
end

module NamedFunctionThenModule11176
import ..FunctionProvider11176: Shared11176
import ..ProviderB11176: Shared11176
winner() = Shared11176()
end

# Renames within one import statement share a source span. Their source paths
# still distinguish the first winner from a later conflicting assignment.
module SameStmtFunctionThenModule11176
import ..FunctionProvider11176: shared_function_11176 as SameStmt11176, AliasModule11176 as SameStmt11176
winner() = SameStmt11176()
end

module SameStmtModuleThenFunction11176
import ..FunctionProvider11176: AliasModule11176 as SameStmt11176, shared_function_11176 as SameStmt11176
winner() = SameStmt11176.winner()
end

module SameStmtTypeThenFunction11176
import ..TypeProvider11176: ImportedType11176 as SameStmtType11176, shared_function_11176 as SameStmtType11176
winner() = SameStmtType11176() isa SameStmtType11176
end

module SameStmtFunctionThenType11176
import ..TypeProvider11176: shared_function_11176 as SameStmtType11176, ImportedType11176 as SameStmtType11176
winner() = SameStmtType11176()
rejects_as_type() = try
    SameStmtType11176 <: Any
    false
catch
    true
end
end

# Type aliases share the same first-wins table as expression bindings. A later
# type import must not make a module/function winner type-valued to the compiler.
module ModuleThenType11176
import ..ProviderA11176.Shared11176 as MixedType11176
import ..TypeProvider11176: ImportedType11176 as MixedType11176
winner() = MixedType11176.winner()
rejects_as_type() = try
    MixedType11176 <: Any
    false
catch
    true
end
end

module FunctionThenType11176
import ..FunctionProvider11176: shared_function_11176 as MixedType11176
import ..TypeProvider11176: ImportedType11176 as MixedType11176
winner() = MixedType11176()
rejects_as_type() = try
    MixedType11176 <: Any
    false
catch
    true
end
end

module NamedFunctionThenType11176
import ..FunctionProvider11176: Shared11176
import ..TypeProvider11176: Shared11176
winner() = Shared11176()
rejects_as_type() = try
    Shared11176 <: Any
    false
catch
    true
end
end

module QualifiedAliasConsumer11176
import ..AliasTypeProvider11176: OwnedAlias11176 as ImportedOwned11176
construct() = ImportedOwned11176()
typed(x::ImportedOwned11176) = :typed_owned
end

# Two nonselective exports of different submodules make the name ambiguous.
module ExportedAB11176
using ..ProviderA11176
using ..ProviderB11176
bound() = Shared11176
winner() = Shared11176.winner()
deep_winner() = Shared11176.Deeper11176.winner()
direct_call() = Shared11176()
direct_splat(xs) = Shared11176(xs...)
side_effects = Int[]
touch_arg() = (push!(side_effects, 1); 1)
direct_effect() = Shared11176(touch_arg())
side_effect_count() = length(side_effects)
end

module ExportedBA11176
using ..ProviderB11176
using ..ProviderA11176
bound() = Shared11176
winner() = Shared11176.winner()
deep_winner() = Shared11176.Deeper11176.winner()
end

module NoExportConsumer11176
using ..NoExportProvider11176
bound() = Hidden11176
winner() = Hidden11176.winner()
end

# An explicit selective binding wins over a conflicting nonselective export,
# independent of which statement appears first.
module SelectiveThenExported11176
using ..ProviderA11176: Shared11176
using ..ProviderB11176
bound() = Shared11176
winner() = Shared11176.winner()
description() = string(Shared11176)
end

module ExportedThenSelective11176
using ..ProviderA11176
using ..ProviderB11176: Shared11176
bound() = Shared11176
winner() = Shared11176.winner()
description() = string(Shared11176)
end

# `import M: S` is explicit and follows the same precedence rules as selective
# `using M: S`.
module ImportThenExported11176
import ..ProviderA11176: Shared11176
using ..ProviderB11176
bound() = Shared11176
winner() = Shared11176.winner()
description() = string(Shared11176)
end


module ExportedThenImport11176
using ..ProviderA11176
import ..ProviderB11176: Shared11176
bound() = Shared11176
winner() = Shared11176.winner()
description() = string(Shared11176)
end

# Re-importing the identical canonical target is idempotent.
module Duplicate11176
using ..ProviderA11176: Shared11176
using ..ProviderA11176: Shared11176
winner() = Shared11176.winner()
bound() = Shared11176
description() = string(Shared11176)
end

# Distinct alias keys never compete, regardless of per-statement symbol-set
# iteration order.
module NonConflict11176
using ..ProviderA11176: OnlyA11176
using ..ProviderB11176: OnlyB11176
both() = (OnlyA11176.winner(), OnlyB11176.winner())
end

# A nested module gets the same first-wins rule from its own ordered imports.
module Nested11176
module Child
using ...ProviderA11176: Shared11176
using ...ProviderB11176: Shared11176
winner() = Shared11176.winner()
bound() = Shared11176
description() = string(Shared11176)
end
end

# Main and a Main-owned function use the same resolved alias identity.
using .ProviderA11176: OnlyA11176
main_value = OnlyA11176.winner()
main_function11176() = OnlyA11176.winner()
import .LiveValueProvider11176: live_value_11176 as main_live_y_11176
LiveValueProvider11176.live_value_11176 = 2
# Whole-program global inference knows this name, but nested modules do not
# inherit Main bindings and must not use it to escape export ambiguity.
Shared11176 = (args...) -> :main_leak

@test ConflictAB11176.winner() === :A
@test ConflictBA11176.winner() === :B
@test ConflictAB11176.deep_winner() === :deep_a
@test ConflictBA11176.deep_winner() === :deep_b
@test ConflictAB11176.bound() === ProviderA11176.Shared11176
@test ConflictBA11176.bound() === ProviderB11176.Shared11176
@test ConflictAB11176.description() == string(ProviderA11176.Shared11176)
@test ConflictBA11176.description() == string(ProviderB11176.Shared11176)
@test ExplicitAB11176.winner() === :A
@test ExplicitBA11176.winner() === :B
@test ExplicitAB11176.bound() === ProviderA11176.Shared11176
@test ExplicitBA11176.bound() === ProviderB11176.Shared11176
@test ExplicitAB11176.description() == string(ProviderA11176.Shared11176)
@test ExplicitBA11176.description() == string(ProviderB11176.Shared11176)
@test FunctionThenModule11176.winner() === :function_binding
@test ModuleThenFunction11176.winner() === :A
@test NamedFunctionThenModule11176.winner() === :function_named_like_module
@test SameStmtFunctionThenModule11176.winner() === :function_binding
@test SameStmtModuleThenFunction11176.winner() === :alias_module
@test SameStmtTypeThenFunction11176.winner()
@test SameStmtFunctionThenType11176.winner() === :type_provider_function
@test SameStmtFunctionThenType11176.rejects_as_type()
@test ModuleThenType11176.winner() === :A
@test ModuleThenType11176.rejects_as_type()
@test FunctionThenType11176.winner() === :function_binding
@test FunctionThenType11176.rejects_as_type()
@test NamedFunctionThenType11176.winner() === :function_named_like_module
@test NamedFunctionThenType11176.rejects_as_type()
@test NamedFunctionThenType11176.Shared11176() === :function_named_like_module
@test QualifiedAliasConsumer11176.construct() isa AliasTypeProvider11176.OwnedType11176
@test QualifiedAliasConsumer11176.typed(AliasTypeProvider11176.OwnedType11176()) === :typed_owned
@test ExplicitIntValueConsumer11176.value() === :explicit_int_value
@test FutureUsingRootOuter11240.Client.before
@test FutureUsingRootOuter11240.Client.after === :future_using_root
@test PublicModuleConsumer11240.is_visible_public
@test BaseNameConstantConsumer11240.value == 42
@test BaseInheritedConsumer11240.value == 0.0
@test UsingRootLocalOuter11240.reflected.value === :local
@test getfield(UsingRootLocalOuter11240, :Provider).value === :local
@test LetAliasShadow11176.before_initialization()
@test LiveModuleAliasConsumer11176.reflected() === ProviderA11176
@test LiveValueConsumer11176.read() == 2
@test main_live_y_11176 == 2
@test ReflectedAliasConsumer11176.reflected() === AliasTypeProvider11176.OwnedType11176

function throws_undef_11176(f)
    try
        f()
        false
    catch err
        err isa UndefVarError
    end
end

@test throws_undef_11176(ExportedAB11176.bound)
@test throws_undef_11176(ExportedAB11176.winner)
@test throws_undef_11176(ExportedAB11176.deep_winner)
@test throws_undef_11176(ExportedBA11176.bound)
@test throws_undef_11176(ExportedBA11176.winner)
@test throws_undef_11176(ExportedBA11176.deep_winner)
@test throws_undef_11176(ExportedAB11176.direct_call)
@test throws_undef_11176(() -> ExportedAB11176.direct_splat((1, 2)))
@test throws_undef_11176(ExportedAB11176.direct_effect)
@test ExportedAB11176.side_effect_count() == 0
@test throws_undef_11176(NoExportConsumer11176.bound)
@test throws_undef_11176(NoExportConsumer11176.winner)
@test throws_undef_11176(ExportedFunctionModuleConflict11176.binding)
@test throws_undef_11176(ExportedValueModuleConflict11176.binding)
@test throws_undef_11176(ExportedTypeModuleConflict11176.binding)
@test throws_undef_11176(UndefinedCallableConsumer11176.call)
@test UndefinedCallableConsumer11176.effect_count() == 0

@test SelectiveThenExported11176.bound() === ProviderA11176.Shared11176
@test SelectiveThenExported11176.winner() === :A
@test SelectiveThenExported11176.description() == string(ProviderA11176.Shared11176)
@test ExportedThenSelective11176.bound() === ProviderB11176.Shared11176
@test ExportedThenSelective11176.winner() === :B
@test ExportedThenSelective11176.description() == string(ProviderB11176.Shared11176)
@test ImportThenExported11176.bound() === ProviderA11176.Shared11176
@test ImportThenExported11176.winner() === :A
@test ImportThenExported11176.description() == string(ProviderA11176.Shared11176)
@test ExportedThenImport11176.bound() === ProviderB11176.Shared11176
@test ExportedThenImport11176.winner() === :B
@test ExportedThenImport11176.description() == string(ProviderB11176.Shared11176)
@test Duplicate11176.winner() === :A
@test Duplicate11176.bound() === ProviderA11176.Shared11176
@test Duplicate11176.description() == string(ProviderA11176.Shared11176)
@test NonConflict11176.both() === (:only_a, :only_b)
@test Nested11176.Child.winner() === :A
@test Nested11176.Child.bound() === ProviderA11176.Shared11176
@test Nested11176.Child.description() == string(ProviderA11176.Shared11176)
@test main_value === :only_a
@test main_function11176() === :only_a
@test ConflictAB11176.shadow((; winner = () -> :local)) === :local
@test ConflictAB11176.local_shadow() === :local_assignment
@test ConflictAB11176.field_shadow(ProviderB11176.Shared11176) === ProviderB11176.Shared11176.winner
@test ConflictAB11176.capture_field((; winner = :captured))() === :captured

true
