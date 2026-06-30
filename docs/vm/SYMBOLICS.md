# Symbolics サブセット (Issue #6572)

sjulia 上で記号計算 (Symbolics.jl / SymbolicUtils.jl) の **中核セット** を動かすための
バンドルパッケージ `Symbolics` の設計・対応状況・既知の制限をまとめる。

参照: 上流ソースは `extern/Symbolics.jl/`・`extern/SymbolicUtils.jl/`。

## 方針

本家 Symbolics/SymbolicUtils は Moshi `@data`・hashconsing・型ベースの大量メタプロで構成され、
no-JIT の subset VM にそのまま移植するのは非現実的。よって **本家の型分類
(`Num <: Real` が `Sym`/`Term`/`AddMul`/`Div` をラップ) を簡約した忠実なサブセット** を
Pure Julia で実装する。

- **機能スコープ = 中核セット**: `@variables` / `Num`・`Sym`・`Term` 型 / 四則・冪・初等関数
  (sin, cos, exp, log, sqrt, tan) / `show` 表示 / `substitute` / 基本 `simplify`・`expand` /
  `Differential` + `derivative`(微分)。
- **表示 = テキストのみ**: 既存 REPL の `show` 出力をそのまま使う。
- `solve` や高度な simplify は後続フェーズ。

## 型分類 (本家 → サブセット)

| 本家 (`SymbolicUtils`) | サブセット (`Symbolics` パッケージ) |
|---|---|
| `BasicSymbolic{T}` uni-type (7 variant) + hashconsing | `Sym` / `Term` の 2 構造体 |
| `BSImpl.Sym{...}`(name, shape, type, metadata, hash...) | `struct Sym; name; end` |
| `BSImpl.Term{...}`(f, args, ...) | `struct Term; op::Symbol; args::Vector{Any}; end` |
| `BSImpl.AddMul`(coeff + dict 正準形) / `BSImpl.Div` | `Term(:+/:*/:^/:/, args)`(正準化は浅い) |
| `Num <: Real`(`val::BasicSymbolic`) | `struct Num <: Real; val; end`(val = Real/Sym/Term) |
| `unwrap(x::Num) = x.val` / `value` | 同じ |

`Num <: Real` により記号変数が実数と自然に混ざる(本家と同じ)。

## 対応関数 (現状)

