# Aizawa / IFS 生データ（Issue #8183 最適化 *後*, 2026-06-28）

PR #8189（混合 Int/Float 算術特化 + typed-loop 認識器拡張 + specializer 修正）を
main にマージした **後** の再計測。harness は各プログラムを 3 回実行し、N ごとに
`time_ns` の中央値を採る（`benchmarks/results/aizawa_ifs_20260628.md` が最適化前）。

- 環境: macOS（arm64 / Apple Silicon）, Julia 1.12.6, juliars v0.9.1（`--emit-binary`),
  sjulia `target/release/sjulia`（PR #8189 マージ後の main）, CPython 3.14.3（uv）。
- 内部計測 = `time_ns()`（Python は `time.perf_counter_ns()`）, 3 回の中央値, 秒。
- wall = `/usr/bin/time -p` の `real`（1M+5M+10M を 1 プロセスで連続実行）, 3 回の最小。

## Aizawa attractor — 内部計測中央値（秒）

```
## julia
  N=1000000   median_t=0.010713875  r=645446.1405517095
  N=5000000   median_t=0.053836708  r=3.226728965144987e6
  N=10000000  median_t=0.106824167  r=6.454647040909805e6
## juliars_AoT
  N=1000000   median_t=0.00942      r=645446.1405517095
  N=5000000   median_t=0.047973     r=3226728.965144987
  N=10000000  median_t=0.094732     r=6454647.040909805
## sjulia
  N=1000000   median_t=0.275877     r=645446.1405517095
  N=5000000   median_t=1.380841     r=3.226728965144987e6
  N=10000000  median_t=2.766835     r=6.454647040909805e6
## sjulia_typed
  N=1000000   median_t=0.202455     r=645446.1405517095
  N=5000000   median_t=1.0132       r=3.226728965144987e6
  N=10000000  median_t=2.031559     r=6.454647040909805e6
## python3.14
  N=1000000   median_t=0.372996042  r=645446.1405517095
  N=5000000   median_t=1.859151917  r=3226728.965144987
  N=10000000  median_t=3.720355625  r=6454647.040909805
```

Aizawa wall（real, 最小, 秒）: julia 0.29 / juliars_AoT 0.15 / sjulia 4.51 /
sjulia_typed 3.31 / python3.14 5.96

## IFS フラクタル — 内部計測中央値（秒）

```
## julia
  N=1000000   median_t=0.003470875  r=6.961030180786168e6
  N=5000000   median_t=0.017350666  r=3.483519514680661e7
  N=10000000  median_t=0.034888458  r=6.966701745156752e7
## juliars_AoT
  N=1000000   median_t=0.00334      r=6.961030180786168e6
  N=5000000   median_t=0.016079     r=3.483519514680661e7
  N=10000000  median_t=0.030199     r=6.966701745156752e7
## sjulia
  N=1000000   median_t=0.171374     r=6.961030180786168e6
  N=5000000   median_t=0.857493     r=3.483519514680661e7
  N=10000000  median_t=1.717649     r=6.966701745156752e7
## sjulia_typed
  N=1000000   median_t=0.144422     r=6.961030180786168e6
  N=5000000   median_t=0.723186     r=3.483519514680661e7
  N=10000000  median_t=1.442942     r=6.966701745156752e7
## python3.14
  N=1000000   median_t=0.30440925   r=6961030.180786168
  N=5000000   median_t=1.520438917  r=34835195.14680661
  N=10000000  median_t=3.042852667  r=69667017.45156752
```

IFS wall（real, 最小, 秒）: julia 0.17 / juliars_AoT 0.04 / sjulia 2.79 /
sjulia_typed 2.36 / python3.14 4.91

## checksum（全 5 実装でビット一致）

| ワークロード | N=1M | N=5M | N=10M |
|---|---|---|---|
| Aizawa | 645446.1405517095 | 3226728.965144987 | 6454647.040909805 |
| IFS    | 6961030.180786168 | 34835195.14680661 | 69667017.45156752 |
