# テスト戦略と開発ワークフロー

**最終更新**: 2026-06-10

## sjulia による Base 関数テスト

`subset_julia_vm/src/julia/base/*.jl` に定義されている Pure Julia 関数は、`sjulia` コマンドを使ってテストできます。

### テストファイル構成

```
subset_julia_vm/tests/base/
├── run_all.sh           # 一括実行スクリプト
├── test_operators.jl    # min, max, minmax, copysign, etc.
├── test_number.jl       # iszero, isone, identity, etc.
├── test_bool.jl         # xor, nand, nor
├── test_math.jl         # sign, clamp, hypot, trig functions, etc.
├── test_intfuncs.jl     # gcd, lcm, factorial, isqrt, etc.
├── test_array.jl        # prod, minimum, maximum, mean, etc.
├── test_statistics.jl   # var, std, median, cor, cov
├── test_sort.jl         # sort, sortperm, searchsortedfirst, etc.
└── test_set.jl          # unique, union, intersect, setdiff, etc.
```

### テスト実行方法

```bash
# ビルド
cd subset_julia_vm
cargo build --release

# 個別テスト実行
./target/release/sjulia tests/base/test_math.jl

# 全テスト一括実行
./tests/base/run_all.sh
```

### テストの書き方

```julia
# Julia の @test マクロを使用（VM レベルで実装済み）
using Test

@testset "function_name" begin
    @test function_name(arg1, arg2) == expected
    @test isapprox(function_name(x), expected) == true
end
```

テストケースは Julia 公式 (`julia/test/`) を参考に作成しています。

---

## 重要な原則：iOS 機能追加 = Rust テスト追加

**iOS のサンプルコードや新機能を追加する場合、必ず Rust 側のテストも追加してください。**

この方針により以下が実現されます：
1. VM の正確性を独立して検証
2. iOS アプリは検証済みの Rust に依存
3. 回帰を早期に検出
4. 機能がテストで文書化される

## テスト組織

**Rust テスト** (`subset_julia_vm/tests/` と `subset_julia_vm/src/`)：
- `fixture_tests.rs`: Fixture テスト（2,377件・104カテゴリ、Julia コード実行の検証）
- `code_samples_tests.rs`: コードサンプルの検証
- `ios_samples_tests.rs`: iOS サンプルコードの検証
- `integration_array_tests.rs`: 配列統合テスト
- `integration_dict_broadcast_tests.rs`: Dict/broadcast 統合テスト
- `integration_string_type_tests.rs`: 文字列/型 統合テスト
- `integration_struct_hof_tests.rs`: 構造体/HOF 統合テスト
- `integration_module_base_tests.rs`: モジュール/Base 統合テスト
- `integration_compile_sample_tests.rs`: コンパイル/サンプル 統合テスト
- `panic_free_vm_tests.rs`: パニックフリー VM 回帰テスト
- `core_ir_aot_tests.rs`: Core IR `.sjir` シリアライズ / AoT 変換テスト
- `parser_pure_rust.rs`: パーサーテスト
- `dispatch_tests.rs`: 多重ディスパッチテスト
- `aot_e2e_tests.rs`: AoT コンパイルの E2E テスト
- `src/compile/`: コンパイラの単体テスト（各モジュール内 `#[cfg(test)]`）
- `src/vm/`: VM 実行の単体テスト（各モジュール内 `#[cfg(test)]`）

**Fixture テスト** (`tests/fixtures/` と `subset_julia_vm/tests/fixtures/`)：
- 104 カテゴリ、2,377 件のテスト
- Julia コードを実行し、期待値と比較
- `manifest.toml` でテスト定義を管理

## 新機能追加時のワークフロー

**Step 1: Rust VM に機能を実装**
```bash
# 例：行列演算
vim subset_julia_vm/src/vm/mod.rs
# MatMul 命令を追加
```

