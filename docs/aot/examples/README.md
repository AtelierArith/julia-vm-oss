# AoT examples

**最終更新**: 2026-01-17

このディレクトリは、AoT（Ahead-of-Time）コンパイラが得意なパターン（型が静的に決まる数値計算、ループ中心、単純な配列操作など）の例を置く場所です。

> 注意: ここにある例は “ドキュメント用のサンプル” です。fixture テストとして運用しているわけではありません。

## 例のカテゴリ

### Numeric (`numeric/`)

- `sum_squares.jl`
- `dot_product.jl`
- `statistics.jl`
- `mandelbrot_broadcast.jl`

### Loops (`loops/`)

- `loop_unrolling.jl`
- `loop_invariant.jl`
- `nested_loops.jl`

### Types (`types/`)

- `type_stable.jl`
- `type_inference.jl`
- `type_dispatch.jl`

## 実行方法

### AoT（Julia → Rust → rustc）

```bash
# 1) Rust を生成
cargo run -p subset_julia_vm --bin aot --features aot -- \
  docs/aot/examples/numeric/sum_squares.jl \
  -o /tmp/example.rs \
  --minimal-prelude

# 2) rustc でビルド（スタンドアロンで通る場合）
rustc -O /tmp/example.rs -o /tmp/example

# 3) 実行
/tmp/example
```

生成物が `Value` / `RuntimeError` / `RuntimeResult` 等を参照する場合は `subset_julia_vm_runtime` のリンクが必要です（詳細は `docs/aot/README.md` を参照）。

### Julia（公式）で動かす

サンプルは `main()` を定義しているので、Julia からは `main()` を呼び出して実行します。

```bash
julia -e 'include("docs/aot/examples/numeric/sum_squares.jl"); main()'
```

## 参考

- `docs/aot/README.md`: AoT CLI とリンク方法
- `docs/aot/DESIGN.md`: 現行実装ベースの設計メモ
