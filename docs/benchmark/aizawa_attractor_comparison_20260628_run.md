# Aizawa Attractor ベンチマーク比較（Julia 本家 / sjulia VM / sjulia AoT / Python 3.14）

`docs/benchmark/aizawa_ifs_comparison.md` および `docs/benchmark/aizawa_attractor_100m_scaling.md` に記録されている手法に従い、
`benchmarks/aizawa_attractor_bench.jl`（untyped）・`benchmarks/aizawa_attractor_bench_typed.jl`（typed）を
**Julia 本家**、**juliars（AoT）**、**sjulia（VM）**、**Python 3.14（uv）** の 4 実行系で計測した結果です。

## 計測環境

- OS: macOS（arm64 / Apple Silicon）
- 公式 Julia: `julia version 1.12.6`
- sjulia: `target/release/sjulia`（main 最新）
- juliars AoT: `target/release/juliars --emit-binary`（v0.9.1 相当）
- Python: `uv run --python 3.14 --no-project` で取得した CPython 3.14.5
- 計測日: 2026-06-28

## 計測方法

- ホットループ時間は各プログラム内の `time_ns()` / `time.perf_counter_ns()` で取得。
- 各実装を 3 回連続実行し、中央値を採用。
- wall time は `/usr/bin/time -p` による `real`（プロセス起動を含む）。
- AoT の wall time は「Julia → Rust ソース生成 → rustc コンパイル → ネイティブバイナリ実行」の一連の時間。
- sjulia の wall time は Base キャッシュが構築済みの状態（2 回目以降の実行）で測定しています。

## ホットループ時間（秒）

| 実装 | N=1M | N=5M | N=10M |
|---|---:|---:|---:|
| 公式 Julia 1.12 | 0.007824 | 0.040520 | 0.078049 |
| juliars AoT | 0.007056 | 0.034743 | 0.069502 |
| sjulia VM（untyped） | 0.127270 | 0.633158 | 1.261455 |
| sjulia VM（typed） | 0.124760 | 0.637858 | 1.290202 |
| Python 3.14 | 0.173145 | 0.870496 | 1.741849 |

## 相対倍率（公式 Julia 1.12 = 1、小さいほど速い）

| 実装 | N=1M | N=5M | N=10M |
|---|---:|---:|---:|
| 公式 Julia 1.12 | 1.00× | 1.00× | 1.00× |
| juliars AoT | 0.90× | 0.86× | 0.89× |
| sjulia VM（untyped） | 16.27× | 15.63× | 16.16× |
| sjulia VM（typed） | 15.95× | 15.74× | 16.53× |
| Python 3.14 | 22.13× | 21.48× | 22.32× |

## wall time（プロセス全体、`real` 秒）

N=1M/5M/10M を 1 プロセスで連続実行した合計。

| 実装 | real |
|---|--:|
| 公式 Julia 1.12 | 0.25 s |
| sjulia VM（untyped） | 2.06 s |
| sjulia VM（typed） | 2.05 s |
| Python 3.14 | 2.92 s |
| juliars AoT（コンパイル＋実行） | 3.22 s |

juliars AoT の実行本体のみの時間は N=10M で 0.074757 s なので、コンパイルを除けば wall time は Julia 本家と同等です。

## for ループ版

`while i < n` を `for i in 0:n-1`（Python は `for i in range(n)`）に変更した版も計測した。
ソースは以下の通り。

- Julia / juliars / sjulia（untyped）: `benchmarks/aizawa_attractor_bench_for.jl`
- sjulia（typed）: `benchmarks/aizawa_attractor_bench_typed_for.jl`
  - 注: sjulia のパーサーは `for i::Int64 in ...` をサポートしていないため、ループ変数の型注釈のみ外している。
- Python: `benchmarks/aizawa_attractor_bench_for.py`

### for ループ版 ホットループ時間（秒）

| 実装 | N=1M | N=5M | N=10M |
|---|---:|---:|---:|
| 公式 Julia 1.12 | 0.007864 | 0.039101 | 0.079438 |
| juliars AoT | 0.007009 | 0.034754 | 0.069285 |
| sjulia VM（untyped） | 0.124335 | 0.672563 | 1.278745 |
| sjulia VM（typed） | 0.112607 | 0.566808 | 1.179593 |
| Python 3.14 | 0.168673 | 0.834689 | 1.666941 |

### for ループ版 相対倍率（公式 Julia 1.12 = 1）

