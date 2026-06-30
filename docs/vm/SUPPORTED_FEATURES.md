# SUPPORTED_FEATURES.md

**最終更新**: 2026-06-10

SubsetJuliaVM（`subset_julia_vm`）が **現時点でサポートしている Julia 機能**を、コードベース（特に `docs/vm/DONE.md`, `docs/vm/STATUS.md`, `subset_julia_vm/tests/fixtures/`）に基づいて整理したドキュメントです。

- **未実装/非対応の一覧**: `docs/vm/UNIMPLEMENTED.md`
- **実装の履歴ログ（追加機能の詳細）**: `docs/vm/STATUS.md`
- **実装済みの正規一覧（DONE）**: `docs/vm/DONE.md`

> 注意: ここでの「サポート」は **SubsetJuliaVM 上で動作し、テストで検証されている**ことを指します（互換性は「Julia 本家と同じ結果を返す」ことが目標）。ただし一部は **簡略化/部分実装** で、制限があります（各セクションに明記）。

---

## 対象プラットフォームと差分

- **Native (macOS/Linux/Windows)**: 最も機能が揃っている実行環境。`include()` 等のファイル読み込み系は主にここ向け。
- **iOS**: App Store 制約（JIT 禁止など）に合わせた AoT/VM 形態。外部プロセス等は非対象。
- **WASM/Web**: `subset_julia_vm_web` で実行。Web 側は API で実行・補完を提供。
  - `subset_julia_vm_web/src/lib.rs` の `get_supported_features()` は **概要の短いリスト**であり、詳細は本書の内容が正です。

---

## パイプライン（全体アーキテクチャ）

- **Pure Rust パーサー**（`subset_julia_vm_parser`）による Julia 構文解析
  - WASM/Native で同一コードパス
  - `where` 節、マクロ、アロー関数、修飾演算子名、juxtaposition（`2x`, `3.0im`）等も解析可能
- **Lowering**（CST → Core IR）
  - 対応可否（UnsupportedFeature）は lowering で判定し、**span + hint** を含むエラーを返す
  - マクロ展開は主に **コンパイル時（lowering 時）**に行われる
- **Compiler**（Core IR → Bytecode）
  - 多重ディスパッチ、パラメトリック型、型推論（v2 統合）を含む
  - Peephole 最適化と Base キャッシュ統合
- **VM**（スタックベース VM）
  - 例外、HOF（map/filter/reduce 等）、ブロードキャスト、配列（型付き）などを実行
- **AoT / persisted formats**
  - `.sjir` Core IR ファイルの保存/読み込み
  - `.sjvmbc` VM bytecode ファイルの保存/読み込み
  - Core IR から AoT（Rust コード生成）までのツールチェーン

---

## 言語構文（Julia コア構文）

### 制御フロー

- **条件分岐**: `if / elseif / else`
- **ループ**:
  - `for`（`start:stop`, `start:step:stop`）
  - `for x in iterable`（配列/タプル/文字列/Range/Dict/Set 等の iterate プロトコル）
  - `while ... end`
  - `break`, `continue`, `return`
  - `for` での **タプル分解**: `for (i, x) in pairs(arr)` 等
- **低レベル制御フロー**:
  - `@label name`, `@goto name`
  - 関数内のみ（関数境界をまたぐジャンプは不可）、未定義ラベルはコンパイルエラー
- **例外処理**:
  - `try/catch/finally`
  - `catch e` で例外値を受け取り、フィールドアクセスが可能（例外が構造体の場合）
  - `try/finally` 内で `return` しても finally が実行される
- **短絡評価**:
  - `&&`, `||` の短絡
  - これらを使った `return`/`break`/`continue` パターンの実行
- **三項演算子**: `a ? b : c`
- **let ブロック**: `let ... end`

### 関数

- **定義**:
  - 通常定義: `function f(x, y) ... end`
  - ショート定義: `f(x) = expr`
  - 無名関数（ラムダ/アロー）: `x -> x^2`, `(x, y) -> x + y`
  - 再帰
  - do 構文（HOF 呼び出し）: `map(arr) do x ... end`
- **引数**:
  - **キーワード引数**: `f(; x=1, y=2)`
  - **可変長引数（Varargs）**: `f(args...)`, `f(a, b, rest...)`
  - **スプラット呼び出し**: `f(args...)`（配列/タプルの展開）
  - **キーワード引数スラープ**: `f(; kwargs...)`（`Base.Pairs` として受け取る）
- **戻り値型注釈**:
  - `f(x)::T = ...` をサポート
  - 返り値に対し `convert(T, value)` 相当の変換が適用される（互換性のため）
- **第一級関数（First-class functions）**:
  - 関数を引数として渡し、複数引数で呼び出し可能
  - `caller(f, x, y) = f(x, y)` のパターンをサポート
  - `mergewith`, `mergewith!` 等の高階関数が利用可能に

### モジュール

- **定義**: `module Name ... end`
- **baremodule**: `baremodule Name ... end`（現状は `module` とほぼ同等のセマンティクスで動作）
- **import/using/export**:
  - `using Module`, `import Module`
  - `using Module: f`, `import Module: f`
  - `Module.f` のモジュール修飾呼び出し
  - `S = Statistics; S.mean(...)` のようなモジュール別名
