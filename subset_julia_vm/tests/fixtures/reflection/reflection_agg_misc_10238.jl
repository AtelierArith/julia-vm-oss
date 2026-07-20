# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: reflection/core_builtin_type_distinction_5129.jl =====
module Agg_core_builtin_type_distinction_5129
# Issue #5129: Core.Builtin と generic function の型上の区別.
# 組み込み関数 (`===`, `getfield`, `typeof`, ...) は `typeof(f) <: Core.Builtin`
# となる singleton 型を持ち、ユーザー定義/一般の generic function (`+`, `sin`,
# ユーザー関数) は `Function` の subtype ではあるが `Core.Builtin` ではない。
# 本家 julia 1.12 と parity を取った値のみ assert している。

using Test

f() = 1
g(x) = x + 1

@testset "Core.Builtin vs generic function (Issue #5129)" begin
    # Core.Builtin は Function のサブタイプ (julia/base/boot.jl: Builtin <: Function)
    @test Core.Builtin <: Function
    @test Core.Builtin !== Function

    # 真の組み込み関数: isa(f, Core.Builtin) は true
    @test isa(===, Core.Builtin)
    @test isa(isa, Core.Builtin)
    @test isa(typeof, Core.Builtin)
    @test isa(<:, Core.Builtin)
    @test isa(tuple, Core.Builtin)
    @test isa(throw, Core.Builtin)
    @test isa(fieldtype, Core.Builtin)
    @test isa(applicable, Core.Builtin)

    # generic function (演算子) は Core.Builtin ではない
    @test isa(+, Core.Builtin) == false
    @test isa(*, Core.Builtin) == false
    @test isa(identity, Core.Builtin) == false
    @test isa(!, Core.Builtin) == false
    @test isa(sin, Core.Builtin) == false
    @test isa(map, Core.Builtin) == false
    @test isa(println, Core.Builtin) == false
    @test isa(string, Core.Builtin) == false

    # ユーザー定義関数は Core.Builtin ではないが Function ではある
    @test isa(f, Core.Builtin) == false
    @test isa(g, Core.Builtin) == false
    @test isa(f, Function)
    @test isa(g, Function)

    # 組み込み・一般どちらも Function のサブタイプ
    @test isa(===, Function)
    @test isa(+, Function)

    # 型レベルの区別: typeof(builtin) <: Core.Builtin, typeof(generic) は否
    @test typeof(===) <: Core.Builtin
    @test typeof(isa) <: Core.Builtin
    @test (typeof(+) <: Core.Builtin) == false
    @test (typeof(sin) <: Core.Builtin) == false
    @test (typeof(f) <: Core.Builtin) == false

    # いずれも Function のサブタイプは保たれる
    @test typeof(===) <: Function
    @test typeof(+) <: Function
    @test typeof(f) <: Function
end
end # module Agg_core_builtin_type_distinction_5129

# ===== source: reflection/function_singleton_typevar_binding_5128.jl =====
module Agg_function_singleton_typevar_binding_5128
# Issue #5128: each function has its own singleton type, typeof(f) <: Function.
# A `where {F}` / `where {F<:Function}` type variable matched against a function
# argument must bind to that function's singleton type, not Any.

using Test

# Unbounded type variable binds to the function singleton type.
ftype(x::F) where {F} = F
# Bounded `F<:Function` binds the same singleton type.
gtype(x::F) where {F<:Function} = F
# The bound singleton type is usable as a value.
hsub(x::F) where {F<:Function} = F <: Function
# Singleton identity: the bound type variable is exactly typeof(x).
idsame(x::F) where {F} = (F === typeof(x))
# Two function args get independent singleton types.
distinct(f::F, g::G) where {F, G} = (F === G)

