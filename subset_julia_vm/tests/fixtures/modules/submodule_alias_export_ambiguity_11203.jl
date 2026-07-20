# Issue #11203: two non-selective `using`s that export different bindings
# with the same bare name make that name ambiguous. Explicit selective imports
# still follow Julia's precedence rules.

using Test

module ExportConflict11203
module A
module Sub
greet() = "A"
end
export Sub
end

module B
module Sub
greet() = "B"
end
export Sub
end

using .A
using .B

bare_value() = Sub
bare_call() = Sub.greet()
end

module ExportThenExplicit11203
module A
module Sub
greet() = "A"
end
export Sub
end

module B
module Sub
greet() = "B"
end
export Sub
end

using .A
using .B: Sub
bare_call() = Sub.greet()
end

module ExplicitThenExport11203
module A
module Sub
greet() = "A"
end
export Sub
end

module B
module Sub
greet() = "B"
end
export Sub
end

using .A: Sub
using .B
bare_call() = Sub.greet()
end

module AssignedAlias11203
module A
module Sub
greet() = "A"
end
export Sub
end

using .A
const S = Sub
const assigned_alias_result = S.greet()
end

module ExplicitConflict11203
module A
module Sub
greet() = "A"
end
export Sub
end

module B
module Sub
greet() = "B"
end
export Sub
end

using .A: Sub
using .B: Sub
bare_call() = Sub.greet()
end

module OwnedSub11203
greet() = "owned"
end

module OwnedA11203
module OwnedSub11203
greet() = "A"
end
export OwnedSub11203
end

module OwnedB11203
module OwnedSub11203
greet() = "B"
end
export OwnedSub11203
end

using .OwnedA11203
using .OwnedB11203

# Cross-kind collisions use the same binding table: an exported module must not
# silently win over a same-named exported function/value/type.
module CrossKindModule11203
module Shared11203
end
export Shared11203
end

module CrossKindFunction11203
Shared11203() = "function"
export Shared11203
end

module CrossKindConflict11203
using ..CrossKindModule11203
using ..CrossKindFunction11203
bare_value() = Shared11203
end

# An explicit imported value owns its spelling even when that spelling is an
# implicitly available Base type name.
module ExplicitBaseTypeCollision11203
module Values
const String = "imported String value"
export String
end
using .Values: String
picked() = String
end

# A function-local qualified root is lexical for the whole function. Before its
# first assignment it raises UndefVarError rather than falling back to an
# imported module with the same name.
module LocalRootBeforeInit11203
module A
module Sub
greet() = "imported"
end
export Sub
end
using .A
function probe()
    before = try
        Sub.greet()
        nothing
    catch err
        typeof(err)
    end
    Sub = (greet = () -> "local",)
    (before, Sub.greet())
end
end

# Import renames are live bindings, not assignment-time snapshots. A renamed
# value observes a later mutation of its source binding, and a whole-module
# rename is installed as a real binding visible through module reflection.
module RenamedLiveAlias11203
module P
x = 1
export x
end
using .P: x as y
const before_mutation = y
P.x = 2
const after_mutation = y
import .P as Q
const reflected_module_alias = getfield(@__MODULE__, :Q) === P
end

# Issue #11216: alias visibility changes when each top-level `using` executes;
# compiling from the scope's final import set would make the first call below
# fail too early. The same already-compiled function must observe A before the
# conflict, ambiguity after B, and the explicit C binding afterward.
module TemporalAlias11216
module A
module Sub
greet() = "A"
end
export Sub
end

module B
module Sub
greet() = "B"
end
export Sub
end

module C
module Sub
greet() = "C"
end
export Sub
end

using .A
bare_call() = Sub.greet()
const before_conflict = bare_call()

using .B
const after_conflict_error = try
    bare_call()
    nothing
catch err
    typeof(err)
end

side_effect_count = 0
bump_side_effect() = (global side_effect_count += 1; side_effect_count)
const argument_order_error = try
    Sub.greet(bump_side_effect())
    nothing
catch err
    typeof(err)
end
const argument_side_effect_count = side_effect_count

using .C: Sub
const after_explicit = bare_call()

local_shadow(Sub) = Sub.greet()
const local_shadow_result = local_shadow((greet = () -> "local",))
end

# PR #11221 review: whole-program import metadata must not expose an exported
# value before the corresponding source-ordered `using` executes.
module FutureValueProvider11221
const x11221 = 42
export x11221
end

const before_value_using_error_11221 = try
    x11221
    nothing
catch err
    typeof(err)