- **stdlib モジュール**（後述）:
  - `Statistics`, `Random`, `Dates`, `Test`, `LinearAlgebra`（部分）
  - `Iterators`, `Printf`, `Broadcast`, `InteractiveUtils`

### リテラル/基本式

- **数値リテラル**:
  - 10 進整数/浮動小数点
  - 16 進/2 進/8 進整数（アンダースコア区切り対応）
  - Float32 リテラル（`1.0f0`, `1f2` 等）
  - 16 進浮動小数点（`0x1.8p3` 等）
  - 大きな整数リテラル（Int128/BigInt への自動昇格）
- **文字/文字列**:
  - 文字: `Char`
  - 文字列: `String`
  - 文字列補間: `"x = $(x)"`
  - `raw"..."`（raw 文字列）
  - `r"..."`（正規表現リテラル、フラグ `i/m/s/x`）
  - `v"1.2.3"`（VersionNumber リテラル）
  - `b"data"`（バイト列リテラル → `Vector{Int64}` として生成）
- **juxtaposition**:
  - `2x`（暗黙の乗算）
  - `3.0im`（複素数の虚数単位）
- **Range**:
  - `1:10`, `1:2:10`
  - `OneTo(n)` / `oneto(n)`
- **配列/タプル**:
  - 配列: `[1,2,3]`, 行列: `[1 2; 3 4]`
  - 空配列: `[]`、型付き空配列: `Int64[]`, `Float64[]`, `String[]` 等
  - タプル: `(1,2,3)`、分解代入: `a, b = (1,2)`
  - NamedTuple: `(x=1, y=2)`、フィールドアクセス: `nt.x`
- **内包表記/ジェネレータ**:
  - 配列内包表記（フィルタ付き）
  - Generator（遅延評価）
  - `Dict(...)` / `Set(...)` の内包表記（単一 generator + フィルタ付きの代表ケース）
  - 注意: 多次元内包表記は `UNIMPLEMENTED.md` の「内包表記・ジェネレータ」参照

---

## マクロ・メタプログラミング

### マクロシステム（コンパイル時展開 + hygiene）

- **ユーザー定義マクロ**:
  - `macro name(args) ... end`
  - 可変長マクロ引数 `macro f(p...)`
- **quote / 補間**:
  - `quote ... end`（式として利用可）
  - 式補間: `$expr`
  - ランタイムのスプラット補間: `:(f($(args...)))`
- **hygiene**:
  - ローカル変数の衝突回避（2 パス: 収集 → リネーム適用）
  - `esc()` による hygiene escape
  - `local` in quote をサポート
- **Base/stdlib マクロのロード**
  - user → Base → stdlib の 3 層レジストリで解決
  - `using Test` 等で stdlib マクロを早期ロード

### 実装済みの代表的マクロ/機能

- **テスト**: `@test`, `@testset`, `@test_throws`（`using Test`）
- **タイミング/割り当て**: `@time`, `@elapsed`, `@timed`, `@timev`, `@showtime`, `@allocated`, `@allocations`
- **デバッグ**: `@show`, `@assert`
- **ロギング**: `@debug`, `@info`, `@warn`, `@error`（メッセージ + 最大3つの `key=value`、logger/filter なし）
- **互換マクロ（no-op）**: `@inline`, `@noinline`, `@inbounds`, `@boundscheck`, `@propagate_inbounds`, `Base.@nospecializeinfer`（関数定義 wrapper）
- **簡易互換**: `@eval`, `@deprecate`（`@eval` は通常展開、`@deprecate` は警告なし）
- **その他**: `@something`, `@coalesce`, `@evalpoly`
- **位置情報**: `@__LINE__`, `@__FILE__`, `@__MODULE__`（`@__DIR__` は環境依存）
- **@kwdef**
  - `@kwdef struct ... end` でキーワード引数コンストラクタを生成（lowering 実装）
- **@static**
  - コンパイル時条件評価マクロ
  - `@static if cond ... else ... end` または `@static cond ? a : b`
  - サポートされる条件: `true`, `false`, `Sys.isapple()`, `Sys.isunix()`, `Sys.iswindows()`, `Sys.islinux()`, `Sys.isbsd()`
- **@enum**
  - 列挙型定義マクロ
  - `@enum TypeName member1 member2 ...`（0 から自動インクリメント）
  - `@enum TypeName member1=1 member2=5`（明示的な値）
  - 型システム: `JuliaType::Enum`, `Value::Enum` をサポート
- **@generated**
  - Phase 1: `if @generated ... else fallback end` のフォールバック
  - Phase 2: `Val{N}` から N を抽出して実行時に利用
  - Phase 3: 単純な quote を "アンクォート" して直接実行
    - `return :(x + y)` → `return x + y`
    - 複数文ブロックの展開
    - `:(sin(x))`, `:(abs(sin(x)))` 等の関数呼び出し
    - begin/end ブロック、三項演算子のサポート
  - full `@generated function ... end` 構文: 型/値パラメータを直接返す代表ケース
    - 注意: SubsetJuliaVM 固有機能（Julia 本家とは異なる動作）