@testset "function singleton typevar binding (Issue #5128)" begin
    @test ftype(sin) === typeof(sin)
    @test ftype(+) === typeof(+)
    @test ftype(cos) === typeof(cos)

    @test gtype(sin) === typeof(sin)

    @test hsub(sin) == true
    @test hsub(+) == true

    @test idsame(sin) == true
    @test idsame(+) == true

    @test distinct(sin, sin) == true
    @test distinct(sin, cos) == false

    # typeof(f) is a subtype of Function and is a concrete singleton type.
    @test typeof(sin) <: Function
    @test (ftype(sin) <: Function) == true
end
end # module Agg_function_singleton_typevar_binding_5128

# ===== source: reflection/isa_typeof_kind_consistency_3909.jl =====
module Agg_isa_typeof_kind_consistency_3909
using Test

struct IsaKindBox3909{T}
    x::T
end

@testset "isa agrees with typeof for runtime type object kinds (Issue #3909)" begin
    # DataType-kind values: concrete and parametric concrete types
    @test isa(Int64, DataType)
    @test isa(Vector{Int64}, DataType)
    @test isa(IsaKindBox3909{Int64}, DataType)
    @test !isa(Int64, UnionAll)
    @test !isa(Vector{Int64}, UnionAll)

    # UnionAll-kind values: parametric type schemas
    @test isa(Vector, UnionAll)
    @test isa(Dict, UnionAll)
    @test isa(IsaKindBox3909, UnionAll)
    @test !isa(Vector, DataType)
    @test !isa(Dict, DataType)
    @test !isa(IsaKindBox3909, DataType)

    # TypeVar-kind values
    @test isa(TypeVar(:T), TypeVar)
    @test !isa(TypeVar(:T), DataType)
    @test !isa(TypeVar(:T), UnionAll)

    # Both DataType and UnionAll are subtypes of Type
    @test isa(Int64, Type)
    @test isa(Vector, Type)
    @test isa(Vector{Int64}, Type)
    @test isa(Dict, Type)
    # TypeVar is not a subtype of Type in Julia
    @test !isa(TypeVar(:T), Type)
end

@testset "Base.unwrap_unionall iterates through UnionAll bodies (Issue #3909)" begin
    @test isa(Base.unwrap_unionall(Vector), DataType)
    @test isa(Base.unwrap_unionall(Dict), DataType)
    @test isa(Base.unwrap_unionall(IsaKindBox3909), DataType)

    # Non-UnionAll inputs are returned unchanged
    @test Base.unwrap_unionall(Int64) === Int64
    @test Base.unwrap_unionall(Vector{Int64}) === Vector{Int64}
end
end # module Agg_isa_typeof_kind_consistency_3909

# ===== source: reflection/parametric_runtime_typevar_4696.jl =====
module Agg_parametric_runtime_typevar_4696
using Test

struct ParamTypeVarBox4696{T}
    x::T
end

@testset "parametric application preserves runtime TypeVar (Issue #4696)" begin
    T = TypeVar(:T)

    # Vector{T} and Matrix{T} keep T as a TypeVar reference rather than
    # erasing it to Any. (Full identity (`===`) preservation between the
    # original fresh TypeVar and the projected parameter is not yet
    # modeled in sjulia — see follow-up below — so we assert kind + name.)
    @test isa(Vector{T}.parameters[1], TypeVar)
    @test Vector{T}.parameters[1].name === :T
    @test isa(Matrix{T}.parameters[1], TypeVar)
    @test Matrix{T}.parameters[1].name === :T

    # Multi-parameter cases also preserve the TypeVar name.
    @test isa(Dict{T, T}.parameters[1], TypeVar)
    @test Dict{T, T}.parameters[1].name === :T
    @test isa(Dict{T, T}.parameters[2], TypeVar)
    @test Dict{T, T}.parameters[2].name === :T

    # User parametric structs accept runtime TypeVars too.
    @test isa(ParamTypeVarBox4696{T}.parameters[1], TypeVar)
    @test ParamTypeVarBox4696{T}.parameters[1].name === :T
end

@testset "UnionAll wraps when body references runtime TypeVar (Issue #4696)" begin
    S = TypeVar(:S)
    ua = UnionAll(S, Vector{S})
    @test isa(ua, UnionAll)
    @test ua.var.name === :S
    # And the smart-wrap from #4694 still kicks in when body doesn't
    # mention the bound variable.
    @test UnionAll(S, Vector{Int64}) === Vector{Int64}
