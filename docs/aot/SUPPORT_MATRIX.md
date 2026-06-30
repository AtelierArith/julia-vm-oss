# AoT 対応サブセット・マトリクス

**最終更新**: 2026-06-24

この表は `juliars` の現行 Rust backend (`--backend rust`) を基準に、Julia source/Core IR から Rust source を生成できる範囲をまとめます。VM (`sjulia`) の対応範囲とは一致しません。AoT は VM より静的情報を多く要求し、必要に応じて `subset_julia_vm_runtime::Value` へフォールバックします。

Cranelift backend 固有の対応/制限/ロードマップは [CRANELIFT_SUPPORT_MATRIX.md](./CRANELIFT_SUPPORT_MATRIX.md) に分離しています。

凡例:

- `対応`: 現行 pipeline で通常利用できる
- `一部対応`: 型/shape/呼び出し形によっては生成できるが、制限または runtime 依存がある
- `gate`: 診断付きで拒否する、または `--pure-rust` で失敗する
- `未対応`: 実装がない、または専用 issue で追跡中

## 入力と CLI

| 項目 | 状態 | 備考 |
|---|---:|---|
| Julia source file | 対応 | Parser -> Lowering -> Core IR -> AoT IR |
| stdin (`juliars -`) | 対応 | `-o -` と組み合わせ可能 |
| `-e` / `--eval` | 対応 | 1 つの source として lower |
| Core IR `.sjir` (`--ir`) | 対応 | serialized `Program` をロード |
| `compile_from_ir_bytes(&[u8])` | 対応 | `.sjir` と同じ共有 pipeline |
| `--check` | 対応 | Rust を書かずに AoT 可否と dynamic site を報告 |
| `--stats` / `--time-passes` | 対応 | LOC、推定 byte 数、dynamic site、pass timing を表示 |
| `--pure-rust` | 対応 | runtime 依存と dynamic dispatch が残る場合は失敗 |
| `--backend rust` | 対応 | 既定 backend |
| `--backend cranelift` | 一部対応 | `cranelift` feature build で experimental backend を呼び出す。scalar / straight-line subset のみ低レベル `IrModule` へ adapter lowering し、feature なし build は rebuild 手順付き diagnostic (Issue #6927)。`-O0..-O3` は Cranelift settings へ反映済み (Issue #7091) |
| `--emit-binary` | 対応 | 一時 Cargo project で `subset_julia_vm_runtime` を path dependency として link |
| `--target` | 対応 | `--emit-binary` の Cargo build へ target triple を転送。target toolchain は事前追加が必要 |
| `--diagnostic-format` / `--color` | 対応 | span 付き human 診断、JSON 診断、human 診断色付け |
| `--export-c-abi` | 一部対応 | scalar-only `#[no_mangle] extern "C"` entry 生成。overload は `symbol=function(Int64,Float64)` の引数型指定または generated method 名で解決。comma-separated bulk specs も対応 (Issue #7078)。non-scalar return contract は [ABI_AND_NUMERIC_CONTRACTS.md](./ABI_AND_NUMERIC_CONTRACTS.md) を参照 (Issue #7077) |

## 型と値

| Julia 機能 | 状態 | 備考 |
|---|---:|---|
| `Int32` / `Int64` / unsigned 整数 / `Float32` / `Float64` | 一部対応 | AoT `StaticType` と Rust primitive へ投影。同一整数型の `+`/`-`/`*` と `+=`/`-=`/`*=` は Rust `wrapping_*` で Julia overflow parity を保つ。`Float32` と整数の static arithmetic / comparison は `f32` 幅を保持し、`Float64` 混在時のみ `f64` へ広げる。`print` / `println` / `string(...)` 境界では whole float と `Inf` / `NaN` の Julia 表示を保持。float->integer / narrowing / sign-risk conversion は `InexactError` parity 用 runtime check が入るまで gate |
| `Bool` | 一部対応 | 条件式では Rust `bool` のみ許可し、non-Bool condition は diagnostic。`+`/`-`/`*` と比較では Julia の Bool-as-Integer promotion に合わせて numeric cast を生成し、`Bool * Bool` は Bool result を保持。`Bool`/`Bool` と mixed `Bool`/integer の `÷`/`%`/`^` は static primitive cases で Julia result surface に合わせ、signed `Bool ^ n` は `Value` boundary で `Bool` / `Float64` / `DomainError` を表す。Bool/float `^` は float `powf` path へ落とす (Issue #6980) |
| `String` | 一部対応 | literal / `*` concat (`String`/`Char`) / `string(...)` concat / 文字列補間 `"$x"`(Issue #7052)/ `length`(コードポイント数)に加え、`uppercase` / `lowercase` / `occursin` / `startswith` / `endswith` を Rust string メソッドへ直接 lower(call-graph leaf 化して pure-Julia Base body の `HasShape` 等を回避、Issue #7058)。`repeat` / `reverse`(Array overload あり)/ `split` / `replace` / full Base string semantics は VM 側が広い。C ABI export return は [ABI_AND_NUMERIC_CONTRACTS.md](./ABI_AND_NUMERIC_CONTRACTS.md) の borrowed/owned string shape が実装されるまで gate (Issue #7077) |
| `Char` | 一部対応 | literal は Rust `char` で表せる valid Unicode scalar を escaped Rust char literal として生成。`Char(0xd800)` のような Julia-invalid-codepoint carrier は Rust `char` で表現できないため gate (Issue #6967) |
| `nothing` / `Nothing` | 一部対応 | `LitNothing` は `()` へ落ちる。`Nothing` return function と `Union{T,Nothing}` nullable return は generated Rust の `()` / `Value` boundary で扱う |
| `missing` | 一部対応 | `AotExpr::LitMissing` と `Value::Missing` codegen。missing propagation / union splitting は別途追跡 |
| `Symbol` / `:foo` リテラル | 一部対応 | bare Symbol リテラル `:foo` は interned name を Rust `String` carrier として持ち、display(`foo`、コロン無し)/ Symbol 同士の `==`/`===` が parity。型は `Str` として推論し dynamic `Value` boundary を回避。Symbol と String の型区別(`typeof` / dispatch、`:foo == "foo"`)と quoted expression `:(a+b)` は carrier の範囲外 (Issue #7051) |
| `Any` | 一部対応 | runtime `Value` へフォールバック。`--pure-rust` では失敗要因 |
| `Union` | 一部対応 | multi-variant `Union{...}` は runtime `Value` Rust enum boundary で表現する。`Union{T,Nothing}` など nullable return / type-unstable local も同じ boundary で対応 (Issue #6977) |
| abstract type return | 一部対応 | 多くは `Any` / runtime boundary へ退避 |
| user `struct` | 一部対応 | field layout / constructor surface は限定的。struct definitions は field-type dependency の topological order で出力し、循環は diagnostic。parametric struct は `Complex` special-case を除き codegen 未対応だが、**未使用(reachable だが未構築)の parametric struct 定義は skip** して無関係なプログラムを通す。実際の構築 / unresolved constructor-like call は use-site で diagnostic-gate(Issue #6975 / #7251、一般 codegen は #7040) |
| `@enum` | 一部対応 | top-level `@enum`(`Stmt::EnumDef`)を AoT program に収集し、`pub type Name = i32` + メンバ定数(Julia 名のまま)を出力。メンバは Int32 として型付けし、`c = member` / `Int(c)` / `c == member` / 算術 / 条件分岐は parity(明示値 `red=1` も対応)。`println(enum)` のメンバ名 Display は Int32 carrier の範囲外(Display impl 未実装) (Issue #7050) |
| `BigInt` / `BigFloat` / `Rational` / `Irrational` | gate | 任意精度・有理・無理数 family は [ABI_AND_NUMERIC_CONTRACTS.md](./ABI_AND_NUMERIC_CONTRACTS.md) の runtime numeric handle contract で扱う。`BigInt`→fixed integer、`BigFloat`→Float64、`Irrational`→Float64 の silent narrowing は禁止し、helper 実装までは span 付き diagnostic を維持する (Issue #7056) |

## 制御構造と関数

| Julia 機能 | 状態 | 備考 |
|---|---:|---|
| 単純な関数定義 / 呼び出し | 対応 | 静的に解ける呼び出しは mangled Rust 関数へ direct call |
| キーワード引数 (kwargs) | 一部対応 | デフォルト値を持つ kwarg(`f(x; y=1)`)は trailing positional parameter として codegen し、call site は宣言順に「指定値 or デフォルト」で埋める(順序入替え・一部省略に対応)。`kwargs...` varargs / 必須 kwarg は未対応 (Issue #7042) |
| デフォルト位置引数 | 一部対応 | `f(x, y=10)` は lowering で forwarding stub(`f(x)=f(x,10)`)へ desugar。各 stub を自身の arity で typed-signature にマッチさせ、別 mangled 関数として出力(以前は full-arity 署名に潰れて呼び出しが arity 不一致)。dynamic dispatcher は最大 arity の arm のみ含み、低 arity stub は静的呼び出しで解決 (Issue #7044) |
| varargs / splatting | 一部対応 | fixed tuple splat と fixed-count `Vararg{T,N}` は static signature へ展開可能。open `args...` tail と dynamic `f(xs...)` は runtime tuple packing / call adapter contract が接続されるまで gate する。詳細は [CALL_CONTROL_FLOW_CONTRACTS.md](./CALL_CONTROL_FLOW_CONTRACTS.md) (Issue #7043) |
| 多重ディスパッチ | 一部対応 | 静的解決中心。static resolver は matching methods から concrete/exact specificity を優先し、runtime dispatcher も broad `Any` より specific arm を先に出す |
| first-class functions | 一部対応 | known monomorphic callee は generated function item / `fn` pointer として扱い、unknown callable・戻り値としての関数・method table は runtime callable handle が必要。runtime callable helper までは dynamic sites を gate (Issue #7053) |
| `if` / ternary | 対応 | 条件は `Bool` のみ。`if 1` のような non-Bool condition は Julia parity diagnostic |
| `while` / `for` | 一部対応 | range/collection 形状に依存 |
| type-unstable locals | 一部対応 | slot type が `Any` / `Union` の場合は assignment value を `Value::from(...)` へ boxing。native slot への incompatible assignment は diagnostic |
| `break` / `continue` / `return` | 対応 | AoT statement surface あり |
| top-level globals | 一部対応 | AoT は compile-time に閉じた scalar primitive initializer のみ扱う。top-level `const` marker / const 再定義、関数内 `global` による mutable global state、同一 signature の関数再定義(world-age 依存)は span 付き diagnostic で拒否。`String`/配列など heap/runtime initializer も diagnostic で拒否 (Issue #7061) |
| expression-position `begin` / `let` | gate | bindings なし・単一 expression の value block は対応。multi-statement / side-effecting block は sequence expression carrier まで diagnostic gate (Issue #7014) |
| closures / lambdas | 一部対応 | non-capturing lambda は static function path。capturing closure は by-value / by-reference environment と call shim contract に従う。mutable capture、unknown HOF dispatch、runtime callable handle が必要な形は helper 接続まで gate (Issue #7055) |
| do-block | 一部対応 | Julia lowering 後の anonymous function argument として扱う。non-capturing do block は known-callee path、capturing do block は closure environment contract、unknown callee は first-class function gate に従う (Issue #7054) |
| exceptions (`throw` / `error`) | 一部対応 | runtime helper に依存する経路あり。AoT `throw` helper は `Display` text を使い、`RuntimeError::DivisionByZero` などは Julia-compatible message で表示する (Issue #7018) |
| `try` / `catch` / `finally` | gate | Rust unwinding ではなく status-bearing Julia exception boundary へ lower する。catch variable、finally execution ordering、rethrow state を保持できるまで span diagnostic で拒否する。詳細は [CALL_CONTROL_FLOW_CONTRACTS.md](./CALL_CONTROL_FLOW_CONTRACTS.md) (Issue #7032) |
| macros | 一部対応 | lowering 後 Core IR に展開される形次第。AoT 専用 macro semantics はない。Base timing macros は生成バイナリ実行時の `time_ns()` で測定し、`@time` は経過秒を出力して本体値を返し、`@elapsed` は `Float64` 秒を返す(no-op ではない)。allocation/GC 統計は現行 Base macro の簡略実装に従う (Issue #7059) |

## コレクション

| Julia 機能 | 状態 | 備考 |
|---|---:|---|
| 1D `Array` / `Vector` literal | 一部対応 | element type が静的な範囲では Rust `Vec<T>` |
| 2D array / matrix literal | 一部対応 | nested `Vec` 表現。`length` / `size` / `ndims` の 2D branch は `StaticType` rank から生成し、2D `length` は rows*cols を返す。indexing parity は追跡中 |
| 3D+ array | 未対応 | 一般 N 次元 array codegen は未整備。shape builtins は 1D/2D 以外 diagnostic gate |
| array indexing | 一部対応 | 1-based -> Rust indexing 変換あり。bounds/error parity は限定的 |
| `zeros` / `ones` | 一部対応 | inferred return element type が concrete scalar の場合は fill literal の型幅を保持。full constructor semantics / non-scalar element は追跡中 |
| `map` / `filter` | 一部対応 | static/named function 形では対応範囲あり。要素は `Clone` 前提で扱い、`String` など non-`Copy` element も Copy-only destructuring しない |
| broadcast / broadcast fusion | gate | static scalar/array element loop と fused expression tree の contract を固定。shape/axes/element type が静的に証明できない場合や runtime `Value` dispatch が必要な場合は runtime broadcast helper 接続まで gate (Issue #7047) |
| tuple literal | 対応 | Rust tuple へ生成 |
| tuple indexing / `first` / `last` | 一部対応 | 定数 in-range index は Rust tuple field access (`.0`, `.N`) を生成。dynamic `t[i]` と out-of-bounds literal は Union/runtime tuple indexing 設計まで diagnostic gate (Issue #6962)。tuple-specific `first` / `last` も field access を生成 |
| `Range` | 一部対応 | Rust backend では range expression を `Vec<T>` に materialize。integer / `Float32` / `Float64` の positive step・negative step・empty direction・zero-step diagnostic を固定。lazy range family / Char range は追跡中 |
| `Dict` | gate | construction / literal / get / haskey / iteration codegen は未整備。local `Dict(...)` construction は Issue #6971/#7016 として span 付き `UnsupportedInstruction` で拒否 |
| `Set` | gate | construction / membership / iteration codegen は未整備。local `Set(...)` construction は Issue #6972/#7016 として span 付き `UnsupportedInstruction` で拒否 |

## 数値・組み込み

| Julia 機能 | 状態 | 備考 |
|---|---:|---|
| 基本算術 (`+`, `-`, `*`, `/`) | 一部対応 | primitive static path と dynamic fallback が混在 |
| 比較 / 論理 | 一部対応 | 条件式で使う範囲は対応。Bool と整数/浮動小数の mixed comparison は Rust 側で numeric cast |
| ビット演算 (`&`, `|`, `xor`/`⊻`, `<<`, `>>`, `>>>`, `~`) | 一部対応 | `&`/`|`/`xor` は Rust native 演算子。`<<`/`>>`/`>>>`/`~` は Julia 意味論の helper(`SjuliaShift` trait + `op_bnot`)経由で codegen し、過大シフト→0(符号付き `>>` は sign fill)・負シフトは逆方向・`>>>` は論理シフトを保つ。定数畳み込みも同じ意味論に揃えた。Int64 は parity 確認済み。unsigned 値の構築は `InexactError` 変換 gate (#6968) のため end-to-end 未検証 (Issue #7057) |
| subtype (`<:`) | 一部対応 | 両オペランドが静的に既知の型名(builtin 型、ユーザ `struct` / `abstract type`)の場合は IR converter で型関係を解決し `true` / `false` のブール定数へ畳み込む。`Dog <: Animal` のようなユーザ階層は struct/abstract の親子関係から解決する。runtime の型値が絡む `<:` は値の `<` へは落とさず gate のまま (Issue #7037) |
| `typeof` / DataType values | 一部対応 | `typeof(x)` は runtime `Value::DataType(String)` を生成し、Julia type name display (`Int64`, `DataType` など) を保持。現状の carrier は display/name 用で、full DataType identity / parameters / reflection / dispatch object model までは主張しない |
| 型変換 (`T(x)`) | 一部対応 | lossless integer widening、integer->float、Float32/Float64 間、Bool->numeric は unchecked Rust cast。float->integer / 整数 narrowing / 符号境界 / numeric->Bool / `fptosi` は **`InexactError`-checked 変換**で対応(float->int はラウンドトリップ `(v as T) as F == v` で範囲外/小数/NaN/Inf を検出、int->int は `try_from`、メッセージは `InexactError: Int64(3.5)` / `trunc(Int8, 300)` / `Bool(2)` parity)。`Issue #7038`。注: 超大 out-of-range float の error メッセージ内 float 表示は別途 `__sjulia_format_float64` の大値表示課題に依存 |
| Complex | 一部対応 | monomorphic Rust `Complex` は `Float64` field layout のみ。`Complex` / `Complex{Float64}` / `ComplexF64` / legacy `Complex64` の basic construction/add/sub/mul surface は対応。Julia global `im` は lowercase Rust const として出し、local `im` binding は通常の lexical shadowing に任せる (Issue #6966)。`Complex{Float32}` / `Complex{Int64}` など parameterized non-`Float64` arithmetic は parameterized Complex codegen まで diagnostic gate (Issue #6965) |
| random (`rand` / `randn`) | gate | VM-compatible RNG contract / seed control が未設計のため、ad hoc `rand::random` や定数 fallback は出さず diagnostic で拒否 |
| `ccall` / `llvmcall` / Core intrinsics | gate | native-call boundary として span 付き unsupported error にする |

## バックエンドと生成物

| 項目 | 状態 | 備考 |
|---|---:|---|
| Rust source generation | 対応 | 主経路 |
| runtime-linked Rust | 対応 | `subset_julia_vm_runtime` が必要な生成物あり |
| standalone Rust | 一部対応 | `--pure-rust` で enforcement |
| optimizer pipeline | 一部対応 | DCE / constant folding / CSE / strength reduction / loop opts / inlining / direct self-tail recursion TCO。相互再帰は通常の static call として codegen し、mutual tail-call elimination は現時点では行わない。DCE は block-local overwritten dead stores を conservative に削除。CSE は structured dominator から branch/loop body への available expression reuse に対応し、loop-modified operands は invalidation。LICM の低レベル CFG back-edge 検出は dominator analysis ベースで、loop-invariant scalar control condition は induction 依存を除外して hoist 可能。inliner purity は known pure static callees を fixed-point 解析し、dynamic calls は impure のまま保持 (Issue #7060) |
| `rustc -D warnings` clean guarantee | 対応 | generated Rust prelude が既知の codegen 由来 warning を抑制し、`aot_e2e_tests` は代表生成 crate を `RUSTFLAGS=-Dwarnings cargo check` で検証。`scripts/test_aot.sh` も warning 誘発 source を generated-Rust `cargo clippy -- -D warnings` smoke に通す (Issue #7076) |
| C ABI export (`#[no_mangle] extern "C"`) | 一部対応 | `Int8/16/32/64`, `UInt8/16/32/64`, `Float32/64`, `Bool`, `Nothing` return の scalar signature のみ。`String`/配列/struct/`Any` は [ABI_AND_NUMERIC_CONTRACTS.md](./ABI_AND_NUMERIC_CONTRACTS.md) の C-stable view / out-param / opaque handle shapes が実装されるまで拒否 (Issue #7077) |
| Cranelift backend | 一部対応 | `--features cranelift` で CLI から到達可能。現状は scalar / native stack aggregate subset の experimental native compile に限定する。runtime `Value` / rooting model が必要な signature・型は [CRANELIFT_GC_ROOTING_CONTRACT.md](./CRANELIFT_GC_ROOTING_CONTRACT.md) の実装 follow-up が入るまで拒否。詳細は [CRANELIFT_SUPPORT_MATRIX.md](./CRANELIFT_SUPPORT_MATRIX.md) |

## 診断の読み方

`--check --stats --time-passes` は、生成物を書かずに AoT pipeline を通し、静的に落とせない箇所、dynamic dispatch site、pass timing を確認するための最短経路です。

完全 standalone を狙う場合は `--pure-rust` を追加してください。失敗時は AoT IR 上の dynamic operation と、生成 Rust に残った `subset_julia_vm_runtime` 参照行が表示されます。

## ベンチマーク

AoT optimizer pass の synthetic Criterion gate は `cargo bench -p subset_julia_vm --features aot --bench aot_optimizer_benchmark` で実行できます。CI や構文/リンク確認だけなら `--no-run` を付けて bench target の compile gate として使えます。

## Fixture Parity

AoT stdout parity を fixture 単位で見る場合は、release `juliars` を build したうえで `bash scripts/aot_fixture_julia_parity.sh <fixture.jl>` を使います。この helper は `juliars --emit-binary` で native binary を作り、generated binary stdout と upstream `julia` stdout を exact diff します。

VM と generated AoT binary の差分だけを切り分ける場合は、release `juliars` と release `sjulia` を build したうえで `bash scripts/aot_vm_differential.sh <fixture.jl> [...]` を使います。この helper は `sjulia` VM stdout と generated binary stdout を exact diff します。

VM-passing fixture が silent mismatch を起こしていないことを検査する場合は、release `juliars` と release `sjulia` を build したうえで `bash scripts/aot_fixture_no_silent_mismatch.sh [fixture.jl ...]` を使います。引数なしでは manifest fixture corpus を列挙し、compiled fixture は original stdout と final value の両方を VM と比較します。AoT unsupported は `UnsupportedInstruction` exit code のみ許容します。

Supported builtin/operator parity fixtures live under `subset_julia_vm/tests/fixtures/aot/`. `builtin_stdout_parity_6999.jl` is the first generated-binary stdout fixture; Float64 whole-value printing for `sqrt(9.0)` is tracked separately in Issue #7013.