### Expr/QuoteNode/LineNumberNode/GlobalRef と eval

- `Expr(:head, args...)` による AST 構築
- `QuoteNode(value)` と `qn.value`
- `LineNumberNode(line)` / `LineNumberNode(line, file)` と `.line` / `.file`
- `GlobalRef(mod, name)` と `.mod` / `.name`
- `eval(expr)` と `eval(mod, expr)`（モジュール引数は現在 Main を想定）
- `macroexpand` / `macroexpand!`（実行時は展開済みのため実質 no-op 互換）

---

## 型システム・ディスパッチ

### 型階層と基本型

- `Any, Number, Real, Integer, AbstractFloat` 等の階層
- 代表的な具象型:
  - 整数: `Int8..Int128`, `UInt8..UInt128`, `Int64`（中心）
  - 浮動小数: `Float32`, `Float64`
  - 任意精度: `BigInt`, `BigFloat`
  - `Bool`, `Char`, `String`
  - `Complex{T}`（Pure Julia 実装）
  - `Rational{T}`（Pure Julia 実装）
  - コレクション: `Array`, `Tuple`, `NamedTuple`, `Dict`, `Set`, `Range` 系
  - `Module`（モジュールを値として扱える）
- `Union{...}` をサポート（`Union{}` Bottom も含む）

### 多重ディスパッチ（Multiple dispatch）

- 型アノテーションに基づくメソッド選択
- パラメトリック型:
  - `struct Point{T} ... end`
  - `f(x::MyStruct{T}) where T` などの where 型変数
- `Type{T}` dispatch: `f(::Type{T}) where T`
- ランタイムディスパッチ（Any を含む二項演算など）

### 型関連の組み込み/ユーティリティ

- `typeof`, `isa`, `<:`（サブタイプ式の評価）
- `convert`, `promote`, `promote_type`, `promote_rule`（Julia 準拠を目標）
- 反射/イントロスペクション（実装済み範囲）:
  - `nameof`, `nfields`, `fieldnames`, `fieldcount`
  - `fieldtype(T, i)`, `fieldtype(T, name::Symbol)`, `fieldtypes`
  - `fieldindex(T, name::Symbol)`, `fieldindex(T, name, err)`
  - `methods`, `hasmethod`, `which`
  - `ispublic`, `isexported`（内部関数、Base から export されない）
- プロパティ/フィールドアクセス:
  - `getproperty(x, s::Symbol)` - プロパティ値取得
  - `setproperty!(x, s::Symbol, v)` - プロパティ値設定
  - `propertynames(x)` - プロパティ名一覧
  - `hasproperty(x, s::Symbol)` - プロパティ存在確認
  - `getfield(x, name)` / `setfield!(x, name, v)` - フィールド操作（builtin）
- 型トレイト（Julia `base/traits.jl` 由来の一部、内部使用のみ）:
  - `OrderStyle`（Ordered/Unordered）
  - `ArithmeticStyle`（Rounds/Wraps/Unknown）
  - `RangeStepStyle`（Regular/Irregular）
  - `IndexStyle`（IndexLinear/IndexCartesian）
  - 注意: これらは Base から export されておらず、内部実装用
- 例外型（多数実装・export）: `ArgumentError`, `DomainError`, `MethodError`, `UndefVarError`, `LoadError` 等

### 型推論（Type Inference v2）

- ラティスベース（`LatticeType`: Bottom/Concrete/Union/Conditional/Top）
- 抽象解釈（不動点反復）
- 主要な転送関数（算術・配列・文字列・intrinsics）
- fixture での型推論テストカテゴリ（`tests/fixtures/type_inference/`）

---

## データ構造と配列（Array/Typed Array/Broadcast）

### 配列（1D/2D）と型付き配列

- 1D/2D 配列の作成と基本操作
- 型付き配列ストレージ（`ArrayData` + `ArrayElementType`）により以下を効率的に扱う:
  - 数値（I*/U*/F32/F64）、Bool、Char、String
  - Tuple 配列（`TupleOf`）
  - isbits 構造体配列の AoS 形式インライン格納（`StructInlineOf`）
- 線形インデックス（多次元配列に `A[i]`、column-major）
- N 次元スライス:
  - 1D: `arr[1:3]`, `arr[:]`
  - 2D: `mat[:, :]`, `mat[1:2, :]`
  - 3D+: `arr[1:5, 2:4, :]` 等の任意次元スライス
  - 型保存: スライス操作時に要素型が保存される
  - インデックスの `begin` / `end`（lowering で `firstindex`/`lastindex` へ変換）
- `getindex` / `setindex!` が Julia 流にディスパッチされる（Array/String/Tuple/Dict など）
- 論理インデックス:
  - `arr[arr .> 0]`
  - `arr[[true,false,true]]`

### SubArray / view（代表サブセット）

