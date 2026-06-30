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

true