end
end # module Agg_parametric_runtime_typevar_4696

# ===== source: reflection/runtime_type_object_acceptance_3909.jl =====
module Agg_runtime_type_object_acceptance_3909
using Test

# Issue #3909: runtime type-object identity and layout-semantics acceptance
# surface. This fixture consolidates the issue's acceptance criteria into a
# single regression guard, exercising fresh `TypeVar` construction, `UnionAll`
# wrapping/unwrapping, parametric type parameters, builtin/user struct layout
# metadata, and identity-sensitive type comparisons. Every assertion is verified
# field-for-field against upstream Julia 1.12.6.
#
# The former focused follow-up for `Vector`/`Matrix` as `Array{T,N}`
# dimensional aliases is covered separately by `array_dimensional_alias_5593.jl`.

struct Box3909{T}
    value::T
end

mutable struct MBox3909
    x::Int64
end

@testset "fresh TypeVar construction (#3909)" begin
    tv = TypeVar(:T)
    @test tv isa TypeVar
    @test tv.name === :T
    @test tv.lb === Union{}
    @test tv.ub === Any
end

@testset "UnionAll wrapping / unwrapping (#3909)" begin
    @test Vector isa UnionAll
    @test Box3909 isa UnionAll
    # Unwrapping a user parametric type's UnionAll yields the bound body.
    @test Base.unwrap_unionall(Box3909) === Box3909{Base.unwrap_unionall(Box3909).parameters[1]}
    # Rewrapping the unwrapped body with the bound var roundtrips to the alias.
    body = Base.unwrap_unionall(Box3909)
    @test Base.rewrap_unionall(body, Box3909) === Box3909
end

@testset "parametric type parameters (#3909)" begin
    @test Box3909{Int}.parameters == Core.svec(Int64)
    @test Box3909{Int} === Box3909{Int}
    @test Box3909{Int} !== Box3909{Float64}
    @test Box3909{Int} <: Box3909
end

@testset "builtin / user struct layout metadata (#3909)" begin
    @test fieldnames(Box3909{Int}) === (:value,)
    @test fieldtypes(Box3909{Int}) === (Int64,)
    @test isbitstype(Box3909{Int}) === true
    @test isbitstype(MBox3909) === false
    @test ismutabletype(MBox3909) === true
    @test ismutabletype(Box3909{Int}) === false
    @test sizeof(Int64) === 8
end

@testset "identity-sensitive type comparisons (#3909)" begin
    @test typeof(Int64) === DataType
    @test Int64 === Int64
    @test Vector{Int} === Vector{Int}
    @test (Vector{Int} === Vector{Float64}) === false
    @test isa(Box3909{Int}, DataType)
end
end # module Agg_runtime_type_object_acceptance_3909

# ===== source: reflection/runtime_type_object_kind_3909.jl =====
module Agg_runtime_type_object_kind_3909
using Test

struct RuntimeKindBox3909{T}
    x::T
end

@testset "runtime type object kind typeof (Issue #3909)" begin
    @test typeof(Int64) === DataType
    @test typeof(Vector{Int64}) === DataType
    @test typeof(RuntimeKindBox3909{Int64}) === DataType

    @test typeof(Vector) === UnionAll
    @test typeof(Dict) === UnionAll
    @test typeof(RuntimeKindBox3909) === UnionAll

    @test typeof(Vector.var) === TypeVar
    @test typeof(TypeVar(:T)) === TypeVar

    @test Vector.var === Vector.body.parameters[1]
end
end # module Agg_runtime_type_object_kind_3909

# ===== source: reflection/subtypes_any_vector_helper_3908.jl =====
module Agg_subtypes_any_vector_helper_3908
using Test
using InteractiveUtils