- `SubArray` と `view(A, ...)`（1D `Vector` range view、range / `OneTo` view、2D matrix range/colon/dimension-dropping view、3D range view）
- `@view` / `@views`（スライスを `view` 呼び出しへ変換）
- 代表ケースでは `getindex` / `setindex!` / `collect` / `map` / broadcast / `sum` が親配列への aliasing を保つ
- 制限: 本家 Julia の全 SubArray index combination を網羅するものではなく、上記代表ケースを fixture で固定

### ブロードキャスト（Broadcast）

- ドット演算子:
  - `.+, .-, .*, ./, .^`
  - `.<, .>, .<=, .>=, .==, .!=`
  - `.& , .| , .!`
  - `.=` と複合代入（`.+=` など）
- `broadcast(f, A, B)` / `broadcast!(f, dest, A, B)`（ユーザー定義関数も対象）
- タプルブロードキャスト:
  - `(1,2,3) .+ (4,5,6)` など
  - 配列とタプル混在時は配列へフォールバックする挙動を含む

---

## イテレーション（iterate プロトコル）

- `iterate(obj)` / `iterate(obj, state)` をユーザー型で定義可能
- `IterateDynamic` により `Any` 型のランタイム iterate ディスパッチも可能
- `collect(iterable)`（Pure Julia 実装）
- 実装済みの代表的イテレータ/ユーティリティ（Pure Julia）:
  - `enumerate`, `zip`
  - `take`, `drop`
  - `countfrom`（無限カウント）
  - `eachcol`, `eachrow`
  - `skipmissing`
  - `peel` と `Rest`

---

## 数値・演算子・数学関数

### 演算子

- 四則/剰余/冪: `+ - * / % ^`
- 有理数: `//`（`Rational{T}`）
- 比較: `< > <= >= == !=`
- 同一性/等価性:
  - `===` / `≡`
  - `!==` / `≢`
  - `isequal`（NaN/±0.0 を考慮）
- 連鎖比較（lowering で `&&` 連結へ展開）:
  - `1 <= x <= 10` など任意長
- 関数合成: `∘`（`ComposedFunction` と実行サポート）
- Unicode 数学演算子:
  - `√`, `∛`, `∜`
  - `≈`, `≉`

### 数学関数（代表）

Rust builtins と Pure Julia 実装を組み合わせ、以下を中心にサポート:

