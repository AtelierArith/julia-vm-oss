# `calc_pi` / ユークリッド GCD ベンチマーク

与えられた Julia コードを **公式 Julia**、**juliars（AoT）**、**sjulia**、**Python 3.14** の 4 実装で実行し、速度を比較した知見です。

```julia
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end
```

## 計測環境

- OS: macOS 26.5.1 (arm64 / Apple Silicon)
- sjulia: `target/release/sjulia`（SubsetJuliaVM、VM インタプリタ）
- juliars: `target/release/juliars`（SubsetJuliaVM、AoT → Rust → ネイティブバイナリ）
- 公式 Julia: `julia version 1.12.6`
- Python: `uv run --python 3.14 --no-project` で取得した **Python 3.14.5**
- 計測方法: `bash` の `time` による wall time（`real`）
  - 各プロセスの起動時間を含みます
  - Julia / sjulia / juliars についてはコード内 `@time` 経過時間も併記しています

## 実行コマンド

```bash
# 公式 Julia
time julia --startup-file=no bench.jl

# sjulia（VM インタプリタ）
time target/release/sjulia bench.jl

# juliars（AoT: Julia → Rust → ネイティブバイナリ）
target/release/juliars bench.jl -o generated.rs
# Cargo.toml に subset_julia_vm_runtime を依存に追加してビルド
cargo build --release
time ./target/release/bench_pi_aot

# Python 3.14
time uv run --python 3.14 --no-project python bench.py N
```

## 結果

### wall time（プロセス全体）

| N | 総ループ回数 | Julia 1.12 (real) | juliars AoT (real) | sjulia (real) | Python 3.14 (real) |
|---|-------------|-------------------|--------------------|---------------|--------------------|
| 1000 | 1.0×10⁶ | 0.130 s | — | 0.178 s | 0.351 s |
| 2000 | 4.0×10⁶ | 0.164 s | — | 0.524 s | 0.702 s |
| 5000 | 2.5×10⁷ | 0.551 s | — | 3.122 s | 3.920 s |
| 10000 | 1.0×10⁸ | 2.045 s | — | 12.568 s | 16.410 s |

※ 2026-06-28 再計測（sjulia に型注釈最適化パッチ #8147 適用済み）

※ juliars AoT の wall time は「Julia → Rust 変換 + Cargo ビルド」の **一度きりのオフライン工程** を含まない。ネイティブバイナリ単体の wall time は後述の内部計測に近い値。

### コード内 `@time` 経過時間（起動を除く）

2026-06-27 再計測（Apple Silicon, Julia 1.12.6, sjulia current main, juliars current main）：

| N | Julia 1.12 | juliars AoT | sjulia | sjulia / Julia | sjulia / AoT |
|---|------------|-------------|--------|----------------|--------------|
| 1000 | 0.029 s *(49% JIT)* | 0.035 s | 0.118 s | 4.1× | 3.4× |
| 2000 | 0.065 s | 0.081 s | 0.478 s | 7.4× | 5.9× |
| 5000 | 0.455 s | **0.450 s** | 3.115 s | 6.8× | 6.9× |
| 10000 | 1.997 s | **1.922 s** | 12.946 s | 6.5× | 6.7× |

プロセス wall time（N=1000〜10000 を一括実行）:
- Julia 1.12: 2.71 s / juliars AoT binary: 2.85 s / sjulia: 16.92 s

旧計測値（参考・Python 3.14 内部計測）:

| N | Julia 1.12 内部計測（旧） | sjulia 推定（旧） | Python 3.14 内部計測 |
|---|--------------------------|-------------------|----------------------|
| 1000 | 0.0186 s | ~0.10 s | 0.1287 s |
| 2000 | 0.0703 s | ~0.45 s | 0.5520 s |
| 5000 | 0.4609 s | ~3.05 s | 3.7568 s |
| 10000 | 2.0049 s | ~12.51 s | 16.1268 s |

## 主な知見

1. **juliars (AoT) は N≥5000 で公式 Julia と同等**
   - N=5000: Julia 0.455 s vs AoT 0.450 s（誤差範囲）。
   - N=10000: Julia 1.997 s vs AoT 1.922 s（AoT がわずかに速い）。
   - Julia → Rust に変換してネイティブコンパイルすると LLVM 最適化が同様に効く。
   - N=1000 では Julia の JIT (49% がコンパイル) を引いた実行時間 (~15 ms) より AoT (35 ms) が遅い — Julia の JIT がインライン展開を積極的に行うため。