# Regression for Issue #3908: builtins_types.rs routes the `subtypes(...)`
# result through a shared Memory-first `any_vector` helper. Exercise both the
# empty-result branch (`subtypes(::Type)` on a concrete leaf) and the
# populated-result branch (`subtypes(Signed)`) so the routed construction is
# observable end-to-end without depending on the sjulia-specific scalar
# fallback path.

function find_subtype_3908(types, name)
    for i in 1:length(types)
        if string(types[i]) == name
            return true
        end
    end
    return false
end

@testset "subtypes any_vector helper (Issue #3908)" begin
    # Empty-result branch: concrete leaf has no direct subtypes, exercising
    # the routed `any_vector(Vec::new())` construction.
    empty_result = subtypes(Int64)
    @test empty_result isa AbstractVector
    @test length(empty_result) == 0
    @test isempty(empty_result)
    @test size(empty_result) == (0,)

    # Populated-result branch: routed construction preserves shape and
    # allows the elements to roundtrip through `string`.
    signed_children = subtypes(Signed)
    @test signed_children isa AbstractVector
    @test length(signed_children) >= 1
    @test size(signed_children) == (length(signed_children),)
    @test all(t -> t isa Type, signed_children)
    @test find_subtype_3908(signed_children, "Int64")
    @test find_subtype_3908(signed_children, "Int32")
end
end # module Agg_subtypes_any_vector_helper_3908

# ===== source: reflection/subtypes_builtin_core_hierarchy.jl =====
module Agg_subtypes_builtin_core_hierarchy
using Test
using InteractiveUtils

function reflection_has_type_3837(types, name)
    for i in 1:length(types)
        if string(types[i]) == name
            return true
        end
    end
    false
end

@testset "subtypes builtin core hierarchy" begin
    signed_children = subtypes(Signed)
    @test reflection_has_type_3837(signed_children, "Int8")
    @test reflection_has_type_3837(signed_children, "Int16")
    @test reflection_has_type_3837(signed_children, "Int32")
    @test reflection_has_type_3837(signed_children, "Int64")
    @test reflection_has_type_3837(signed_children, "Int128")
    @test reflection_has_type_3837(signed_children, "BigInt")

    float_children = subtypes(AbstractFloat)
    @test reflection_has_type_3837(float_children, "Float16")
    @test reflection_has_type_3837(float_children, "Float32")
    @test reflection_has_type_3837(float_children, "Float64")
    @test reflection_has_type_3837(float_children, "BigFloat")

    type_children = subtypes(Type)
    @test reflection_has_type_3837(type_children, "DataType")
end
end # module Agg_subtypes_builtin_core_hierarchy

# ===== source: reflection/type_parameters.jl =====
module Agg_type_parameters
using Test

struct ParamBox4673{T}
    x::T
end

@testset "DataType parameters" begin
    vector_params = Vector{Int64}.parameters
    @test vector_params[1] === Int64

    matrix_params = Matrix{Bool}.parameters
    @test matrix_params[1] === Bool

    dict_params = Dict{Symbol, Int64}.parameters
    @test length(dict_params) == 2
    @test dict_params[1] === Symbol
    @test dict_params[2] === Int64

    tuple_params = Tuple{Int64, Float64}.parameters
    @test length(tuple_params) == 2
    @test tuple_params[1] === Int64
    @test tuple_params[2] === Float64

    nested_params = Dict{String, Vector{Int64}}.parameters
    @test length(nested_params) == 2
    @test nested_params[1] === String
    @test nested_params[2] === Vector{Int64}

    @test length(Int64.parameters) == 0
end

@testset "generic DataType getfield parameters (#4673)" begin
    parameter_from_getfield(::Type{T}) where T = getfield(T, :parameters)[1]
    instance_parameter_from_getfield(x::T) where T = getfield(T, :parameters)[1]

    @test parameter_from_getfield(Vector{Int64}) === Int64
    @test parameter_from_getfield(Matrix{String}) === String
    @test parameter_from_getfield(ParamBox4673{Int64}) === Int64
    @test instance_parameter_from_getfield(ParamBox4673("x")) === String
end
end # module Agg_type_parameters

true