- 三角/逆三角: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`（ほか派生も多数）
- 双曲線/逆双曲線: `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`
- 指数/対数: `exp`, `exp2`, `exp10`, `expm1`, `log`, `log2`, `log10`, `log1p`
- ルート: `sqrt`, `cbrt`, `fourthroot`
  - `sqrt` は負の実数引数で `DomainError`（Julia 互換）
- 丸め/絶対値: `floor`, `ceil`, `round`, `trunc`, `abs`
- ビット操作:
  - `count_ones`, `count_zeros`, `leading_ones`, `leading_zeros`, `trailing_ones`, `trailing_zeros`
  - `bitreverse`, `bitrotate`, `bswap`
- 浮動小数点ユーティリティ:
  - `nextfloat`, `prevfloat`
  - `frexp`, `exponent`, `significand`, `issubnormal`, `maxintfloat`
- `fma`, `muladd`
- `gcd`, `lcm`（BigInt 対応含む）

### Complex / Rational / BigInt / BigFloat

- **Complex**:
  - `Complex{T}` を Pure Julia で実装（`im`, `real`, `imag`, `conj`, `abs2` 等）
  - `cis`, `cispi`, `reim`
  - `Complex{Float32}` / `Complex{Int64}` などの generic dispatch、Real/Complex 混在演算・比較
  - 複素数配列ブロードキャスト（代表的ケース）
  - 複素数配列の行列-ベクトル乗算: `A * v` where v is `Vector{ComplexF64}`
- **Rational**:
  - `Rational{T<:Integer}`、正規化、算術/比較、`numerator`/`denominator`
  - `Rational` と `Float64` の混合乗算
- **BigInt**:
  - リテラル/変換 `big`、BigInt-Int64/Int128 混合比較と演算
  - `gcd`/`lcm`、`factorial(::BigInt)` など
- **BigFloat**:
  - `precision`, `setprecision`
  - `rounding`, `setrounding`（複数丸めモード）

---

## コレクション（Dict / Set）と関連 API

### Dict

- `Dict("a" => 1)` と基本操作
- `d[key]` / `d[key] = value`
- `haskey`, `get`, `keys`, `values`, `pairs`, `merge`, `merge!`, `get!`
- 反復（iteration）
- Dict 内包表記（`Dict(k => v for ...)`、フィルタ付き代表ケースを含む）

### Set

- `Set([1,2,3])`
- `union`, `intersect`, `setdiff`, `symdiff`
- `issubset`（Set/Array 両対応）
- `push!`, `delete!`
- `issetequal`（集合として同値判定）
- Set 内包表記（`Set(x for ...)`、フィルタ付き代表ケースを含む）

---

## 文字列（String）と正規表現

### 文字列操作（抜粋）

Pure Julia と Rust builtin の併用で、以下を中心にサポート:

- 基本: `string`, `repr`
- 大文字/小文字: `uppercase`, `lowercase`, `titlecase`, `lowercasefirst`, `uppercasefirst`
- トリム/切り落とし: `strip`, `lstrip`, `rstrip`, `chomp`, `chop`
- パディング: `lpad`, `rpad`
- 検索/分割: `findnext`, `findprev`, `split`, `rsplit`, `join`, `contains`, `startswith`, `endswith`
- 文字列インデックス: `nextind`, `prevind`, `thisind`, `reverseind`, `isvalid`
- エスケープ: `escape_string`, `unescape_string`
- バイト列: `codeunits`, `bytes2hex`, `hex2bytes`
- バイナリ表現: `bitstring`
- `tryparse(Int|Float, s)`（失敗時 `nothing`）

### 正規表現（Regex）

- `Regex` / `RegexMatch` 型
- `r"pattern"` リテラル
- `match`, `occursin`, `eachmatch`
- フラグ: `i`（ignorecase）, `m`（multiline）, `s`（dotall）, `x`（extended）

---

## I/O・表示・パス/ファイルシステム（サブセット）

### 基本 I/O

- `print`, `println`
- `IOBuffer()` と `write(io, x)`, `take!(io)`, `takestring!(io)`
- `sprint(x)` / `sprint(f, args...)`
  - user-defined `f(io, args...)` を VM がサポート（専用命令あり）
  - `context` 付き sprint（`IOContext`）をサポート
- `@printf`, `@sprintf`
- `printstyled`（ANSI カラーのサブセット）

### IOContext（context-aware printing）

- `IOContext` 型（`iocontext(io, ...)` で生成するスタイル）
- `ioget`, `iohaskey`, `iokeys`
- `:compact` などのプロパティを使った出力調整（例: `sprint(...; context=:compact=>true)`）

### 表示（display / マルチメディア I/O）

- **MIME 型**:
  - `MIME"text/plain"` / `MIME("text/plain")` リテラルとコンストラクタ
  - `@MIME_str` マクロ（`MIME"..."` 構文）
  - `istextmime` - テキスト MIME 判定
- **表示関数**:
  - `show(io, mime, x)`（未定義の場合は `show(io, x)` へフォールバック）
  - `display(x)` - stdout へ表示
  - `displayable(mime)` - テキスト MIME は true
  - `showable(mime, x)` - text/plain は true
  - `redisplay(x)` - display へ委譲
- **ディスプレイスタック**:
  - `AbstractDisplay`, `TextDisplay` 型
  - `pushdisplay`, `popdisplay` - スタブ実装
- 制限: ディスプレイスタックの完全なバックエンド選択機能は未完成

### パス操作（Base の一部）

- `basename`, `dirname`, `joinpath`, `splitdir`, `splitext`, `splitpath`
- `isabspath`, `isdirpath`

### ファイル/ディレクトリ（Rust builtins の一部）

- `pwd`, `readdir`
- `mkdir`, `mkpath`, `rm`, `touch`, `cd`, `tempdir`, `tempname`, `islink`
- `isfile`, `isdir`, `ispath`, `filesize`, `mtime`
- `read(filename, String)`, `readlines(filename)`, `readline(filename)`, `countlines(filename)`
- `cp`, `mv`

### ファイルハンドル

- `open(filename)` / `open(filename, mode)`（`r`, `r+`, `w`, `w+`, `a`, `a+`）
- `close(io)`, `isopen(io)`
- `readline(io)`（ファイル IO のみ、その他 IO は未対応）

> 制限: 本家 Julia の I/O/ファイルシステム全体を網羅しているわけではありません（詳細は `UNIMPLEMENTED.md`）。

---

## Base exports（関数/型/定数）

`subset_julia_vm/src/julia/base/exports.jl` を参照して、**Base から export されている公開シンボル**を整理。

- **対象**: 関数・演算子・マクロ（関数相当）・型・定数
- **除外**: 内部用シンボル
- **完全一覧**: `subset_julia_vm/src/julia/base/exports.jl`

### Types
- `AbstractArray`, `AbstractChar`, `AbstractDict`, `AbstractDisplay`, `HTML`, `MIME`, `AbstractFloat`, `AbstractIrrational`, `AbstractMatrix`, `AbstractRange`, `AbstractSet`, `AbstractString`, `AbstractUnitRange`, `AbstractVector`, `Any`, `ArgumentError`, `AssertionError`, `BigFloat`, `BigInt`, `Bool`, `BoundsError`, `CanonicalIndexError`, `CapturedException`, `CartesianIndex`, `CartesianIndices`, `Char`, `Complex`, `ComplexF64`, `CompositeException`, `DenseArray`, `DenseMatrix`, `DenseVector`, `Dict`, `DimensionMismatch`, `DivideError`, `DomainError`, `EOFError`, `ErrorException`, `Exception`, `IndexCartesian`, `IndexLinear`, `IndexStyle`, `InexactError`, `InvalidStateException`, `IOBuffer`, `IOContext`, `Irrational`, `KeyError`, `LinearIndices`, `LinRange`, `LoadError`, `MethodError`, `Missing`, `MissingException`, `Float32`, `Float64`, `Int`, `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `Integer`, `Matrix`, `Memory`, `Nothing`, `Number`, `OutOfMemoryError`, `OverflowError`, `Pair`, `ProcessFailedException`, `Rational`, `Real`, `Ref`, `Regex`, `RoundingMode`, `RoundDown`, `RoundFromZero`, `RoundNearest`, `RoundNearestTiesAway`, `RoundNearestTiesUp`, `RoundToZero`, `RoundUp`, `Set`, `Signed`, `StackOverflowError`, `String`, `StringIndexError`, `Symbol`, `SystemError`, `Channel`, `Task`, `TaskFailedException`, `Condition`, `Text`, `TextDisplay`, `Tuple`, `TypeError`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`, `UndefKeywordError`, `UndefRefError`, `UndefVarError`, `UnitRange`, `Unsigned`, `Vector`, `VersionNumber`

### Mathematical constants
- `VERSION`, `ENDIAN_BOM`, `Inf`, `Inf16`, `Inf32`, `Inf64`, `NaN`, `NaN16`, `NaN32`, `NaN64`, `im`, `missing`, `nothing`, `pi`, `π`, `ℯ`
- 注意: `e`, `γ`, `eulergamma`, `φ`, `golden`, `catalan` は `Base.MathConstants` サブモジュールからのみアクセス可能（upstream Julia と同様）

### Operators
- `!`, `!=`, `!==`, `%`, `&`, `*`, `+`, `-`, `/`, `//`, `<`, `<=`, `==`, `>`, `>=`, `\`, `^`, `|`, `~`, `:`, `=>`, `÷`, `≠`, `≡`, `≢`, `≤`, `≥`

