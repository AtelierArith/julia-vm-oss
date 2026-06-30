# Aizawa attractor / IFS フラクタル ベンチマーク

浮動小数点演算が支配的な 2 つのワークロードを、**公式 Julia**、**juliars（AoT）**、
**sjulia**、**sjulia（型注釈版）**、**Python 3.14** の 5 実装で実行し速度を比較した
知見です。`calc_pi`（整数 GCD 中心）が sjulia のネイティブ認識器に乗って好成績
だったのに対し、こちらは **Float64 スカラー演算のホットループ**を測ります。

> **更新 (2026-06-28, PR #8189 マージ後)**: 当初この 2 例は sjulia の VM で
> 公式 Julia/AoT 比 100〜200x・Python 比でも数倍遅かった。Issue #8183 の最適化
> （混合 Int/Float 算術の typed 特化 + typed-loop 認識器の拡張 + 引数型特化の
> Swap 排除）により、**両ワークロードとも native typed-loop 高速路に乗り、
> sjulia が Python 3.14 を上回るところまで改善**した。**本文の表は最適化後の数値**。
> 最適化前との対比は「改善前後」節を参照。生データ:
> `benchmarks/results/aizawa_ifs_after_8183_20260628.md`（後）/
> `benchmarks/results/aizawa_ifs_20260628.md`（前）。

ソース: `benchmarks/aizawa_attractor_bench{,_typed}.jl` / `.py`、
`benchmarks/ifs_fern_bench{,_typed}.jl` / `.py`。

## ワークロード

### 1. Aizawa attractor（陽的 Euler 積分）

3 次元カオス力学系を陽的 Euler 法で `n` ステップ積分する純 Float64 ループ。
checksum は全ステップの `(x + y + z)` の総和。

```julia
function aizawa(n)
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    i = 0
    while i < n
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
        i = i + 1
    end
    (sx + sy) + sz
end
```

### 2. IFS フラクタル（Barnsley のシダ）

4 つのアフィン変換を擬似乱数で選びながら点列を更新する、**整数 LCG + Float64 +
分岐**の混合ループ。乱数は実装非依存にするため glibc 互換 LCG を手書きし、全実装で
同一の乱数列を用いる。checksum は全点の `(x + y)` の総和。

```julia
seed = (1103515245 * seed + 12345) % 2147483648
r = seed / 2147483648.0
# r で 4 分岐し (nx, ny) をアフィン変換、x,y を更新、sx,sy に加算
```

## 計測環境

- OS: macOS 26.5（arm64 / Apple Silicon）
- sjulia: `target/release/sjulia`（SubsetJuliaVM、VM インタプリタ）
- juliars: `target/release/juliars` v0.9.1（AoT → Rust → ネイティブバイナリ、`--emit-binary`）
- 公式 Julia: `julia version 1.12.6`
- Python: `uv run --python 3.14 --no-project` で取得した **CPython 3.14.4**
- 計測方法:
  - **内部計測**（起動を除く）: 各プログラム内で `time_ns()`（Python は
    `time.perf_counter_ns()`）。小さい `n` で warmup 後、3 回実行の **中央値**。
  - **wall time**: `/usr/bin/time -p` による `real`（プロセス起動を含む）。
- すべての実装で checksum がビット一致（後述）。

## 実行コマンド

```bash
# 公式 Julia / sjulia / sjulia 型注釈版
julia --startup-file=no benchmarks/aizawa_attractor_bench.jl
target/release/sjulia       benchmarks/aizawa_attractor_bench.jl
target/release/sjulia       benchmarks/aizawa_attractor_bench_typed.jl

# juliars（AoT: Julia → Rust → ネイティブバイナリ）
target/release/juliars benchmarks/aizawa_attractor_bench.jl --emit-binary /tmp/aizawa_aot
/tmp/aizawa_aot

# Python 3.14
uv run --python 3.14 --no-project python benchmarks/aizawa_attractor_bench.py
```

## 結果（内部計測・中央値, 秒）

### Aizawa attractor（最適化後）

| N | Julia 1.12 | juliars AoT | sjulia | sjulia 型注釈 | Python 3.14 |
|---|-----------:|------------:|-------:|--------------:|------------:|
| 1,000,000  | 0.01071 | 0.00942 | 0.2759 | 0.2025 | 0.3730 |
| 5,000,000  | 0.05384 | 0.04797 | 1.3808 | 1.0132 | 1.8592 |
| 10,000,000 | 0.10682 | 0.09473 | 2.7668 | 2.0316 | 3.7204 |

相対倍率（N=10M、小さいほど速い。基準 = Julia）:

| 実装 | 対 Julia | 備考 |
|---|---:|---|
| juliars AoT | **0.89×** | Julia より僅かに速い |
| sjulia 型注釈 | **19.0×** | Python より速い |
| sjulia | **25.9×** | **Python (34.8×) より速い** |
| Python 3.14 | 34.8× | |

### IFS フラクタル（Barnsley fern）（最適化後）

| N | Julia 1.12 | juliars AoT | sjulia | sjulia 型注釈 | Python 3.14 |
|---|-----------:|------------:|-------:|--------------:|------------:|
| 1,000,000  | 0.00347 | 0.00334 | 0.1714 | 0.1444 | 0.3044 |
| 5,000,000  | 0.01735 | 0.01608 | 0.8575 | 0.7232 | 1.5204 |
| 10,000,000 | 0.03489 | 0.03020 | 1.7176 | 1.4429 | 3.0429 |

相対倍率（N=10M、基準 = Julia）:

| 実装 | 対 Julia | 備考 |
|---|---:|---|
| juliars AoT | **0.87×** | Julia と同等（僅かに速い） |
| sjulia 型注釈 | **41.4×** | untyped より速い（退行解消） |
| sjulia | **49.2×** | **Python (87.2×) より速い** |
| Python 3.14 | 87.2× | |

## wall time（プロセス全体, `real` 秒）

3 つの N（1M/5M/10M）を 1 プロセスで連続実行した合計。起動を含む。

| ワークロード | Julia | juliars AoT | sjulia | sjulia 型注釈 | Python 3.14 |
|---|---:|---:|---:|---:|---:|
| Aizawa | 0.29 | 0.15 | 4.51 | 3.31 | 5.96 |
| IFS    | 0.17 | 0.04 | 2.79 | 2.36 | 4.91 |

wall time でも sjulia（型注釈版）が Python を上回る（Aizawa 3.31 < 5.96、IFS 2.36 < 4.91）。
最適化前は sjulia の wall が Python の数倍だった（Aizawa 14.04 / IFS 8.44 vs Python ~2.3）。

## 結果一致（parity）

5 実装すべてで checksum がビット一致（IEEE-754 同一演算列のため）:

| ワークロード | N=1M | N=5M | N=10M |
|---|---|---|---|
| Aizawa | 645446.1405517095 | 3226728.965144987 | 6454647.040909805 |
| IFS    | 6961030.180786168 | 34835195.14680661 | 69667017.45156752 |

## 主な知見

1. **juliars (AoT) は公式 Julia と同等（むしろ僅かに速い）**
   - Aizawa N=10M: Julia 0.0664 s vs AoT 0.0615 s、IFS N=10M: 0.0252 s vs 0.0240 s。
   - Julia → Rust → LLVM のネイティブコンパイル経路で、JIT なしに Julia 相当の
     最適化（インライン・ベクトル化）が効く。`calc_pi` と同じ傾向。

2. **最適化後は sjulia が Python 3.14 を上回る（`calc_pi` と同傾向に回復）**
   - Aizawa N=10M: sjulia 2.77 s vs Python 3.72 s（**sjulia が 1.35×、型注釈版は 1.83× 速い**）。
   - IFS N=10M: sjulia 1.72 s vs Python 3.04 s（**sjulia が 1.77×、型注釈版は 2.11× 速い**）。
   - Issue #8183 の最適化で、これら **汎用 Float64 ループも `calc_pi` と同じ native
     typed-loop 認識器に乗る**ようになった（`DivF64`/`ModI64`/融合 I64 load/`NegF64`
     をサポートし、命令毎ディスパッチを Rust ループに置換）。混合 `Int64/Float64`
     算術が毎反復のメソッド `Call` だったのも typed 命令に特化し、untyped ループも
     引数型特化経由で認識器に乗る。
   - 対 Julia 倍率は **Aizawa 131.7×→25.9×、IFS 208×→49.2×** に縮小（後述「改善前後」）。
     公式 Julia/AoT には依然及ばないが、CPython 3.14 の特殊化適応インタプリタ
     （PEP 659 系）を上回る水準。

3. **最適化後は型注釈が両ワークロードで有効（IFS の逆効果が解消）**
   - Aizawa: 型注釈で 25.9×→19.0×（対 Julia）。密な Float64 算術で I64/F64 専用命令が効く。
   - IFS: 49.2×→41.4×。最適化前は **型注釈版が untyped より遅い**（+29%）逆効果だったが
     解消。当初は「毎反復の typeassert/convert コスト」が原因と推定していたが、計測の結果
     **真因は混合 `Int64 / Float64` 除算（`seed / 2147483648.0`）が毎反復フルメソッド
     `Call` になっていたこと**で、型注釈版本体に convert は残っていなかった（#8147 で省略済み）。
     Stage 1 の混合算術特化でこのコールが消え、型注釈版が正しく untyped 以上の速度に戻った。
     （関連: `calc_pi_gcd_comparison.md` の型注釈節、Issue #8147 / #8183）

4. **juliars (AoT) で発見した 2 つのギャップ（upstream Julia / sjulia VM では動作）→ 修正済み**
   - ベンチマーク作成中に AoT で次の 2 件に遭遇し Issue 起票のうえ修正した。
   - **Issue #8180（修正済み）**: `a + b + c` のような **3 項以上の `+`/`*`** は n 項
     `+(a,b,c)`（`afoldl`）に解決され、AoT が変数到達解析で変分 `+` を引き込み
     `HasShape{1}` を生成できず unsupported になっていた。修正: call graph が
     畳み込み対象の演算子呼び出しに辺を張らないようにした。Aizawa は自然な
     `sx + sy + sz` で AoT ビルド可能。
   - **Issue #8181（修正済み）**: `if`/`elseif` の**分岐内で初代入した変数を分岐後に使う**と、
     AoT が変数を hoist せず生成 Rust が `cannot find value` でコンパイル不能だった。
     修正: codegen が「入れ子ブロックで初代入され別スコープから参照される局所変数」を
     関数先頭の遅延宣言 `let mut x: T;` に巻き上げ、ブロック内は代入として出力する。
   - **補足（残存・別 Issue）**: IFS の `nx`/`ny` は依然 if 前で `0.0` 初期化し
     3 項和を二項化している。これは **typed スロットへの代入で n 項 `+` の結果型が
     `Any` 推論され Any→Float64 変換が AoT 非対応になる**ため（Issue #6978 / #6968、
     #8180 とは別の型安定性の制約）。

## 改善前後（Issue #8183 / PR #8189）

最適化前後は **別セッション計測**でマシン負荷が異なるため、絶対秒は直接比較せず
**機械非依存の「対 Julia 倍率」（N=10M）**で示す（同一 run 内の sjulia↔Python 比較は
本文の各表のとおり有効）。

| 実装 / ワークロード | 改善前 (対 Julia) | 改善後 (対 Julia) | 改善 |
|---|---:|---:|---|
| sjulia / Aizawa | 131.7× | **25.9×** | 約 5.1× 高速化 |
| sjulia 型注釈 / Aizawa | 92.3× | **19.0×** | 約 4.9× |
| sjulia / IFS | 208× | **49.2×** | 約 4.2× |
| sjulia 型注釈 / IFS | 268×（untyped より遅い） | **41.4×**（untyped より速い） | 退行解消 + 約 6.5× |

- 改善前は「汎用 Float64 ループでは公式 Julia/AoT 比 100〜200×、Python 3.14 比でも
  数倍遅い」状態だった。最適化後は **Python 3.14 を上回り、対 Julia 倍率が 1/4〜1/6** に。

## 考察

- AoT は「Julia を事前に Rust 化してネイティブビルド」する経路で、JIT 無しに Julia
  相当の速度を達成できる。iOS のような no-JIT 環境で Julia 相当性能を得る本命。
- sjulia（VM）は iOS 向け no-JIT インタプリタ。当初 **汎用 Float64 ループの VM
  ディスパッチ・オーバーヘッド**が明確なボトルネックだったが、Issue #8183 で
  `calc_pi` の native typed-loop 認識器を Float64 ODE/反復系へ広げ（`DivF64`/`ModI64`/
  融合 I64 load/`NegF64` 追加、混合 Int/Float 算術の typed 特化、引数型特化の Swap 排除）、
  公式 Julia/AoT には及ばないものの Python 3.14 を上回る水準まで改善した。
- 残る差（対 Julia 25〜49×）は、native ループに乗らない一般経路（命令毎ディスパッチ）と、
  特化版が peephole 融合（`LoadMulF64Slot` 等）を完全には再現しない点に由来。さらなる
  認識器カバレッジ拡大・特化コードの融合適用が今後の最適化ポイント。

## 生データ

`benchmarks/results/` に harness 出力を保存。
最適化後: `benchmarks/results/aizawa_ifs_after_8183_20260628.md`、
最適化前: `benchmarks/results/aizawa_ifs_20260628.md`。中央値は本文表のとおり。

---

計測日: 2026-06-28（Apple Silicon, Julia 1.12.6, juliars v0.9.1,
sjulia: PR #8189 マージ後の main, CPython 3.14.3 / uv）
