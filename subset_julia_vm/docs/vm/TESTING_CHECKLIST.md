# テストチェックリスト

新機能や修正を追加する際に確認すべき項目です。

## なぜこのチェックリストが必要か

SubsetJuliaVM は複数の環境で動作します：
- **sjulia REPL** (Native Rust)
- **iOS アプリ** (iOS バイナリ)
- **Web Playground** (WASM)

これらの環境は同じ Rust コードを使用しますが、**ビルドタイミング**が異なります：
- sjulia: `cargo run` で常に最新ソースからビルド
- iOS/Web: 事前にビルドされたバイナリ/WASM を使用

そのため、sjulia でテストが通っても、iOS/Web では古いビルドが使われて動作しないことがあります。

## 新機能追加時のチェックリスト

### 1. 基本テスト
- [ ] fixture テストを追加 (`subset_julia_vm/tests/fixtures/<category>/`)
- [ ] `cargo test` が通過
- [ ] **Julia 本家で検証**: すべての fixture テストを `julia path/to/test.jl` で実行し、同一の結果を確認 (Issue #2626)

### 2. カテゴリ別テスト実行（開発中）
```bash
# 変更に関連するカテゴリのみ実行（推奨）
timeout 300 cargo test --test fixture_tests <category>::

# 特定テスト1件だけ
timeout 300 cargo test --test fixture_tests <test_name>

# ユニットテストのみ
timeout 300 cargo test --lib

# カテゴリ一覧の確認
cargo test --test fixture_tests -- --list 2>/dev/null | sed 's/::.*/::/;s/ .*//' | sort -u
```

### 3. モジュール/グローバル参照のテスト
新しいモジュールやグローバル定数を追加する場合：
- [ ] 単独での参照をテスト (`typeof(NewModule)`)
- [ ] 関数呼び出しをテスト (`NewModule.func()`)
- [ ] `lowering/expr/mod.rs` でモジュールリテラルとして登録されているか確認

### 4. WASM ビルドの確認
```bash
cd subset_julia_vm_web
wasm-pack build --target web --profile web-release
```

### 5. ドキュメント更新
- [ ] `docs/vm/STATUS.md` に実装状況を追加
- [ ] `docs/vm/UNIMPLEMENTED.md` から削除（該当する場合）

### 6. PR 提出前の最終確認
```bash
# フルテスト（PR 前のみ実行）
timeout 300 cargo test
```

## テストフィクスチャ配置ルール

**重要:** すべてのフィクスチャテストは `subset_julia_vm/tests/fixtures/<category>/` に配置すること。

1. `.jl` ファイルを `subset_julia_vm/tests/fixtures/<category>/` に配置
2. `manifest.toml` にテストエントリを追加
3. Julia 本家で実行して結果を検証
4. `cargo test --test fixture_tests <test_name>` で SubsetJuliaVM でも確認

### テストの書き方

```julia
using Test

# 型・関数・モジュール定義は @testset の外に
struct MyType
    x::Int64
end

@testset "Description" begin
    @test some_expression == expected_value
end

true  # テスト通過
```

**スコープルール:** 型定義、関数/マクロ定義、モジュール定義、`using`/`import` は `@testset` ブロックの外に配置。

## 過去の問題事例

### Meta モジュール未定義 (2026-01-11)

**症状**: `typeof(Meta)` が Web/iOS で `UndefVarError: Meta not defined` エラー

**原因**: `Meta` が `lowering/expr/mod.rs` でモジュールリテラルとして登録されていなかった

**教訓**:
- `Module.func()` 形式の呼び出しは `Expr::ModuleCall` として処理されるので動作する
- `typeof(Module)` のような単独参照は `Expr::Var` として処理され、モジュールリテラルとして登録されていないと失敗する
- 新しいモジュールを追加する際は、必ず両方のパスをテストする

### Phase 4-4 Set 移行後のテスト不整合 (2026-02-11)

**症状**: Pure Julia 移行後にテストの exemption リストとアサーションが stale になった

**教訓**:
- Rust ビルトインから Pure Julia に移行した際は、関連するすべてのテストを確認
- exemption リストや「未実装」アサーションは移行後に更新が必要
- `is_base_function()` の変更時は `test_builtin_id_registration_completeness` を実行

## 現在のテストフィクスチャカテゴリ一覧 (93+)

abstract, arithmetic, array, array_utils, arrays, bigfloat, bigint, bool, broadcast, closures, collections, combinatorics, comparison, complex, comprehension, concurrency, constants, control_flow, conversion, dates, dict, dispatch, do_block, error, exceptions, exports, filesystem, floatfuncs, floatprops, function, functions, generated, generator, getindex, global_arrays, hof, intfuncs, io, iocontext, iteration, iterators, julia_manual, kwargs, kwargs_splat, kwdef, let_blocks, linalg, literals, logging, macro, macros, math, mathconstants, meta, missing, mixed, module, modules, multimedia, number, numeric, operators, path, promotion, pure_julia, range, rational, reduce, reflection, regex, scope, sets, sort, splat, statistics, stdlib, strings, struct, subarray, ternary, timing, tuple, type_inference, type_stability, types, varargs, where