end
using .FutureValueProvider11221
const after_value_using_11221 = x11221

# A function compiled before `using` must consult the same runtime activation
# state. Whole-scope method metadata must not make the future export callable.
module FutureFunctionProvider11216
export future_function_11216
future_function_11216() = 42
end

call_future_function_11216() = future_function_11216()
const before_function_using_error_11216 = try
    call_future_function_11216()
    nothing
catch err
    typeof(err)
end
using .FutureFunctionProvider11216
const after_function_using_11216 = call_future_function_11216()

# Issue #11228: plain `import M` binds only M, never M's exported members.
module PlainImportOnly11228
module Provider
export x
const x = 7
end

import .Provider
const bare_export_result = try
    x
catch err
    typeof(err)
end
const qualified_result = Provider.x
end

# Tightening plain `import M` must not erase the ordinary module's implicit
# `using Base`. Base is backed by the merged prelude rather than a regular IR
# module, so its live-binding inventory follows a dedicated compiler path.
module ImplicitBaseAfterImportFix11228
using Test
abstract_irrational_type() = AbstractIrrational
step_range_len_type() = StepRangeLen
end

# Issue #11132 source-order hardening: a future relative module import must not
# make the parent-owned module visible to an earlier expression or function.
module FutureParentModule11132
module Provider
f() = 9
end
module Consumer
call_provider() = Provider.f()
const before_expression = try
    Provider.f()
catch err
    typeof(err)
end
const before_function = try
    call_provider()
catch err
    typeof(err)
end
import ..Provider
const after_import = call_provider()
end
end

# Issue #11229: a static module-path alias is also a real runtime value.
module AssignedModuleValue11229 end
const AssignedModuleAlias11229 = AssignedModuleValue11229

# The same source-order rule applies while eagerly evaluating method
# signatures: a type exported by a known module is still undefined until the
# `using` executes, even though the whole-program struct table already knows it.
module FutureTypeProvider11221
export T11221
struct T11221 end
end

signature_before_using_error_11221 = nothing
try
    f_before_using_11221(x::T11221) = 1
catch err
    global signature_before_using_error_11221 = typeof(err)
end
using .FutureTypeProvider11221
f_after_using_11221(x::T11221) = 2

@testset "submodule alias export ambiguity (Issue #11203)" begin
    @test_throws UndefVarError ExportConflict11203.bare_value()
    @test_throws UndefVarError ExportConflict11203.bare_call()
    @test ExportConflict11203.A.Sub.greet() == "A"
    @test ExportConflict11203.B.Sub.greet() == "B"

    @test ExportThenExplicit11203.bare_call() == "B"
    @test ExplicitThenExport11203.bare_call() == "A"
    @test AssignedAlias11203.assigned_alias_result == "A"
    @test ExplicitConflict11203.bare_call() == "A"
    @test OwnedSub11203.greet() == "owned"
    @test_throws UndefVarError CrossKindConflict11203.bare_value()
    @test ExplicitBaseTypeCollision11203.picked() == "imported String value"
    @test LocalRootBeforeInit11203.probe() == (UndefVarError, "local")
    @test RenamedLiveAlias11203.before_mutation == 1
    @test RenamedLiveAlias11203.after_mutation == 2
    @test RenamedLiveAlias11203.reflected_module_alias

    @test TemporalAlias11216.before_conflict == "A"
    @test TemporalAlias11216.after_conflict_error === UndefVarError
    @test TemporalAlias11216.argument_order_error === UndefVarError
    @test TemporalAlias11216.argument_side_effect_count == 0
    @test TemporalAlias11216.after_explicit == "C"
    @test TemporalAlias11216.local_shadow_result == "local"

    @test before_value_using_error_11221 === UndefVarError
    @test after_value_using_11221 == 42
    @test before_function_using_error_11216 === UndefVarError
    @test after_function_using_11216 == 42
    @test PlainImportOnly11228.bare_export_result === UndefVarError
    @test PlainImportOnly11228.qualified_result == 7
    @test ImplicitBaseAfterImportFix11228.abstract_irrational_type() === AbstractIrrational
    @test ImplicitBaseAfterImportFix11228.step_range_len_type() === StepRangeLen
    @test FutureParentModule11132.Consumer.before_expression === UndefVarError
    @test FutureParentModule11132.Consumer.before_function === UndefVarError
    @test FutureParentModule11132.Consumer.after_import == 9
    @test AssignedModuleAlias11229 === AssignedModuleValue11229
    @test signature_before_using_error_11221 === UndefVarError
    @test f_after_using_11221(T11221()) == 2
end

true
