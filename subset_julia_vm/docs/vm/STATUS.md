# SubsetJuliaVM Status

## 最近の実装

### 2026-05-29

#### PartialStruct のフィールド事実を分岐コンストラクタ間で join (Issue #4847) ✅ 完了

`getfield(flag ? Ctor(1, "x") : Ctor(2, "y"), :b)` のように、ternary / if-else の
両枝が同一の immutable struct を構築する場合に、各枝の `ConstructorPartial` を
フィールドごとに lattice join するよう型推論を拡張。これにより内側の分岐
コンストラクタが生成するフィールド型事実が外側の `getfield` まで伝播し、
返り値推論が `Any` に widen せず upstream Julia と一致した `String` 等を返す。
`infer_partial_struct_expr` / `infer_partial_struct_field` に `Expr::Ternary`
分岐を追加し、`join_constructor_partials` ヘルパで shape (struct 名・フィールド
集合) が一致する場合のみ各フィールドを `join_limited` で結合
(`compile/abstract_interp/engine/mod.rs`)。回帰テスト:
`tests/fixtures/type_inference/partial_struct_branch_join_4847.jl`。

### 2026-02-11: Phase 4-4 Set代数テスト修正 + Pythagorean Identity テスト追加 ✅ 完了

Phase 4-4 (Pure Julia Set 移行) 後に stale になったテストを修正し、三角関数のプロパティテストを追加。

- `test_builtin_id_registration_completeness`: stale な exemption リストを修正
- `test_set_functions`: Set 代数演算が Pure Julia に移行済みであることを反映
- `math_pythagorean_identity`: `sin(θ)² + cos(θ)² ≈ 1` を `θ ∈ [-π, π]` で検証

---

### 2026-02 (継続中): Pure Julia 移行 & 大規模リファクタリング

2026-01-16 以降、2,400+ コミットにわたる大規模な開発が進行。主要な変更を以下にまとめる。

#### Phase 6-1, 6-2: 三角関数/指数/対数の Pure Julia 化 ✅ 完了

**PR #2734**

sin, cos, tan, asin, acos, atan, exp, log の Rust ビルトインを削除し、Pure Julia 実装に移行。

- `subset_julia_vm/src/julia/base/math.jl` に80+関数を Pure Julia 実装
- `subset_julia_vm/src/julia/base/special/trig.jl` - 三角関数特化実装
- `subset_julia_vm/src/julia/base/special/exp.jl` - 指数関数特化実装
- `subset_julia_vm/src/julia/base/special/log.jl` - 対数関数特化実装
- Rust intrinsics は CPU レベル操作（sqrt_llvm, floor_llvm 等）のみに限定

**実装済み数学関数一覧:**
- 基本三角関数: sin, cos, tan, asin, acos, atan
- π乗算系: sinpi, cospi, sincospi, tanpi
- 度数系: sind, cosd, tand, asind, acosd, atand, sincosd
- 逆数系: sec, csc, cot, asec, acsc, acot
- 度数逆数系: secd, cscd, cotd, asecd, acscd, acotd
- 双曲線: sinh, cosh, tanh, asinh, acosh, atanh
- 逆数双曲線: sech, csch, coth, asech, acsch, acoth
- 指数/対数: exp, exp2, exp10, expm1, log, log2, log10, log1p, log(b,x)
- 根: sqrt (intrinsic), cbrt, fourthroot
- 複合: sincos, sinc, cosc, hypot, fma, muladd
- 除算: div, rem, mod, fld, divrem, fldmod, mod1, fld1, fldmod1, mod2pi, rem2pi
- 多項式: evalpoly
- 符号/クランプ: sign, clamp, copysign, minmax
- 角度変換: deg2rad, rad2deg
- 偶奇: iseven, isodd

#### Phase 4-4: Set 代数演算の Pure Julia 化 ✅ 完了

**PR #2733, Issue #2575**

Set 代数演算を Rust ビルトインから Pure Julia に移行。

- `subset_julia_vm/src/julia/base/set.jl` に実装
- 非破壊: union, intersect, setdiff, symdiff, issubset, isdisjoint, issetequal
- 破壊的: union!, intersect!, setdiff!, symdiff!
- ユーティリティ: push!, delete!, empty!, length, copy, empty, in!
- 配列ユーティリティ: unique, unique(f, arr), allunique, allequal, unique!
- Rust intrinsics は `_set_push!`, `_set_delete!`, `_set_in`, `_set_empty!`, `_set_length` のみ