| 実装 | N=1M | N=5M | N=10M |
|---|---:|---:|---:|
| 公式 Julia 1.12 | 1.00× | 1.00× | 1.00× |
| juliars AoT | 0.89× | 0.89× | 0.87× |
| sjulia VM（untyped） | 15.81× | 17.20× | 16.10× |
| sjulia VM（typed） | 14.32× | 14.50× | 14.85× |
| Python 3.14 | 21.45× | 21.35× | 20.98× |

### while 版 vs for ループ版（N=10M）

| 実装 | while（秒） | for（秒） | for/while |
|---|---:|---:|---:|
| 公式 Julia 1.12 | 0.078049 | 0.079438 | 1.018 |
| juliars AoT | 0.069502 | 0.069285 | 0.997 |
| sjulia VM（untyped） | 1.261455 | 1.278745 | 1.014 |
| sjulia VM（typed） | 1.290202 | 1.179593 | 0.914 |
| Python 3.14 | 1.741849 | 1.666941 | 0.957 |

## 結果一致（parity）

すべての実装で checksum がビット一致しています。

| N | checksum |
|---|---:|
| 1M | 645446.1405517095 |
| 5M | 3226728.965144987 |
| 10M | 6454647.040909805 |

## 主な知見

1. **juliars AoT は公式 Julia と同等（やや速い）**
   - N=10M で Julia 0.078 s に対し AoT 0.070 s（0.89×）。
   - コンパイル時間を除けば実行速度は Julia 本家と同等かそれ以上。

2. **sjulia VM は Python 3.14 を上回る**
   - while 版 N=10M: sjulia untyped 1.26 s vs Python 1.74 s（約 1.38× 速い）。
   - for ループ版 N=10M: sjulia typed 1.18 s vs Python 1.67 s（約 1.41× 速い）。
   - wall time でも sjulia 2.05-2.06 s vs Python 2.92 s と sjulia が速い。

3. **for ループにすると sjulia VM（typed）で型注釈の効果が現れる**
   - while 版では typed/untyped がほぼ同等だったが、for ループ版では typed（1.180 s）が untyped（1.279 s）より約 8% 速くなった。
   - これは for ループの方が sjulia の typed-loop 認識器に乗りやすいことを示唆している。
   - なお、sjulia のパーサーは `for i::Int64 in ...` をサポートしていないため、ループ変数の型注釈は外している。

4. **for ループ版でも全体傾向は変わらない**
   - juliars AoT は Julia 本家と同等（0.87-0.89×）。
   - sjulia VM は Python 3.14 を上回る（N=10M: sjulia typed 1.18 s vs Python 1.67 s、約 1.41× 速い）。
   - Python 3.14 は for ループの方が while ループより約 4% 速い（1.742 s → 1.667 s）。

5. **N=10M での対 Julia 倍率（for ループ版）**
   - juliars AoT: 0.87×
   - sjulia VM（typed）: 14.85×
   - sjulia VM（untyped）: 16.10×
   - Python 3.14: 20.98×

## 実行コマンド

### while ループ版

```bash
# 公式 Julia
julia --startup-file=no benchmarks/aizawa_attractor_bench.jl

# sjulia VM（untyped）
target/release/sjulia benchmarks/aizawa_attractor_bench.jl

# sjulia VM（typed）
target/release/sjulia benchmarks/aizawa_attractor_bench_typed.jl

# juliars AoT
target/release/juliars benchmarks/aizawa_attractor_bench.jl --emit-binary /tmp/aizawa_aot
/tmp/aizawa_aot

# Python 3.14
uv run --python 3.14 --no-project python benchmarks/aizawa_attractor_bench.py
```

### for ループ版

```bash
# 公式 Julia
julia --startup-file=no benchmarks/aizawa_attractor_bench_for.jl

# sjulia VM（untyped）
target/release/sjulia benchmarks/aizawa_attractor_bench_for.jl

# sjulia VM（typed）
target/release/sjulia benchmarks/aizawa_attractor_bench_typed_for.jl

# juliars AoT
target/release/juliars benchmarks/aizawa_attractor_bench_for.jl --emit-binary /tmp/aizawa_for_aot
/tmp/aizawa_for_aot

# Python 3.14
uv run --python 3.14 --no-project python benchmarks/aizawa_attractor_bench_for.py
```

## 参照資料

- `docs/benchmark/aizawa_ifs_comparison.md`
- `docs/benchmark/aizawa_attractor_100m_scaling.md`
- `benchmarks/aizawa_attractor_bench.jl`
- `benchmarks/aizawa_attractor_bench_typed.jl`
- `benchmarks/aizawa_attractor_bench.py`
- `benchmarks/aizawa_attractor_bench_for.jl`
- `benchmarks/aizawa_attractor_bench_typed_for.jl`
- `benchmarks/aizawa_attractor_bench_for.py`

---

計測日: 2026-06-28