### Scalar math
- `abs`, `abs2`, `acos`, `acosd`, `acosh`, `acot`, `acotd`, `acoth`, `acsc`, `acscd`, `acsch`, `angle`, `asec`, `asecd`, `asech`, `asin`, `asind`, `asinh`, `atan`, `atand`, `atanh`, `big`, `binomial`, `bitreverse`, `bitrotate`, `bswap`, `cbrt`, `ceil`, `cis`, `cispi`, `clamp`, `clamp!`, `cld`, `cmp`, `complex`, `conj`, `conj!`, `copysign`, `cos`, `cosc`, `cosd`, `cosh`, `cospi`, `cot`, `cotd`, `coth`, `count_ones`, `count_zeros`, `csc`, `cscd`, `csch`, `deg2rad`, `denominator`, `div`, `divrem`, `eps`, `evalpoly`, `exp`, `exp10`, `exp2`, `expm1`, `exponent`, `factorial`, `fld`, `fld1`, `fldmod`, `fldmod1`, `flipsign`, `float`, `floatmax`, `floatmin`, `floor`, `fma`, `fourthroot`, `frexp`, `gcd`, `gcdx`, `get_zero_subnormals`, `hypot`, `identity`, `imag`, `inv`, `invmod`, `isapprox`, `isassigned`, `iseven`, `isfinite`, `isinf`, `isinteger`, `isnan`, `isnegative`, `isodd`, `isone`, `ispositive`, `ispow2`, `isqrt`, `isreal`, `issubnormal`, `iszero`, `lcm`, `ldexp`, `leading_ones`, `leading_zeros`, `log`, `log10`, `log1p`, `log2`, `max`, `maxintfloat`, `min`, `minmax`, `mod`, `mod1`, `mod2pi`, `modf`, `muladd`, `nand`, `nextfloat`, `nextpow`, `nextprod`, `nor`, `numerator`, `one`, `oneunit`, `powermod`, `precision`, `prevfloat`, `prevpow`, `rounding`, `setprecision`, `setrounding`, `set_zero_subnormals`, `rad2deg`, `rationalize`, `real`, `reim`, `reinterpret`, `rem`, `rem2pi`, `round`, `sec`, `secd`, `sech`, `sign`, `signbit`, `signed`, `significand`, `sin`, `sinc`, `sincos`, `sincosd`, `sincospi`, `sind`, `sinh`, `sinpi`, `sleep`, `sqrt`, `tan`, `tand`, `tanh`, `tanpi`, `time`, `time_ns`, `trailing_ones`, `trailing_zeros`, `trunc`, `tryparse`, `parse`, `typemax`, `typemin`, `unsafe_trunc`, `unsigned`, `widemul`, `xor`, `zero`, `√`, `∛`, `∜`, `≈`, `≉`

### Arrays
- `append!`, `axes`, `checkbounds`, `cat`, `checkindex`, `circshift`, `circshift!`, `copy`, `copy!`, `copyto!`, `deepcopy`, `cumprod`, `cumprod!`, `cumsum`, `cumsum!`, `accumulate`, `accumulate!`, `deleteat!`, `diff`, `dropdims`, `insertdims`, `eachcol`, `eachindex`, `eachrow`, `eachslice`, `empty`, `empty!`, `extrema`, `fill`, `fill!`, `first`, `firstindex`, `hcat`, `indexin`, `insert!`, `invperm`, `invpermute!`, `isperm`, `keepat!`, `last`, `lastindex`, `length`, `map!`, `mapslices`, `maximum`, `maximum!`, `minimum`, `minimum!`, `ndims`, `ones`, `permute!`, `permutedims`, `permutedims!`, `pop!`, `popat!`, `popfirst!`, `prepend!`, `prod`, `prod!`, `push!`, `pushfirst!`, `logrange`, `range`, `repeat`, `reshape`, `resize!`, `reverse`, `reverse!`, `rot180`, `rotl90`, `rotr90`, `selectdim`, `similar`, `size`, `splice!`, `stack`, `step`, `stride`, `strides`, `sum`, `sum!`, `transpose`, `vcat`, `vec`, `zeros`

