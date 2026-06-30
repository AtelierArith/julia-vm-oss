# Pure Julia 実装 と Rust VM 実装の境界比較 — sjulia vs 本家 Julia

> 本ドキュメントは、SubsetJuliaVM (sjulia) における **Pure Julia 層** と **Rust VM 層**
> の境界を、移植元である本家 Julia (`./julia`, 1.14.0-DEV) の **Pure Julia (`base/`) と
> C ランタイム (`src/`) の境界** と突き合わせて比較し、sjulia が改善すべき事項を整理した
> ものである。
>
> 関連: `docs/vm/PURE_JULIA_DESIGN.md`, `docs/vm/RUST_BOUNDARY_JUSTIFICATION.md`,
> `docs/vm/BUILTIN_REMOVAL.md`, `docs/vm/ARCHITECTURE_OVERVIEW.md`
>
> *作成: 2026-06-25 / 比較対象 julia: v1.3.0-alpha-16940 (VERSION = 1.14.0-DEV)*

---

## 0. 結論サマリ

| 観点 | 評価 |
|---|---|
| **Intrinsic 層 (Layer 1)** | ✅ 健全。76変種が本家 `intrinsics.h` に素直に対応。過不足なし |
| **二重表現 (dual representation)** | ✅ 解消済み。`Value::Dict`/`Value::Set`/`Value::Array` キャリアは全撤去 (#6731/#6732/#4568) |
| **Builtin 層 (Layer 2)** | ⚠️ 上流の C 境界(67関数)より構造的に厚い。**no-JIT 由来は設計上正当**だが、性能無関係なドメインロジック (~1k行) と Complex 配列特殊化が改善余地 |
| **型推論の型別ハードコード** | ⚠️ `tfunc_real/imag/conj/abs2` 等。過去のコンパイル爆発 (#7215/#7186) と同根 |
| **境界ドキュメント** | ✅ ドリフト解消済み (#7879)。`PURE_JULIA_DESIGN.md` の Set/Dict/Array カバレッジを撤去済みキャリア前提から実態へ更新済み |

最重要メッセージ: **sjulia の Rust Layer-2 が本家の C 境界より厚いこと自体は「no-JIT で
配列/broadcast/matmul を高速実行する」ための設計上の必然であり、原則 #6 (VM Performance
Priority) に沿う。** 改善対象は「厚さそのもの」ではなく、(a) 性能根拠のないドメインロジック
の Pure Julia 化、(b) 性能目的の Rust を明示的トレードオフとして封じ込め・文書化、
(c) 型推論のハードコード削減、の3点である。

---

## 1. 3層モデルと本家 Julia 境界の対応

sjulia は `docs/vm/PURE_JULIA_DESIGN.md` の通り3層構成:

```
Layer 3: Pure Julia     型定義・演算・promotion・数学関数・コレクション・broadcast
Layer 2: Rust VM        ディスパッチ機構・表示・配列演算・組み込み関数 (builtins)
Layer 1: Rust Intrinsic CPU命令 (add_int, mul_float, ...)・ハッシュテーブル
```

本家 Julia の対応する境界:

```
本家 Layer 3: Pure Julia base/ (158ファイル) + Compiler/ (型推論) + stdlib/
本家 Layer 2:  —  (存在しない。codegen が Layer3 を実行時にネイティブ化する)
本家 Layer 1: C ランタイム src/  (builtins.c の 67関数 + intrinsics.h の 94命令)
```

**決定的な構造差**: 本家には Layer 2 が無い。本家は **JIT codegen** が pure Julia の
`base/` をその場でネイティブコードに変換するため、配列ループも broadcast も「pure Julia の
ソースが速いネイティブコードになる」。C 境界 (`builtins.c`) は型・フィールド・メモリ・
ディスパッチといった **VM 内部 primitive 67関数だけ** に絞られている。

sjulia は **no-JIT** (iOS 要件)。pure Julia の要素ループをインタプリタで回すと遅いため、
性能critical な配列/broadcast/matmul/Complex を **Rust で手書き** せざるを得ない。これが
sjulia の Layer 2 が本家 C 境界より厚くなる根本理由であり、原則 #6 が明示的に許容する。

---

## 2. 層別の定量比較

### Layer 1: Intrinsic — ✅ 健全

| | 本家 Julia | sjulia |
|---|---|---|
| 定義場所 | `src/intrinsics.h` (X-macro) | `src/intrinsics.rs` (`enum Intrinsic`) |
| 命令数 | **94** | **76** (`NegInt`…`BigFloatToString`) |
| 命名規約 | `add_int`, `slt_int`, `sqrt_llvm` | 同一規約を踏襲 (`add_int`, `slt_int`, `sqrt_llvm`) |
| BigInt/BigFloat | `ccall` で GMP/MPFR | `add_bigint`/`add_bigfloat` 等 (外部lib境界, 正当) |

sjulia の intrinsic 集合は CPU/FPU 命令と GMP/MPFR 境界に正しく限定され、本家 `intrinsics.h`
の部分集合として整合する。**未実装の約18命令は「sjulia subset が未使用の命令」(atomic,
pointerref, fptrunc 等) であり、欠落ではなく対象外。** この層に改善要件は無い。

> 注: `Intrinsic` enum は append-only 制約あり (bincode キャッシュ互換, [[feedback_builtinop_enum_append_only]])。
> 新変種は必ず末尾に追加すること。

### Layer 2: Rust Builtin — ⚠️ 改善余地

| | 本家 Julia | sjulia |
|---|---|---|
| 実体 | `builtins.c` (2,734行, **67関数**) | `builtins.rs` + `vm/builtins_*.rs` (**約10,620行**) |
| スコープ | VM内部primitiveのみ (typeof/getfield/apply_type/memoryref/invoke/throw…) | 上記 + 配列演算 + broadcast + matmul + 表示 + 数学/数値/文字列 fast path |
| 厚さの理由 | codegen が Layer3 をネイティブ化 → C境界は薄くて済む | **no-JIT のため性能critical を Rust 手書き** |

本家の 67 ビルトイン (`JL_BUILTIN_FUNCTIONS` X-macro) は次の通り、すべて
`RUST_BOUNDARY_JUSTIFICATION.md` の条件(3) "VM内部メタデータ低レベルアクセス" に該当:

```
typeof isa issubtype typeassert sizeof fieldtype nfields apply_type svec _typevar
_structtype _abstracttype _primitivetype getfield setfield modifyfield swapfield
replacefield getglobal setglobal memorynew memoryrefnew memoryrefget memoryrefset
invoke invokelatest applicable _apply_iterate intrinsic_call opaque_closure_call
throw ifelse finalizer _expr _typebody current_scope compilerbarrier _import _using …
```

sjulia の Layer-2 約10,620行を探索分類した内訳:

| ファイル | 行 | ファミリ | 判定 |
|---|---|---|---|
| `builtins_io.rs` | 1,175 | print/IOBuffer/FS/path | **LEGIT** (OS境界, 条件1) |
| `builtins_types.rs` | 1,507 | typeof/isa/subtype/fieldcount/typevar | **LEGIT** (条件3) |
| `builtins_equality.rs` | 1,263 | egal(===)/isequal/hash/isless | **LEGIT** (VM意味論/ハッシュ) |
| `builtins_linalg.rs` | 1,015 | LU/det/inv/svd/eigen/qr | **LEGIT** (条件2, 外部線形代数) |
| `builtins_numeric.rs` | 346 | BigInt/BigFloat ctor・精度制御 | **LEGIT** (条件2, GMP/MPFR) |
| `builtins_math.rs` | 320 | sqrt/round系/bit ops/fma | **LEGIT** (CPU intrinsic) |
| `builtins_collections.rs` | 586 | length/eltype/MemoryRef* | **LEGIT** (条件3, VMストレージ) |
| `builtins_dicts.rs` | 463 | Dict* → pure-Julia Dict{K,V}へ委譲 | **LEGIT** (thin trampoline) |
| `builtins_arrays.rs` | 1,113 | alloc/mutation/reshape/index | **MIXED** (alloc=LEGIT, mutation wrapper=要精査) |
| `builtins_strings.rs` | 771 | StringNew/sprintf/occursin/base変換 | **MIXED** (encode/printf=LEGIT, occursin/parse(base)=候補) |
| `builtins_exec.rs` | 470 | convert/promote/tuple/compose/iterate | **MIXED** (convert dispatch=LEGIT, tuple/compose=候補) |
| `builtins_types_conversion.rs` | 513 | float変換/reinterpret/signed/unsigned | **MIXED** (reinterpret=候補) |

**DOMAIN-LOGIC-IN-RUST 候補規模: 約710〜1,020行 (Layer-2 の 7〜11%)。** ただし mutation 系は
pure Julia `push!` ラッパーが Rust の `_growend!` 相当 primitive を呼ぶ構造の可能性があり、
個別精査が必要 (= 上限値)。確実な候補は `occursin` / `parse(Int, s; base)` / `tuple first/last` /
`compose` / `reinterpret` など。

### Layer 3: Pure Julia — ✅ 充実

| | 本家 Julia | sjulia |
|---|---|---|
| base/ | 158ファイル | 57ファイル (subset) |
| stdlib/ | 22パッケージ | 10パッケージ (Base64/Broadcast/Dates/InteractiveUtils/Iterators/LinearAlgebra/Printf/Random/Statistics/Test) |
| 総行 | — | 約43,000行 |
| `ccall` 境界 | 多数 (GMP/MPFR/utf8proc/libuv…) | **わずか2サイト** (`operators.jl`, `reflection.jl`) — 境界はほぼ intrinsic/builtin に集約 |

主要ファイルは本家と同パス・同設計 (`iterators.jl` 5,231行, `array.jl` 4,303行,
`broadcast.jl` 2,856行)。Rational ~98%, Trig/Exp/Log 100%(Float64), Dict ~95% が Pure Julia 化済み。

---

## 3. 本家境界からの乖離分析

### 乖離① 二重表現の解消 — ✅ 完了 (改善不要)

本家は「1型1表現」。sjulia もかつて `Value::Dict`(Rust) と `Dict{K,V}`(Julia) 等が共存し、
検出/変換/表示で Rust 特殊化が必要だった (`native value op が Value::StructRef を忘れる`系
バグの温床, 過去 #6685 等)。**現状、移行 #6726–#6733 はすべて CLOSED:**

- `Value::Dict` 撤去 (#6731) — `grep Value::Dict` の17ヒットは**全てコメント/スタブ**
- `Value::Set` 撤去 (#6732)
- `Value::Array(ArrayRef)` 撤去 (#4568/#6807) — `NativeArray` 参照は残**1**箇所(コメント)

→ Dict/Set/Array は本家同様「単一の pure-Julia struct over Memory{T}」に統一。**乖離解消済み。**

### 乖離② Complex 配列の interleaved 表現 — ⚠️ 性能トレードオフ (要封じ込め)

本家は Complex 配列も「struct 要素の通常配列」(pure Julia)。SIMD 化は codegen の仕事で
**表現の問題ではない**。sjulia は no-JIT のため `[re0, im0, re1, im1, …]` interleaved 配列を
**Rust で持つ** (`broadcast.rs`, `matmul/`, `value/array*.rs`, `builtins_linalg.rs` 等で
約40〜55サイトの Complex 専用特殊化)。

これは **OS/外部lib に該当しない最大の Rust ドメインコード**。BLAS interop と broadcast 速度
のための意図的トレードオフだが、`RUST_BOUNDARY_JUSTIFICATION.md` の3条件には当てはまらず、
**トレードオフとして文書化されていない**。`PURE_JULIA_DESIGN.md` は Complex を ~40サイトと
記すのみで「なぜ Rust か」が条件外。

### 乖離③ 型推論の型別ハードコード tfunc — ⚠️ 改善対象

本家は型推論を Compiler.jl で行い、`Complex{T}` の `real`/`imag`/`conj`/`abs2` 等の戻り型は
**pure Julia メソッド本体 + 宣言戻り値型から通常推論**する。sjulia は
`compile/tfuncs/complex_ops.rs` (582行中 ~114行) に `tfunc_real`/`tfunc_imag`/`tfunc_conj`/
`tfunc_abs2` を型パターンマッチでハードコード。

これは過去のコンパイル爆発 #7215/#7186 ([[project_7215_symbolics_diff_inference_blowup]],
[[project_7186_partialstruct_inference_hang]]) と同根の「call-site 補間推論が宣言戻り値型を
無視 → 相互再帰で爆発」クラス。型別ハードコードは個別には正しくても、**宣言戻り値型による
短絡 + pure-Julia メソッド推論** に寄せるほど将来の爆発を予防できる。

### 乖離④ unicode.rs (3,896行) — ✅ 正当 (改善不要)

懸念対象だったが、実体は **生成ファイル** (`scripts/generate_unicode.py` が本家
`stdlib/REPL/src/latex_symbols.jl` / `emoji_symbols.jl` から自動生成する LaTeX↔Unicode
マッピング表, 2,555 + 1,242項目)。Unicode 正規化 (NFC/NFD) の独自再実装**ではない**。
本家の utf8proc C ライブラリ相当の **条件(2) 外部データ境界** であり正当。再実装不要。

### 乖離⑤ 正の境界監査の不在 — ⚠️ 改善対象

既存の `scripts/check_*.sh` は「撤去済みキャリアが復活しないこと」を守る**負の監査**
(`check_value_array_allowlist.sh`, `check_native_value_ops_resolve_structref.sh`,
`check_memory_to_array_ref_allowlist.sh` 等)。だが「**新規のドメインロジック Rust builtin を
増やさない**」という**正の監査**は無い。Layer-2 の肥大は気づかぬうちに進みうる。

加えて `docs/vm/PURE_JULIA_DESIGN.md` (2026-06-11) のカバレッジ表記
(`Set ~65% core via intrinsics`, `Value::Dict (Rust-backed)` 共存記述, `Array ~50%`) は
**#6732/#6731/#4568 完了前の数値**でドリフトしている。

---

## 4. sjulia が改善すべき事項 (優先順位付き)

### P1. 性能根拠のないドメインロジックを Pure Julia 化 (~700–1,000行候補) — #7875

- **現状**: `builtins_strings.rs` の `occursin`/`parse(Int,base)`、`builtins_exec.rs` の
  `tuple first/last`/`compose`/`iterate`、`builtins_types_conversion.rs` の `reinterpret`
  等、本家が pure Julia base/ で実装するロジックが Rust に残存。
- **根拠**: 原則 #2/#3 (Pure Julia First)。性能critical でないものは Rust に置く理由が無い。
- **提案**: `BUILTIN_REMOVAL.md` の移行チェックリストで1関数ずつ移行。**先に本家
  `julia/base/` の実装を読み、上流挙動を gold standard に** ([[feedback_verify_against_upstream_julia]])。
  mutation 系は pure-Julia ラッパー/Rust primitive の境界を個別精査 (規模上限の一部は LEGIT の可能性)。
- **規模/リスク**: 小〜中 / 低 (各移行に parity fixture を付ける)。

#### 個別精査の結果 (#7875)

候補を1つずつ精査した結論。「~700–1,000行」は **mutation ラッパー等を含む上限値** であり、確実に
「性能根拠のないドメインロジック」と言えるものは想定より小さかった。

| 候補 | 判定 | 根拠 |
|---|---|---|
| `occursin` (非regex) | ✅ **既に pure-Julia** | `base/strings/search.jl` で実装済み。残る `Occursin` builtin は `occursin(Regex, s)` 専用 (regex エンジン = 条件2 LEGIT)。 |
| `parse(Int, s; base=N)` | ✅ **移行 (本 Issue)** | ドメインロジック (`_tryparse_int`) は既に pure-Julia。`StringToIntBase` builtin を撤去し、compiler が kwargs 形を pure-Julia `_parse_int_base(s, base)` 呼び出しへ書き換え。副産物として underscore 受理バグ #7942 も修正 (全 parse 経路が upstream 準拠に)。 |
| `reinterpret` | ⚠️ **条件4寄り (perf)** | bit-level reinterpret。`base/float.jl` の float 分解など **hot path で多用**。型 dispatch は pure-Julia 化可能だが raw bitcast は高速 intrinsic に残す必要があり、no-JIT 性能境界 (条件4) に近い。単独 Issue 推奨。 |
| `compose` (`∘`) | ⚠️ **表現結合 → defer** | `Value::ComposedFunction` は VM の callable-value 表現。pure-Julia 化には callable struct (functor) サポートが要る = ドメインロジックでなく表現変更。 |
| `tuple first/last` | ⚠️ **特殊ケース蓄積 → defer** | `TupleFirst`/`TupleLast` builtin は Tuple+Range+String+Struct を扱い、typed-element 保存 (#3550)・string→Char (#2048) 等の蓄積あり。pure-Julia `first(::Tuple)`/`last(::Tuple)` は既存だが builtin 撤去は Range/String 回帰リスク。 |

**結論**: 確実に安全な「性能根拠なしドメインロジック」は occursin (既済) と parse(Int,base) (本 Issue で完了)。
残りは LEGIT (regex/perf bitcast) か表現結合 (compose/tuple builtin) で、それぞれ専用 Issue 向き。
代表的な migration パターン (compiler 書き換え + builtin 撤去 + cache version bump + parity fixture) を
parse(Int,base) で確立した。表現結合候補 (compose / tuple first-last) の追跡は #7944。

- **結果 (#7875)**: `parse(Int, s; base=N)` を pure-Julia 化 (`_parse_int_base` + `_tryparse_int`)。
  `BuiltinId::StringToIntBase` 撤去、`CACHE_VERSION` 64 へ bump、`string_parse_base.jl` に移行 +
  underscore 回帰 fixture 追加 (#7942 修正含む)。個別精査表 (上) で残候補を分類。

### P2. Complex 配列の Rust 特殊化をトレードオフとして文書化・封じ込め — #7876 ✅ 文書化+封じ込め DONE（ヘルパー集約は後続）

- **現状**: interleaved 表現が約40〜55サイトに分散、トレードオフ説明なし。
- **提案**:
  1. `RUST_BOUNDARY_JUSTIFICATION.md` に「**条件4: no-JIT 性能境界**(配列/broadcast/matmul/
     Complex の fast path)」を新設し、なぜ Rust かを明文化。
  2. Complex fast path に対し **pure-Julia の正当性フォールバックが必ず存在** することを
     fixture で保証 (gold standard = 通常配列の pure Julia 経路)。
  3. interleaved 変換の入口を `value/` の少数ヘルパー (`to_interleaved_complex` 等) に集約し
     allowlist 監査を付ける (Complex特殊化サイトの拡散を止める)。
- **規模/リスク**: 中 / 中 (表現変更は回帰リスク大。まず封じ込めと文書化から)。
- **結果 (#7876)**: 提案1・2・3の「封じ込めと文書化」部分を実施。
  1. `RUST_BOUNDARY_JUSTIFICATION.md` に**条件4 (no-JIT 性能境界)** を新設し、Complex 配列 fast path
     を「JIT の代替としての意図的トレードオフ」と明文化（封じ込め義務2点付き: フォールバック保証 +
     allowlist 監査）。専用カテゴリ節 (8. no-JIT 性能境界) も追加。
  2. `tests/fixtures/complex/complex_array_fallback_parity_7876.jl` を追加。broadcast (`.+ .- .*`、
     スカラ積、`abs./conj./real./imag.`)・`sum`・matmul (行列×ベクトル/行列×行列) について
     **fast path = スカラ pure-Julia dispatch** を assert（upstream julia と parity 確認済み）。
  3. `scripts/check_complex_interleaved_allowlist.sh` を新設。interleaved-Complex 特殊化を **18 ファイル
     の allowlist にピン留め**し、allowlist 外の新規サイトを監査で阻止（CODE_AUDITS.md 登録）。
  - **後続作業**: 提案3の「`value/` の少数ヘルパー (`to_interleaved_complex` 等) への集約」は表現変更で
     回帰リスクが大きいため本 Issue では実施せず、allowlist による封じ込め確立に留めた（issue 方針
     「まず封じ込めと文書化から」に従う）。

### P3. 型推論の型別ハードコード tfunc を宣言戻り値型ベースへ移行 — #7877 ⚠️ 調査完了・前提 (#7950) ブロック

- **現状**: `compile/tfuncs/complex_ops.rs` 等に型固有 tfunc。
- **根拠**: #7215/#7186 の予防。`PROMOTION.md`/`LATTICE_TYPE.md` の方針と整合。
- **提案**: 補間推論パス先頭で `func.return_type` があれば短絡する設計
  (#7215 で実証済み) を一般化し、Complex 等の
  pure-Julia メソッドに戻り値型注釈を付けてハードコード tfunc を段階的に削除。
- **規模/リスク**: 中 / 中 (推論変更は full suite + AoT gate 必須)。

#### 調査結果 (#7877) — 現状のエンジンでは実現不可 → 前提 #7950

提案どおり `tfunc_real`/`imag`/`conj`/`abs2` を撤去し、pure-Julia メソッドに `::T` /
`::Complex{T}` の戻り値型注釈を付けて #7215 短絡に委ねる実装を試作・実測した結果、
**現状の推論エンジンでは精度退行**することが判明した。

- 根因: #7215 短絡 (`abstract_interp/engine/mod.rs:~3970`) は宣言戻り値型を
  `julia_type_to_lattice(declared_rt)` で**そのまま** lattice 化し、呼び出し引数から
  メソッドの型変数を**束縛しない**。戻り値型が `T` (型変数) の場合、`Complex{Float64}`
  引数から `T=Float64` を具体化できず、`T` の上界 (`Real`) / `Top` に潰れる。
- 実測: tfunc 撤去 + 注釈付与で Complex アクセサの戻り値型推論が `T→Real/Top` に潰れた
  (runtime 値は正しいが compile-time 推論精度が低下 → 特殊化を損ない原則#6 に反する)。
  ハードコード tfunc は `extract_complex_element_type("Complex{Float64}")→Float64` で
  この parametric 抽出を担っている。
- **判断**: tfunc 撤去は migration でなく**退行**になる。`#7215` 短絡が parametric 型変数を
  引数から具体化できるようになる**エンジン拡張が前提** (= **#7950** で追跡)。それまで
  tfunc を retain（`complex_ops.rs` 冒頭に根拠コメントを追記）。
- **規模/リスク再評価**: 当初「中/中」だが、前提エンジン拡張は #7215/#7186 爆発防止の
  load-bearing コードに触れ parametric 全関数へ波及するため**大/高**。安易な実装は不可。

### P4. 「正の境界監査」スクリプトを追加 — #7878

- **提案**: `scripts/check_no_new_domain_builtins.sh` を新設。`vm/builtins_*.rs` の
  `BuiltinId` ハンドラ数 or LOC を baseline と比較し、増分に「条件1〜4 のどれか」+ Issue 番号の
  根拠コメントを必須化 (CODE_AUDITS.md に登録)。
- **規模/リスク**: 小 / 低。**注意**: `.github/workflows/*.yml` への push は権限不足で失敗する
  ため、CI 登録はメンテナ向けに `docs/vm/CODE_AUDITS.md` へ記載に留める ([[feedback_cannot_push_workflow_files]])。

### P5. 境界ドキュメントのドリフト解消 — #7879 ✅ DONE

- **提案**: `PURE_JULIA_DESIGN.md` のカバレッジ表 (Set/Dict/Array) を #6731/#6732/#4568 完了後の
  実態に更新。`RUST_BOUNDARY_JUSTIFICATION.md` の「関連 Pure Julia 化 Issue (#6726–#6733)」を
  **全 CLOSED** と明記し、本ドキュメント (`COMPARISION.md`) を相互参照に追加。
- **規模/リスク**: 小 / なし。
- **結果 (#7879)**: `PURE_JULIA_DESIGN.md` を更新 — Set を「pure-Julia `Dict{T,Nothing}` ラッパー
  (#6721/#6732)」、Dict を「struct dispatch、`Value::Dict` キャリア撤去 (#6731)」、Array を
  「`Memory{T}` 上の wrapper、`Value::Array` キャリア撤去 (#4568)」へ。`Value::Set` の実ハンドラは
  **0サイト** であることをコードで確認。`RUST_BOUNDARY_JUSTIFICATION.md` に #6726–#6733 + #4568 を
  **全 CLOSED** と明記し、本ドキュメントを相互参照に追加。

---

## 5. 検証コマンド

```bash
# 上流 C 境界の関数数 (67)
awk '/#define JL_BUILTIN_FUNCTIONS/,/[^\\]$/' julia/src/builtin_proto.h \
  | grep -oE 'XX\([a-z_!]+' | wc -l

# sjulia Layer-2 Rust builtin 総行
wc -l subset_julia_vm/src/builtins.rs subset_julia_vm/src/vm/builtins*.rs | tail -1

# 撤去済みキャリアが live コードに無いこと (コメントのみのはず)
grep -rn "Value::Dict\|Value::Set" subset_julia_vm/src/vm | grep -v '//' | grep -v retired

# Complex interleaved 特殊化サイトの規模
grep -rln "interleav\|complex" subset_julia_vm/src/vm | wc -l

# Pure Julia 側の ccall 境界 (少数のはず)
grep -rc "ccall" subset_julia_vm/src/julia | grep -v ':0'
```

---

## 6. 参照

- 本家境界: `julia/src/builtins.c` (67関数), `julia/src/intrinsics.h` (94命令), `julia/base/` (Pure Julia), `julia/Compiler/` (型推論)
- sjulia 境界: `subset_julia_vm/src/intrinsics.rs`, `subset_julia_vm/src/vm/builtins_*.rs`, `subset_julia_vm/src/julia/`
- 設計方針: `docs/vm/PURE_JULIA_DESIGN.md`, `docs/vm/RUST_BOUNDARY_JUSTIFICATION.md`, `docs/vm/BUILTIN_REMOVAL.md`
- 移行 Issue (全 CLOSED): #6726, #6727, #6728, #6729, #6730, #6731, #6732, #6733
- キャリア撤去: #6731 (Value::Dict), #6732 (Value::Set), #4568/#6807 (Value::Array)
- 本ドキュメント由来の改善 Issue: #7875 (P1), #7876 (P2), #7877 (P3), #7878 (P4), #7879 (P5)