**Step 2: 即座に Rust テストを追加**
```bash
# subset_julia_vm/tests/code_samples_tests.rs に追加
#[test]
fn test_matrix_multiplication() {
    let code = r#"
        A = [1 2; 3 4]
        v = [1; 2]
        A * v
    "#;
    let result = run_code(code);
    assert_eq!(result, expected);
}
```

**Step 3: テストが全て通ることを確認**
```bash
cd subset_julia_vm
timeout 1800 cargo nextest run --release
timeout 1800 cargo nextest run --release --test code_samples_tests
```

**Step 4: テスト合格後、iOS サンプルコードを追加**
```bash
# その後、iOS サンプルを追加
vim SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSample.swift
```

**Step 5: コミット前に全テストを実行**
```bash
cd subset_julia_vm
timeout 1800 cargo nextest run --release
```

## テスト実行コマンド

**重要**: テストは `timeout` でラップしてハングを防止してください（最大30分）。`cargo nextest run --release` を使用してください。

```bash
# すべてのテスト（timeout 必須）
timeout 1800 cargo nextest run --release

# Fixture テストのみ
timeout 1800 cargo nextest run --release --test fixture_tests

# カテゴリ指定（開発中）
timeout 1800 cargo nextest run --release --test fixture_tests <category>::

# カテゴリ一覧
cargo nextest list --test fixture_tests 2>/dev/null | sed 's/::.*/::/;s/ .*//' | sort -u

# 単体テスト
timeout 1800 cargo nextest run --release --lib

# iOS サンプルコードの検証
timeout 1800 cargo nextest run --release --test ios_samples_tests

# パーサーテスト（別パッケージ）
timeout 1800 cargo nextest run --release --manifest-path subset_julia_vm_parser/Cargo.toml
```

## テストツール

### スナップショットテスト (insta)

パーサ出力や IR のスナップショット比較に使用。

```bash
# スナップショットテスト実行
cargo insta test

# スナップショットのレビュー
cargo insta review

# 新しいスナップショットを受け入れ
cargo insta accept
```

**インストール**:
```bash
cargo install cargo-insta
```

### ベンチマーク (criterion)

VM 実行速度の測定に使用。

```bash
# ベンチマーク実行
cargo bench

# 特定のベンチマークのみ
cargo bench -- fib

# HTML レポート生成 (target/criterion/ に出力)
cargo bench
```

**ベンチマークファイル**: `benches/vm_benchmark.rs`

### テストカバレッジ (tarpaulin)

テストカバレッジの測定に使用。

```bash
# インストール
cargo install cargo-tarpaulin

# カバレッジ測定 (HTML レポート)
cargo tarpaulin --out Html

# Lcov 形式で出力
cargo tarpaulin --out Lcov

# 特定のパッケージのみ
cargo tarpaulin -p subset_julia_vm --out Html
```

**注意**: tarpaulin は Linux でのみ動作します。macOS では `cargo-llvm-cov` を代替として使用できます。

```bash
# macOS 代替
cargo install cargo-llvm-cov
cargo llvm-cov --html
```

---

## テストカバレッジ目標

| コンポーネント | テスト対象 | 現状 |
|----------|---------|------|
| **パーサ** | Julia 構文のエッジケース | ✅ Pure Rust パーサー (400+ tests) |
| **コンパイラ** | IR 型推論、全演算子 | ✅ 90% カバー |
| **VM** | 全命令、エッジケース | ✅ 95% カバー |
| **統合** | iOS サンプルコード | ✅ code_samples_tests.rs + ios_samples_tests.rs |
| **Fixture** | 1,329件（99カテゴリ） | ✅ 全パス |
| **総合** | 3000+ テスト | ✅ 全パス |

## iOS と Rust のテスト同期表

| 側 | テスト内容 | タイミング | 必須 |
|---|---------|----------|------|
| **Rust** | コード正確性 | 機能実装直後 | ✅ YES |
| **iOS** | UI 動作 / FFI 呼び出し | Rust テスト合格後 | ⚠️ Optional |
| **統合** | サンプル実行 / 結果検証 | 両方のテスト合格後 | ✅ YES |