2. **公式 Julia は JIT 後に非常に速い**
   - N=10000 で ~2.0 s。LLVM バックエンドによるネイティブコード生成の恩恵が大きい。

3. **sjulia は公式 Julia / juliars AoT の約 6.5〜7 倍遅い（N=5000〜10000）**
   - sjulia は iOS 向け no-JIT VM インタプリタであるため、比較の土俵が異なる。
   - ホットパスが単純な整数 `%` 演算の繰り返しであり、VM インタプリタとしては十分実用的な速度を示している。

4. **sjulia は Python 3.14 を上回る**
   - N=10000 では sjulia 12.6 s に対し Python 3.14 は 16.5 s と、**sjulia が約 1.3 倍速い**。

5. **juliars AoT のコードジェネレータに既知のバグあり**
   - `@time` の timing 計算で生成される `... as i64.wrapping_sub(t0)` が Rust の演算子優先順位により `as (i64.wrapping_sub(t0))` と解釈されコンパイルエラーになる。
   - 正しくは `(... as i64).wrapping_sub(t0)` と括弧が必要。

## 考察

- このベンチマークは **整数演算とループ制御の実装効率** を測っている。
- juliars (AoT) は「Julia を事前に Rust コードに変換してネイティブビルド」という経路で、JIT なしに Julia 相当の速度を実現できることを示した。
- sjulia の結果は、no-JIT VM として見た場合に**競争力がある**（Python より速い）。
- sjulia でもホットループの型推論や VM 命令の最適化が今後の改善ポイントとなる。

## 型注釈の影響（sjulia だけ）

同じコードに型注釈を加えた場合の sjulia 実行時間を比較しました。

| N | untyped | `::Int` 引数のみ | `::Int` 引数 + 戻り値 `::Int/::Float64` | ローカル変数にも `::Int/::Float64` |
|---|---------|------------------|------------------------------------------|------------------------------------|
| 1000 | 0.178 s | **0.123 s** | 0.123 s | 0.124 s |
| 2000 | 0.524 s | **0.371 s** | 0.368 s | 0.370 s |
| 5000 | 3.122 s | **2.227 s** | 2.171 s | 2.129 s |
| 10000 | 12.568 s | **8.613 s** | 8.542 s | 8.696 s |

※ 2026-06-28 再計測。sjulia main に型注釈最適化パッチ（`type_annotation_noop_convert_8147`）適用済み。

### 知見

- **引数に `::Int` を付けると速くなる**（N=10000 で約 30% 短縮）。
  - 型が決まることで VM が I64 専用の命令（`LoadSlotI64`、`AddConstI64Slot` など）を選べるようになる。
- **パッチ適用後、戻り値・ローカル変数の型注釈も引数型と同等の速度になった**。
  - 以前は `CallBuiltin(Convert, 2)` がホットパスに挿入され、最大で数十倍遅くなっていた。
  - 最適化により、既に正しい型を持つ値に対する `Convert` が no-op または専用命令に置き換えられるようになった。

### 実用上のまとめ

現状の sjulia では、**引数・戻り値・ローカル変数に `::Int` / `::Float64` を付けても、パフォーマンスを損なわない**（むしろ引数型では速くなる）。ただし、他の型や複雑な型注釈では同様の最適化が効くかはケースバイケースなので、計測しながら使うのが無難です。

