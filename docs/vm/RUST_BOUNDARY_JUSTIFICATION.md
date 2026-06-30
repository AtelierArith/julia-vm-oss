# Rust 実装を残す正当な境界に関する設計メモ

## 背景

`AGENTS.md` / `PURE_JULIA_DESIGN.md` では「Pure Julia First」の原則を定めている：

- 原則として `subset_julia_vm/src/julia/`（本家 `julia/base/` と対応）で実装する。
- VM/Rust には「本当に VM や OS/外部ライブラリと接する部分」だけを残す。

本メモは、**あえて Rust に残すべき領域**とその根拠を整理し、今後の Pure Julia 化議論の際に「なぜここは Rust のままか」を参照できるようにするものである。

## 判断基準

Rust に残す条件は以下のいずれかを満たすこととする。

1. **OS/ハードウェア境界**: ファイルシステム、ソケット、プロセス、スレッド、時刻、乱数エントロピーなど、ホスト OS や CPU に直接触る機能。
2. **外部 C/Fortran ライブラリ境界**: BLAS/LAPACK、GMP、MPFR など、既存のネイティブライブラリをラップする機能。
3. **VM 内部メタデータへの低レベルアクセス**: 型情報、メソッドテーブル、フィールドレイアウト、`eval`/`macroexpand`、コード生成に必要な内部表現。
4. **no-JIT 性能境界** (Issue #7876): 配列 / broadcast / matmul / Complex 配列の fast path。本家 Julia は JIT codegen が pure Julia をネイティブ化するため、これらは「表現の問題」ではなく codegen の仕事として吸収される。**sjulia は no-JIT が要件**であり、ホットループを Rust で手書きしない限り iOS 上で実用速度が出ない (原則 #6 VM Performance Priority)。これは条件 1〜3 と異なり「外部世界に触れる」ためではなく「**JIT の代替**」として Rust に残す、**意図的な性能トレードオフ**である。

**後方互換性や既存バイトコードキャッシュとの互換性は、Rust 実装を残す正当な理由ではない。** `sjulia` は本家 Julia と同様の振る舞いを目指す。したがって、上記 4 条件に該当しない Rust 実装はすべて Pure Julia 化の対象となる。

条件 1・2 は「Julia 側から再実装できない」わけではないが、本家 Julia でも `ccall`/`llvmcall`/`@cfunction` などで外部世界に出ている。sjulia は iOS 上で no-JIT 動作が要件であるため、これらの外部呼び出しを Rust の FFI/ネイティブラッパーで集中管理するのが適切である。

**条件 4 には封じ込め義務が伴う。** 条件 1〜3 が外部境界という明確な区切りを持つのに対し、条件 4 は「性能のため」という拡張しやすい根拠であり、放置すると Layer-2 が際限なく肥大する。したがって条件 4 で Rust に残すコードには次の2点を必須とする: (a) **pure-Julia の正当性フォールバックが必ず存在**し、fast path と同一結果を返すこと (gold standard = 通常のスカラ multiple dispatch 経路。`tests/fixtures/complex/complex_array_fallback_parity_7876.jl` で保証)。(b) **拡散を allowlist 監査で止める** (`scripts/check_complex_interleaved_allowlist.sh` / `scripts/check_no_new_domain_builtins.sh`)。

## 残すもの：カテゴリ別

### 1. I/O / ファイルシステム / プロセス

- **対象関数例**: `open`, `close`, `read`, `write`, `print`, `println`, `show` の低レベル sink、`dirname`/`basename`/`joinpath`/`normpath`/`abspath`/`homedir` など。
- **本家対応**: `julia/base/io.jl`, `iobuffer.jl`, `file.jl`, `filesystem.jl`, `path.jl`, `process.jl`, `stream.jl`, `stat.jl`
- **残す理由**: ファイルディスクリプタ、パス解決、プロセス制御、IO バッファは OS に依存する。`dirname`/`basename`/`joinpath` などの純粋な文字列操作は既に Pure Julia 化済みだが、OS 問い合わせを伴うものは Rust のままとする。

### 2. 乱数

- **対象関数例**: `rand`, `randn`, `randperm`, `shuffle`, `MersenneTwister`/`Xoshiro` 状態管理。
- **本家対応**: `julia/stdlib/Random/src/Random.jl`
- **残す理由**: RNG 状態は VM 全体で共有される外部リソースであり、再現性・スレッド安全性・シード管理を Rust 側で一元管理する必要がある。本家も実体は C/Fortran ライブラリまたは LLVM intrinsic に委ねている部分が多い。

### 3. 時間 / sleep

- **対象関数例**: `sleep`, `time`, `time_ns`, `now`, `today`。
- **本家対応**: `julia/base/libc.jl`, `dates/`（stdlib）
- **残す理由**: OS のクロック・スレッドスケジューラに依存する。

### 4. LinearAlgebra

- **対象関数例**: `*`, `\`, `inv`, `eigen`, `svd`, `qr`, `lu`, `norm`, `dot`, `transpose`, `adjoint` などの配列版。
- **本家対応**: `julia/stdlib/LinearAlgebra/src/...`
- **残す理由**: BLAS/LAPACK または Rust の `faer` など外部線形代数ライブラリを呼び出す。本家 Julia と同様に、スカラー演算や小次元演算は Pure Julia で実装し、大規模演算のみ外部ライブラリを利用する。Rust は外部ライブラリ呼び出しの境界に留まる。

### 5. BigInt / BigFloat

- **対象関数例**: `BigInt`, `BigFloat`, 精度・丸め制御、算術演算。
- **本家対応**: `julia/base/gmp.jl`, `julia/base/mpfr.jl`
- **残す理由**: GMP/MPFR へのラッパー。本家も `ccall` で GMP/MPFR を利用しており、sjulia では Rust の `rug` 等のラッパーが該当する。public コンストラクタは Julia 側で実装し、実データの多倍長演算だけを Rust 外部ライブラリ呼び出しに委ねる。

### 6. 深いリフレクション / メタプログラミング / eval

- **対象関数例**: `eval`, `include`, `include_string`, `evalfile`, `macroexpand`, `Meta.parse`, `methods`, `methodswith`, `which`, `@code_typed` 系。
- **本家対応**: `julia/base/reflection.jl`, `julia/base/meta.jl`, `julia/base/expr.jl`
- **残す理由**: コンパイラ/VM の内部状態を直接読み書きする。public ラッパーは Julia 側に置き、primitive は Rust に留まる。

### 7. VM 内部 primitive

- **対象例**: slot 読み書き、フレーム構築、例外キャッチ・再スロー、GC ルート操作、バイトコードディスパッチ、一部の特殊命令。
- **本家対応**: `julia/src/`（C 実装）
- **残す理由**: これらは VM そのものであり、Julia 側に移行する意味がない。

### 8. no-JIT 性能境界: 配列 / broadcast / matmul / Complex 配列 fast path (条件4, Issue #7876)

- **対象例**: `f64`/`Complex` 配列の broadcast、`matmul_complex`、Complex 配列の interleaved
  storage (`[re0, im0, re1, im1, ...]`) とその index/mutation、配列 reduction の Rust fast path。
- **本家対応**: 本家は Complex 配列も「struct 要素の通常配列」(pure Julia) で持ち、SIMD 化は
  codegen の仕事。**表現の問題ではない**。
- **残す理由**: sjulia は no-JIT のため、これらホットループを pure Julia で回すと iOS 上で実用速度に
  届かない。条件 1〜3 (外部境界) には該当せず、**JIT の代替としての意図的な性能トレードオフ**である
  (原則 #6)。これが OS/外部lib に該当しない最大の Rust ドメインコード。
- **トレードオフの封じ込め (必須)**:
  - **正当性フォールバックの保証**: Complex 配列 fast path は、通常のスカラ Complex multiple
    dispatch (pure-Julia gold standard) と**同一結果**を返さなければならない。
    `tests/fixtures/complex/complex_array_fallback_parity_7876.jl` が broadcast (`.+ .- .*`、
    スカラ積、`abs.`/`conj.`/`real.`/`imag.`)・reduction (`sum`)・matmul (行列×ベクトル、行列×行列)
    について fast path = スカラ経路を assert し、表現変更による乖離を検出する。
  - **拡散の防止**: interleaved-Complex 特殊化サイト (現状 vm/ 配下 18 ファイル) は
    `scripts/check_complex_interleaved_allowlist.sh` の allowlist にピン留めし、allowlist 外の
    新規ファイルが interleaved 表現を導入したら監査が落ちる。新規サイトには本条件4 + Issue 番号の
    根拠コメントと allowlist 追記を要求する。Layer-2 全体の肥大は
    `scripts/check_no_new_domain_builtins.sh` (条件4 の LOC/builtin ラチェット) でも監視する。
- **将来の方針**: interleaved 変換の入口を `value/` の少数ヘルパーに集約する表現リファクタは回帰
  リスクが大きいため段階的に行う。本 Issue (#7876) では**まず文書化と封じ込め (allowlist + fallback
  fixture)** を確立し、ヘルパー集約は後続作業とする。

## 境界上の機能も public ラッパーは Pure Julia 側に置く

以下は「純粋な OS 外部境界」ではあるが、本家 Julia 側に簡潔な Julia 実装が存在するもの。本家と同じレイヤリングを sjulia でも維持するため、Pure Julia 側に thin wrapper を置く。

| 領域 | Rust 実体 | Pure Julia 側に置くべきラッパー |
|---|---|---|
| パス操作のうち純文字列部分 | `normpath`/`abspath`/`homedir`（OS 問い合わせあり） | `dirname`/`basename`/`joinpath`（既存） |
| `BigInt`/`BigFloat` コンストラクタ | GMP/MPFR 呼び出し | 型変換・keyword 処理・エラーメッセージ |
| リフレクション primitive | `_fieldnames`, `_fieldtypes`, `_isabstracttype` など | `fieldnames`, `fieldtypes`, `isabstracttype` などの public ラッパー |

## 関連ファイル

- `subset_julia_vm/src/vm/builtins_io.rs`
- `subset_julia_vm/src/vm/exec/print.rs`
- `subset_julia_vm/src/vm/exec/rng.rs`
- `subset_julia_vm/src/vm/exec/sleep.rs`
- `subset_julia_vm/src/vm/builtins_linalg.rs`
- `subset_julia_vm/src/vm/matmul/*.rs`
- `subset_julia_vm/src/vm/builtins_numeric.rs`
- `subset_julia_vm/src/vm/builtins_reflection/primitives.rs`
- `subset_julia_vm/src/vm/builtins_macro/*.rs`
- `subset_julia_vm/src/vm/builtins_exec.rs`

## 関連ドキュメント

- `docs/vm/PURE_JULIA_DESIGN.md`
- `docs/vm/BUILTIN_REMOVAL.md`
- `docs/vm/ARCHITECTURE_OVERVIEW.md`
- `docs/COMPARISION.md` — Pure Julia 実装 と Rust VM 実装の境界を本家 Julia の境界と比較した分析。本メモの3条件 + 「no-JIT 性能境界」の位置づけを俯瞰できる。

## 関連 Pure Julia 化 Issue（**全 CLOSED**）

以下は、本メモで「Rust に残さない」対象として挙げた領域の移行 Issue である。
移行 Issue #6726–#6733 は **すべて CLOSED 済み**であり、二重表現キャリア
（`Value::Dict` / `Value::Set` / `Value::Array`）は #6731 / #6732 / #4568 で
**完全撤去**された（残存参照は到達不能エラー文字列とコメントのみ。詳細は
`PURE_JULIA_DESIGN.md` の Dict / Set / Array 節を参照）。

- #6726 (CLOSED): 残存の数学・数値 builtin (floor/ceil/round/trunc, bit ops, float 分解) の Pure Julia 化
- #6727 (CLOSED): promote_type / convert / promote / signed / unsigned / float の Rust fallback 移行
- #6728 (CLOSED): isequal / isless / hash の Rust intercept 解除
- #6729 (CLOSED): length / size / ndims / eltype / zeros / ones / similar / reshape の Julia dispatch 化
- #6730 (CLOSED): string / repr / sprintf / parse / ncodeunits 等の文字列・パース public API 移行
- #6731 (CLOSED): public Dict API の Value::Dict Rust fallback 除去
- #6732 (CLOSED): Set を Rust-backed HashSet から pure-Julia struct へ移行
- #6733 (CLOSED): range / tuple / HOF reducer のレガシー VM 命令を除去し Pure Julia に一本化
- #4568 (CLOSED): `Value::Array(ArrayRef)` enum variant の完全撤去