### Search/find
- `argmax`, `argmin`, `eachmatch`, `findall`, `findfirst`, `findlast`, `findmax`, `findmax!`, `findmin`, `findmin!`, `findnext`, `findprev`, `insorted`, `match`, `searchsorted`, `searchsortedfirst`, `searchsortedlast`

### Sorting
- `InsertionSort`, `issorted`, `MergeSort`, `partialsort`, `partialsort!`, `partialsortperm`, `partialsortperm!`, `PartialQuickSort`, `QuickSort`, `sort`, `sort!`, `sortperm`, `sortperm!`, `sortslices`

### Collections
- `all`, `allequal`, `allunique`, `any`, `collect`, `count`, `eltype`, `filter`, `filter!`, `foldl`, `foldr`, `foreach`, `mapfoldl`, `mapfoldr`, `get`, `get!`, `getindex`, `getkey`, `setindex!`, `haskey`, `hasmethod`, `applicable`, `in`, `in!`, `intersect`, `isdisjoint`, `isempty`, `issetequal`, `issubset`, `keytype`, `keys`, `map`, `mapreduce`, `merge`, `merge!`, `mergewith`, `mergewith!`, `pairs`, `reduce`, `sizehint!`, `setdiff`, `setdiff!`, `symdiff`, `symdiff!`, `union`, `union!`, `intersect!`, `unique`, `unique!`, `valtype`, `values`, `∈`, `∉`, `⊆`, `⊈`, `⊊`, `⊇`, `⊉`, `⊋`, `∩`, `∪`

### Strings and characters
- `ascii`, `bitstring`, `bytes2hex`, `chomp`, `chop`, `chopprefix`, `chopsuffix`, `codepoint`, `codeunit`, `codeunits`, `contains`, `digits`, `endswith`, `escape_string`, `hex2bytes`, `isascii`, `iscntrl`, `isdigit`, `isletter`, `islowercase`, `isprint`, `isnumeric`, `ispunct`, `isspace`, `isuppercase`, `isvalid`, `isxdigit`, `join`, `lowercase`, `lowercasefirst`, `lpad`, `lstrip`, `ncodeunits`, `nextind`, `ndigits`, `occursin`, `prevind`, `replace`, `repr`, `summary`, `reverseind`, `rpad`, `rsplit`, `rstrip`, `split`, `startswith`, `string`, `strip`, `textwidth`, `thisind`, `titlecase`, `unescape_string`, `uppercase`, `uppercasefirst`

### Text output
- `display`, `displayable`, `displaysize`, `dump`, `istextmime`, `popdisplay`, `print`, `println`, `printstyled`, `pushdisplay`, `redisplay`, `show`, `showable`, `showerror`, `sprint`, `take!`

### Path manipulation
- `abspath`, `basename`, `dirname`, `homedir`, `isabspath`, `isdirpath`, `joinpath`, `normpath`, `splitdir`, `splitext`, `splitpath`

### Filesystem operations
- `cd`, `close`, `countlines`, `cp`, `eof`, `filesize`, `isdir`, `isfile`, `islink`, `isopen`, `ispath`, `mkdir`, `mkpath`, `mtime`, `mv`, `open`, `pwd`, `read`, `readdir`, `readline`, `readlines`, `rm`, `tempdir`, `tempname`, `touch`, `write`

### Iteration
- `eachrsplit`, `eachsplit`, `enumerate`, `iterate`, `ntuple`, `only`, `tuple`, `zip`

### Object identity and equality
- `hash`, `identity`, `ifelse`, `isequal`, `isless`, `isnothing`, `oftype`, `Returns`, `Some`, `something`, `ismissing`, `coalesce`, `skipmissing`, `nonmissingtype`

### Types (type-related functions)
- `convert`, `promote`, `promote_rule`, `promote_type`, `typeof`, `isa`, `eltype`, `sizeof`, `isbits`, `isbitstype`, `supertype`, `fieldcount`, `fieldindex`, `fieldname`, `fieldnames`, `fieldoffset`, `fieldtype`, `fieldtypes`, `getfield`, `getproperty`, `hasfield`, `hasproperty`, `propertynames`, `setfield!`, `setproperty!`, `isconcretetype`, `isabstracttype`, `isprimitivetype`, `isstructtype`, `ismutable`, `ismutabletype`, `methods`, `nameof`, `nfields`, `objectid`, `which`, `isunordered`, `typeintersect`, `typejoin`, `widen`

### Linear algebra
- `adjoint`

### Random
- `rand`, `randn`

### Bitarrays
- `BitArray`, `BitMatrix`, `BitVector`, `falses`, `trues`