| 機能 | 状態 | 備考 |
|---|---|---|
| `@variables x y...` | ✅ | caller 束縛 + `Vector` 返却 |
| `Num` / `Sym` / `Term` / `unwrap` / `value` | ✅ | `types.jl` |
| `operation` / `arguments` / `iscall` / `issym` / `isterm` | ✅ | TermInterface 風アクセサ |
| 四則 `+ - * /`・冪 `^`・単項 `-` | ✅ | `arithmetic.jl`(混合型メソッド網羅) |
| 初等関数 sin/cos/exp/log/sqrt/tan | ✅ | `arithmetic.jl` |
| `==` / `isequal` / `zero` / `one` / `iszero` / `isone` / `hash` | ✅ | 構造的 Bool 比較・構造的ハッシュ |
| `show`(中置プリント) | ✅ | `string`/`print`/`println`/`show(io,·)` + REPL/iOS-Web 結果エコー(#7168 解決) |
| `substitute(expr, dict)` / `substitute(expr, pair)` | ✅ | 数値置換は畳み込み、部分置換は記号のまま |
| `simplify` | ✅ | 同類項/同因子結合・定数畳み込み・正準順序 |
| `expand` | ✅ | 積/小整数冪の分配 → simplify |
| `derivative(expr, var)` | ✅ | 和/積/商/冪/連鎖律・初等関数。eager(非簡約) |
| `Differential(x)` | ✅ | クロージャを返す(`Differential(x)(expr)`)。型ではない |
| `expand_derivatives` | ✅ | eager のため恒等(API 互換用) |

## 実装メモ / 既知の制限

- **promote-fallback 再帰トラップ (Issue #5966 / [PROMOTION.md](./PROMOTION.md))**: `Num <: Real` の
  算術はこのトラップの危険地帯。同型 (`Num⊗Num`) だけでなく **混合型 (`Num⊗Real` / `Real⊗Num`)
  のメソッドを各演算子・`==`・`isequal` に必ず定義** し、どのペアも generic promote fallback に
  落とさない(`arithmetic.jl`)。各メソッドは `unwrap` で plain `Real`/`Sym`/`Term` に剥がして
  正規化 → `Num` 再ラップするため `Num` が `_mk*` に再入せず fallback に到達しない。`^(::Num,
  ::Integer)` は Base `^(::Number, ::Integer)` との曖昧性解消のため別途定義(上流 num.jl 同様)。
- **正規化は浅い**: `_mkadd`/`_mkmul`/`_mkpow` 等は定数畳み込み + 0/1 恒等のみ。`x - x` は
  `Term(:-, [x, x])` のまま(0 に畳まない)。同類項集約・分配は `simplify`/`expand`(後続)に委ねる。
- **`==` の意味的乖離**: 上流 `==` は自由変数上で記号式を返すが、本サブセットは**構造的 `Bool`**
  を返す(`substitute`/微分のパリティ検査が `==`/`isequal` で行えるようにするため)。`isequal` は
  順序依存の浅い構造比較(`x+y` と `y+x` は非等価)。
- **`Term` フィールドアクセスの衝突 (Issue #7162 と同根の field 衝突)**: 動的(`Any`)型の値への
  `t.args` は builtin `Expr.args` アクセサに誤ルートし `GetExprField: expected Expr, got
  StructRef` で落ちる。→ `Term` の検査は必ずアクセサ `operation(t)` / `arguments(t)`(いずれも
  `::Term` ディスパッチで型を確定させる)経由で行う。`.op` は非衝突なので単独では動くが、一貫して
  アクセサを使う。
- **`simplify` / `expand`(`simplify.jl`)**: 単一ボトムアップパスで `+`/`*` を flatten・定数畳み込み・
  同類項(`x+x→2x`, `2x+3x→5x`)/同因子(`x*x→x^2`)結合を行う。可換オペランドを**正準順序**(構造的
  キー `_canonkey` でソート)に並べるため `x*y+y*x→2*x*y`、`(x+y)^2→x^2+2*x*y+y^2` まで結合できる。
  `expand` は積/小整数冪(≤8)を分配してから `simplify`。本家のルールベース不動点
  (`simplify_rules.jl`)の縮約版で、三角恒等式等の高度な規則は未対応。
  - **検証は substitute 評価で**: 出力は正準ソートされ構築順と異なるため、`==`(順序依存の構造比較)では
    なく `substitute` で数値評価して等価性を確認する(fixture もこの方針)。
- **微分(`diff.jl`)**: `derivative(expr, var)` は **eager**(`Differential(x)(expr)` が即座に微分を計算)。
  和/積/商/冪(x 非依存指数)/連鎖律と初等関数(sin/cos/tan/exp/log/sqrt)の導関数表。結果は浅い
  `_mk*` 正規化のみ(上流 `derivative(...; simplify=false)` 既定と同じ非簡約)なので、2 階微分など
  collection が要る場合は `simplify(derivative(...))` を使う。検証は `==`(clean)/`isequal`/substitute 評価。
  - **`Differential(x)` は型ではなくクロージャを返す**: モジュール内で struct call operator
    `(D::T)(args)` がディスパッチされない(**Issue #7185**)ため。クロージャはモジュールヘルパを名前
    参照できない(#7180)ので、実作業は通常関数 `_apply_diff` に閉じ込め、それをローカルに捕捉して
    クロージャを構成する。`Differential(x)(expr)` と `D = Differential(x); D(expr)` は動くが
    `D isa Differential` のような型検査は不可。
  - **x 依存指数(`2^x`, `x^x`)の微分は未対応**: 一般冪則(`log` を含むネスト式)が VM のロード時
    コンパイルをハングさせた(**Issue #7186**)ため除外。該当時は `error` を投げる。
  - **`_deriv(node, x)::Any` の戻り値型注釈はコンパイル速度のために必須**(**Issue #7215**): `_deriv` は
    相互再帰 family(`_deriv ⇄ _deriv_*`)のハブで、注釈が無いと抽象解釈エンジンが各呼び出し点でボディを
    再展開し、`Differential(x)(cos(x))` の初回コンパイルが ~7–17 秒かかっていた。宣言済み戻り値型は
    呼び出し側推論を短絡させる(コンパイラ側の対応も #7215)。`::Any` は `_deriv` の戻り(`0`/`1`/`Num`/`Term`)
    の正確な上界で `convert` を伴わず、`_apply_diff = Num(_deriv(…))` は `Num` 精度を保つ。
- **subset-VM のモジュールスコープ制約(実装上の重要ノート)**: モジュール内の private ヘルパ
  (`_structeq`/`_mkmul`/`_canonkey` 等)を **Base の HOF に関数値として渡す**と
  (`findfirst(x->_structeq(x,b), v)` / `reduce(_mkmul, v)` / `sort(v; by=_canonkey)`)、
  `function '_structeq' is not imported` で解決に失敗する。→ `simplify.jl` では HOF を使わず**明示ループ**
  (`_findbase`/`_foldmul`/`_foldadd`/`_sortbykey`+`sortperm`)で実装。直接呼び出しは別ファイルの
  ヘルパでも解決される。
- **`substitute` は Dict を反復**(`for (k,v) in dict`)し、`dict[key]` インデックスは使わない。
  `Num <: Real` を `Dict` のキーにできる(構造的 `hash`/`isequal` を定義済み): `d = Dict(x=>3); d[x]`
  は動く。ただし**インライン連鎖** `Dict(x=>3)[x]` は呼び出し側で Dict 型が推論されず数値 getindex に
  誤ルートして落ちる(**Issue #7173**)。→ Dict は一度変数に束縛してからインデックスする。
- **`@variables` マクロ構築の subset-VM 制約**:
  - マクロ内で `Expr(head, args...)` へ Vector をスプラットすると展開結果が壊れる
    (`macro expansion returned unsupported value type Any`、**Issue #7162**)。→ `:block` /
    `:vect` を `push!` で 1 要素ずつ構築して回避。
  - マクロが注入した `QuoteNode` の値は実行時に `Any` 型として箱詰めされ、`::Symbol` 注釈
    フィールドへの代入が `Cannot convert Any to Symbol` で失敗する(**Issue #7163**)。→ `Sym.name`
    を未型付けにして回避(格納値は実 `Symbol`、`Sym(:x).name === :x`)。
- 本家 `value(x)` は `BasicSymbolic` を返すが、サブセットの `value`/`unwrap` は `Num` を剥がして
  `Real`/`Sym`/`Term` を返す。内部表現が異なるため、本家との突合は**等価性(数値結果・`isequal`)**で
  行い、表示文字列は緩く扱う。
- **`show`(中置プリント、`show.jl`)**: 演算子優先順位でカッコ付け(左結合は右オペランド、`^` は左
  オペランドを同優先順位でカッコ化)。`+`/`-` は前後空白、`*`/`/`/`^` は空白なし。表示は緩い
  サブセット形式(正準順序なし、`2x` ではなく `2*x`)。`Num` は透過(中身を表示)。
  - 動作経路: `string`/`print`/`println`/`show(io, ·)` は VM の `user_show_method_for` を経由して
    本 `show` を使う。**bare REPL エコー / iOS・Web の結果表示パネル**も user `show` を経由するよう
    修正済み(**Issue #7168 解決**)。`REPLSession::eval` が結果を eval 時に `render_value_via_user_show`
    (`run_until_frame_return` で show を実行)で描画して `REPLResult.value_display` に載せ、CLI/FFI の
    フォーマッタがそれを優先する。`ex = x^2+2x+1` は REPL で `x^2 + 2*x + 1` と表示される。
  - 除外: Complex/Rational/LinRange/array-wrapper は専用 Rust フォーマッタ(上流正準形)を維持し
    `value_display` を作らない(LinRange の `show` は `a:step:b` でなく struct 形のため)。`repr` は
    別の Rust builtin 経路で今回未対応(struct dump のまま)。

## ファイル構成

```
subset_julia_vm/packages/Symbolics/
├── Project.toml
└── src/
    ├── Symbolics.jl   # module + include + export
    ├── types.jl       # Sym, Term, Num, unwrap, value, operation/arguments/iscall
    ├── arithmetic.jl  # 演算子オーバーロード・初等関数・正規化コンストラクタ・==/isequal
    ├── show.jl        # 中置プリティプリント(優先順位カッコ付け)
    ├── substitute.jl  # substitute(置換 + 再正規化)
    ├── simplify.jl    # simplify / expand(同類項結合・分配・正準順序)
    ├── diff.jl        # Differential / derivative / expand_derivatives(eager 微分)
    └── variables.jl   # @variables マクロ
```

登録は `subset_julia_vm/src/julia/packages/mod.rs` のみ(`get_bundled_package` /
`get_package_include` / `bundled_package_names`)。ランタイム解決は `loader.rs` +
`lowering/include/mod.rs` が自動処理。

## テスト

- fixture: `subset_julia_vm/tests/fixtures/packages/symbolics_*.jl`(manifest 登録、名前は
  `symbolics_` 接頭 / カテゴリ接頭 `packages_`)。
- unit: `subset_julia_vm/src/julia/packages/mod.rs` の `test_symbolics_*`。