関連 Issue: [#8147](https://github.com/AtelierArith/ailujsoi/issues/8147)

## 呼び出し型特殊化の直接ディスパッチ化（Issue #8167）

untyped 関数 `mygcd(a, b)` を I64 引数で呼ぶと、呼び出し側は
`CallSpecializeI64Slots` を使う。改善前はこの命令が **毎回**
`SpecializationKey { func_index, arg_types: vec![I64; n] }` を作って `Vec` キーの
HashMap を引き、さらに callee の `param_slots` を clone していた（gcd 内側ループ
では 1 呼び出しあたり 2 回のヒープ確保 + Vec ハッシュ）。

`(spec_func_index, arity)` をキーにした軽量キャッシュ `specialization_i64_cache`
を追加し、初回特殊化後は解決済みエントリへ直接ジャンプするようにした
（#8159 案1 の「2 回目以降は `CallResolvedI64Slots` 相当の直接呼び出しで飛ぶ」）。

### 計測（untyped `calc_pi`、コード内 `@time` の min / 5 回、Apple Silicon）

| N | 改善前 untyped | `::Int` 引数版（目標） | **改善後 untyped (#8167)** | 短縮率 |
|---|---------------:|----------------------:|---------------------------:|-------:|
| 1000 | 0.091 s | 0.066 s | **0.065 s** | 約 −29% |
| 2000 | 0.372 s | 0.270 s | **0.265 s** | 約 −29% |
| 3000 | 0.844 s | 0.620 s | **0.595 s** | 約 −30% |

- untyped 定義が typed-args 版とほぼ同速（むしろ僅かに速い）になった。
- どの N でも結果は upstream Julia と一致。
- 「`mygcd` のみ typed / `calc_pi` のみ typed」を切り分けた計測から、ギャップは
  **callee（`mygcd`）本体側の呼び出しディスパッチ**にあり、`calc_pi` 本体の
  型付けとは独立であることを確認した上での修正。

関連 Issue: [#8167](https://github.com/AtelierArith/ailujsoi/issues/8167), [#8159](https://github.com/AtelierArith/ailujsoi/issues/8159)

## 動的二項演算ディスパッチのキャッシュ（Issue #8168）

`calc_pi` のホットパスは #8167 以降、`%`/`!=`/`==` がいずれも特殊化本体内の
専用 I64 命令か `fast_primitive` 短絡で処理されるため、**`calc_pi` は #8168 では
変化しません（無回帰）**。#8168 が効くのは「両オペランドが `Any` で実行時に
メソッド解決する二項演算」(`CallDynamicBinaryBoth`)、典型的には構造体・自作数値型
を多態的に演算するコードです。

検証用に、`Vector{Any}` に入れた構造体 `V2` を `+` で畳むホットループ
（`acc = acc + xs[k]`、`+` は 24 候補から毎回解決）で計測しました。

| n | #8167（キャッシュ無） | **#8168（キャッシュ有）** | 短縮率 |
|---|---------------------:|--------------------------:|-------:|
| 1,000,000 | 9.31 s | **6.95 s** | 約 −25% |
| 2,000,000 | 18.64 s | **13.91 s** | 約 −25% |

- 構造体×構造体ペアに限定してキャッシュ（`call_site_ip × 型名ハッシュ →
  解決済み func_index`）。解決結果が型名で一意に決まる場合のみ載せるため健全。
- `calc_pi` untyped は #8167 と同値（無回帰）、結果は upstream Julia と一致。

関連 Issue: [#8168](https://github.com/AtelierArith/ailujsoi/issues/8168), [#8159](https://github.com/AtelierArith/ailujsoi/issues/8159)

## 生データ（参考）

### 2026-06-27 再計測（Julia 1.12, juliars AoT, sjulia を一括実行）

#### Julia 1.12

```
N=1000: π ≈ 3.140415340380906
  0.029333 seconds (21.37 k allocations: 1.067 MiB, 49.03% compilation time)
N=2000: π ≈ 3.1406457157456087
  0.065035 seconds (7 allocations: 528 bytes)
N=5000: π ≈ 3.1413097643199746
  0.454583 seconds (7 allocations: 528 bytes)
N=10000: π ≈ 3.141534239016629
  1.997298 seconds (7 allocations: 528 bytes)
wall: 2.71s user 0.05s system  real 2.710s
```

#### juliars AoT（Julia → Rust → ネイティブバイナリ）

```
N=1000: π ≈ 3.140415340380906
  0.034547 seconds
N=2000: π ≈ 3.1406457157456087
  0.080531 seconds
N=5000: π ≈ 3.1413097643199746
  0.450469 seconds
N=10000: π ≈ 3.141534239016629
  1.921683 seconds
wall: 2.47s user 0.01s system  real 2.847s
```

#### sjulia

```
N=1000: π ≈ 3.140415340380906
  0.117963 seconds
N=2000: π ≈ 3.1406457157456087
  0.477816 seconds
N=5000: π ≈ 3.1413097643199746
  3.114917 seconds
N=10000: π ≈ 3.141534239016629
  12.946256 seconds
wall: 16.77s user 0.09s system  real 16.923s
```

### 旧計測データ（Julia 1.12 N=10000）

```
real    0m2.305s
user    0m2.867s
sys     0m0.087s
elapsed: 2.004953167 s
```

### 旧計測データ（Python 3.14 N=10000）

```
3.141534239016629
elapsed: 16.126798 s

real    0m16.459s
user    0m16.351s
sys     0m0.067s
```

---

計測日: 2026-06-27（juliars AoT 追加）／2026-06-28（sjulia 型注釈最適化パッチ #8147 適用後の再計測）