### Dequeues
- `delete!`

### Errors
- `error`

### Tasks and concurrency
- `asyncmap`, `schedule`, `fetch`, `wait`, `yield`, `yieldto`, `notify`, `istaskdone`, `istaskstarted`, `istaskfailed`, `current_task`, `task_local_storage`, `timedwait`, `waitany`, `waitall`, `errormonitor`

### Channels
- `bind`, `put!`, `isfull`, `isready`

### Metaprogramming
- `__precompile__`, `esc`, `evalfile`, `Expr`, `gensym`, `GlobalRef`, `include_dependency`, `include_string`, `LineNumberNode`, `macroexpand`, `macroexpand!`, `QuoteNode`

### Macros
- `@allocated`, `@allocations`, `@assert`, `@coalesce`, `@elapsed`, `@evalpoly`, `@lock`, `@show`, `@showtime`, `@something`, `@time`, `@timed`, `@timev`

---

## stdlib（サポート済みモジュール）

- **Test**
  - `@test`, `@testset`, `@test_throws`
  - VM builtins: `_test_record!`, `_testset_begin!`, `_testset_end!`
- **Printf**
  - `@printf`, `@sprintf`
- **Iterators**
  - `enumerate`, `zip`, `rest`, `countfrom`, `take`, `drop`, `cycle`, `repeated`, `product`, `flatten`, `partition`, `peel`, `nth`
- **Broadcast**
  - `broadcast`, `broadcast!`（ドット演算子/`f.` は VM 側で処理）
- **Statistics**
  - `mean`, `var`, `std`, `median`, `cov`, `cor`, `quantile` 等
- **Random**
  - `rand`, `randn`, `seed!`（決定的 RNG）
- **Dates**
  - Dates の Pure Julia 実装が存在し、fixture あり（モジュール修飾周りは状況により変動）
- **InteractiveUtils**
  - `versioninfo`, `supertypes`（簡易実装、コンパイラ内部の反射 API は未対応）
- **LinearAlgebra（部分）**
  - Pure Julia: `tr`, `dot`, `norm`, `cross`, `kron`, `transpose`, `Diagonal`, 行列積 `A * B`
  - Builtin routing: `svd`, `qr`, `lu`, `inv`, `det`, `eigvals`, `eigen`, `cholesky`, `rank`, `cond`（返り値は NamedTuple を含む）
  - `eigen()` 拡張: 対称行列・非対称行列の両方をサポート（非対称の場合は複素固有値/固有ベクトルを計算）

---

## ツール/周辺機能（REPL / FFI / WASM / AoT）

### REPL（開発用）

- ブロックコメント `#= ... =#`（ネスト含む）を含む入力分割
- REPL セッションで Expr/Symbol 等の値が保持されるため、`Meta.parse` → `eval` の往復が可能

### エラー（span + hint）

- SyntaxError / UnsupportedFeature / RuntimeError などの系統
- Swift/iOS 側に span 情報を伝播してハイライト可能

### C ABI（Swift/iOS 連携）

- `compile_and_run`, `compile_and_run_with_output`, `compile_and_run_detailed`
- メモリ解放 API（`free_string`, `free_execution_result`）
- 実行キャンセル API（`vm_request_cancel`, `vm_reset_cancel`）

### WASM/Web API（subset_julia_vm_web）

- `run_from_source`, `run_from_source_typed`, `run_ir_json`, `run_ir_simple`
- `get_version`
- `get_supported_features`, `get_unsupported_features`（概要リスト）
- Unicode 入力支援 API（LaTeX → Unicode 変換/補完）

### AoT / persisted formats

- `.sjir` の保存/読み込み（magic/version/flags + Core IR）
- `.sjvmbc` の保存/読み込み（magic/version/flags + compiled VM bytecode）
- `sjulia --compile` で Core IR ファイル生成
- `sjulia --compile-vm` / `--run-vm-bytecode` で VM bytecode 生成・実行
- `aot --ir` で Core IR ファイルから Rust 生成（AoT 実行向け）
- AoT 最適化（定数畳み込み / DCE / ループ最適化 / インライン等）

---

## 何をもって「サポートされている」と判断するか（検証の根拠）

- **fixture テスト**: `subset_julia_vm/tests/fixtures/`（カテゴリ別 manifest 管理）
- **統合テスト**: `subset_julia_vm/tests/integration_*_tests.rs` など
- **iOS/サンプルテスト**: `subset_julia_vm/tests/ios_samples_tests.rs` 等
- **関連ドキュメント**:
  - 実装済み: `docs/vm/DONE.md`
  - 進捗ログ: `docs/vm/STATUS.md`
  - 未実装: `docs/vm/UNIMPLEMENTED.md`

---

## 削除された関数（upstream Julia に存在しない）

以下の関数は upstream Julia Base に存在しないため、SubsetJuliaVM からも削除されました（Issue #1322）:

- `fliplr`, `flipud` - Julia の HISTORY.md にのみ言及（deprecated）
- `isalnum` - Julia の HISTORY.md にのみ言及（deprecated）

これらの関数は upstream Julia との互換性を維持するために削除されています。