#### Broadcast Pure Julia 化 (Phase 1-7) ✅ 完了

BroadcastStyle 型階層、Broadcasted ラッパー、materialize/copy/copyto!、@. マクロ、.&&/.|| 演算子をすべて Pure Julia で実装。

- Phase 1-2: BroadcastStyle 型定義と shape 計算
- Phase 3-4: broadcast インデキシングとマテリアライゼーション
- Phase 5: Flatten/isflat ループフュージョンインフラ
- Phase 6: @__dot__/@. マクロ
- Phase 7: .&& / .|| ブロードキャスト演算子
- 43個のテストフィクスチャで検証済み

#### Dict Pure Julia 化 ✅ 完了

**Issues #2572, #2573, #2669**

Dict の読み取り操作を Pure Julia に移行。

- `subset_julia_vm/src/julia/base/dict.jl` に実装
- Pure Julia: haskey, get, getkey, keys, values, pairs, merge, copy, mergewith, mergewith!
- Rust intrinsics: `_dict_get`, `_dict_set!`, `_dict_delete!`, `_dict_haskey`, `_dict_length`, `_dict_empty!`, `_dict_keys`, `_dict_values`, `_dict_pairs`
- 破壊的: get!, empty!, delete!, merge!, pop!

#### 型システム強化

- Vararg{T} / Vararg{T,N} 型アノテーションのディスパッチ対応
- 共変境界 (<:T) の Type ディスパッチ
- Diagonal Rule (Issue #2554) による型パラメータディスパッチ
- Const 型エイリアスディスパッチ (Issue #2527)
- TypeVar upper bounds for where T context

#### Callable Struct 構文 ✅ 完了

**Issue #2671**

`(::Type)(args) = body` 形式の callable struct 構文をサポート。

#### AoT パイプライン復旧

- 24+ コンパイルエラーを解消
- Enum サポート (Stmt::EnumDef, JuliaType::Enum)
- Cranelift JIT バックエンド (関数呼び出し、phi ノード、switch、libm)
- 35+ エンドツーエンドテスト稼働
- GitHub Actions CI テスト追加

#### ビット演算 Pure Julia 化

**Issue #2618**

ビットシフト (<<, >>, >>>) とビット演算 (&, |, ⊻, ~) のラッパー関数を追加。

---

### 2026-01-16: LinRange と StepRangeLen のイテレーション対応 ✅ 完了

`range(start, stop; length=n)` で作成された LinRange と StepRangeLen の for ループでの反復処理をサポート。

#### 問題
Issue #944: mandelbrot_grid サンプルで LinRange 型の反復処理が失敗

```julia
xs = range(-2.0, 1.0; length=10)
for x in xs  # ← Runtime error: iterate: unsupported struct type for StructRef(12)
    println(x)
end
```

#### 原因
`src/vm/type_ops.rs` の `iterate_first` と `iterate_next` メソッドが LinRange および StepRangeLen 構造体に対応していなかった。コンパイラが `CallBuiltin(Iterate)` を発行した際、VMがこれらの型を処理できずエラーになっていた。

#### 実装内容

**LinRange イテレーション**
- Julia の lerp（線形補間）公式を実装: `element = (1 - t) * start + t * stop` where `t = (i - 1) / lendiv`
- 構造体フィールド: `start`, `stop`, `len`, `lendiv`
- ジェネリック型名対応: `LinRange{Float64}` などに `starts_with` パターンマッチングで対応

**StepRangeLen イテレーション**
- 参照値とステップによる範囲計算を実装: `element = ref + (i - offset) * step`
- 構造体フィールド: `ref`, `step`, `len`, `offset`
- ジェネリック型名対応: `StepRangeLen{Float64}` などに対応

#### テスト

- `tests/fixtures/iterators/linrange_iteration.jl` - LinRange 反復処理のテスト

#### 修正されたファイル

| ファイル | 変更内容 |
|----------|----------|
| `src/vm/type_ops.rs` | `iterate_first` と `iterate_next` に LinRange と StepRangeLen の処理を追加 |
| `tests/fixtures/iterators/linrange_iteration.jl` | LinRange 反復テストを追加 |
| `tests/fixtures/iterators/manifest.toml` | テストエントリを追加 |

---

### 2026-01-11: kwargs... を Julia 本家準拠の Base.Pairs 型に変更 ✅ 完了

キーワード引数スラープ `function f(; kwargs...)` を Julia 本家と同じ `Base.Pairs` 型で実装。

#### 実装内容

**kwargs を Base.Pairs として実装**
- `kwargs` は `Base.Pairs` 型として Julia 本家と同じ挙動を実現
- `kwargs[:a]` のシンボルインデックスでアクセス（Julia 本家と同じ）
- `length(kwargs)` で受け取ったキーワード引数の数を取得可能

#### Julia 本家との互換性 ✅ 完全対応

| 機能 | Julia kwargs | SubsetJuliaVM kwargs | 状態 |
|------|--------------|----------------------|------|
| 型 | `Base.Pairs` | `Base.Pairs` | ✅ **完全対応** |
| `kwargs[:a]` | ✅ | ✅ | ✅ **完全対応** |
| `length(kwargs)` | ✅ | ✅ | ✅ **完全対応** |
| `kwargs.a` | ❌ ERROR | ❌ ERROR | ✅ **完全対応** |

---

### 2026-01-10: キーワード引数とテストマクロの修正

11個のテストが失敗していた問題を修正。3つの異なる原因を特定し解決。

---

## 技術的知見

### キーワード引数のパーシング

Pure Rust パーサーでは、キーワード引数 `a=nothing` は以下のようにパースされる：
```
KwParameter
  ├── Identifier: "a"
  └── Identifier: "nothing"
```

`nothing` は `NodeKind::Identifier` として解析されるため、明示的に2番目の `Identifier` をデフォルト値として処理する必要がある。

### VMの出力バッファ

`println!()` はRustの標準出力に直接書き込むため、テストで `vm.get_output()` を使用してキャプチャする場合は動作しない。VM内部から出力を行う場合は必ず `self.emit_output()` を使用すること。

### マクロの `using` 検証

stdlib マクロ（`@test`, `@testset` など）は、適切なモジュールがインポートされているかを `lambda_ctx.has_using()` で検証する必要がある。検証が欠如すると「unknown macro」というエラーになり、ユーザーには `using ModuleName` が必要であることが伝わらない。

### Pure Julia 移行の三層アーキテクチャ

SubsetJuliaVM の関数実装は三層構成：

1. **Intrinsics** (CPU レベル): sqrt_llvm, floor_llvm, ceil_llvm, trunc_llvm, abs_float, copysign_float
2. **Rust Builtins** (パフォーマンス重視): round, 配列操作、I/O 等
3. **Pure Julia** (ライブラリ層): sin, cos, tan, exp, log, Set/Dict 操作、数学関数群

設計方針: Julia で書けるものは Julia で書く。Rust intrinsics は CPU 命令やハッシュマップ等のデータ構造操作に限定。

---

## 既知の問題

### マクロパラメータが quote 外で評価される問題

**発見日:** 2026-01-10
**影響:** マクロ定義内で `string(ex)` などのパラメータ操作が quote ブロック外でできない
**深刻度:** 中（ワークアラウンド可能）

#### 問題の説明

Julia の正式な動作では、マクロパラメータは Expr オブジェクト（AST）として渡され、マクロ本体内でデータとして扱われる。しかし、SubsetJuliaVM では quote ブロック外でマクロパラメータにアクセスすると、そのパラメータが**コードとして評価**されてしまう。

#### 再現コード

```julia
# このマクロは Julia では動作するが、SubsetJuliaVM では失敗する
macro test_throws(T, ex)
    expr_str = string(ex)  # ← ここで ex が評価されてしまう
    quote
        # ...
    end
end

@test_throws ErrorException error("test")
# SubsetJuliaVM: Runtime error: ErrorException: test
# Julia: 正常に動作（ex は Expr(:call, :error, "test") として扱われる）
```

#### 現在のワークアラウンド

1. **quote 外でパラメータを使わない**: すべてのパラメータ操作を quote 内に移動
2. **式の文字列化を諦める**: エラーメッセージから式の内容を省略

#### 修正の方向性

`lowering/expr/mod.rs` の `substitute_params_in_macro_expr` で、マクロパラメータを実行可能 IR ではなく `Expr::Builtin { name: ExprNew }` として保持し、VM 実行時に `Value::Expr(...)` として参照できるようにする。
