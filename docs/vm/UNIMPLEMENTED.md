# 未実装機能一覧

**最終更新**: 2026-07-19. 未実装および残スコープは下の日付別「最新対応」セクションを正とし、先頭メタデータには長い issue 要約を重複させない。

> 実装済みの機能は [STATUS.md](./STATUS.md) と [DONE.md](./DONE.md) を参照してください。

## 最新対応 (2026-07-19)

### top-level control-flow nominal definitions の残スコープ (Issue #11654)

top-level から到達する `if` / `for` / `while` / `try` 内の non-parametric
`struct`、`abstract type`、`primitive type`、`@enum` は、compile-time hoist
ではなく inert bytecode template として保持し、実行が宣言位置へ到達した時だけ
VM registry と lexical module binding を publish するようになった。未到達 branch / zero
iteration は未定義のまま、親型不在は `UndefVarError` として catch 可能かつ
non-publishing、非型の親・未解決 field type・不正 primitive width も mutation 前に
catchable error として検証する。到達型は同じ入力の後続 method signature から参照でき、
REPL は到達した定義だけを次 eval の compiler snapshot に採用する。
後続の distinct root nominal declaration と混在する入力では registry ID を予約し、
fresh/full compile でも実測 registry start ID から activation prefix を検証する。
runtime site ID は rebase 済み definition chronology を使い、runtime enum を含む
live delta は enum registry rollback transaction を必ず開始する。4 family とも
source-order activation を維持する。parity fixtures:
`types_runtime_nominal_control_flow_11654`、
`types_runtime_nominal_control_flow_edges_11654`。

残スコープは次の独立 Issue に限定する。

- #11678: control-flow 内 parametric struct の explicit constructor activation。
- #11683: 到達 runtime nominal と未到達 root nominal の間で uncaught error が出た
  REPL input の exact-prefix recovery / registry ID remap。
- #11697: `x::typeof(1)` のような runtime-computed field annotation を宣言時に
  評価して layout/reflection へ採用する。現状は誤った `Any` publish を避け、到達時に
  catchable `NotImplemented` として non-publishing にする。

### REPL full-recompile retirement の残スコープ (Issue #9784)

通常の lambda/HOF、do-block、generator body/predicate helper は activation
index set による live install と exact-prefix recovery を実装済みのため、
full-recompile の残スコープから外す。

今後も fail-closed fallback に残るのは Base/preload-owned method extension、
parametric / inner-constructor / redefined type、package module、inner `using` /
`import`、module-level macro/type alias、`baremodule`、non-mirrorable module
binding、opaque runtime `eval`、および完全な target/alignment surface を
構造的に証明できない将来の helper lowering 形。これらを live
transaction へ移した後、global 再注入・蓄積 definition・module mirror を
削除するまで #9784 は open のままとする。

## 最新対応 (2026-07-18)

### REPL live definition transaction の残スコープ (Issue #9784)

新規 Main abstract type、非 parametric primitive type、`@enum` は Issue #11635
の修正で source-order live activation、後続 eval の held-VM 再利用、catchable
error 後の exact-prefix recovery まで実装済みとなったため、未実装一覧から外す。

Issue #9784 に残るのは、parametric / inner-constructor struct、既存 type の
redefinition、Base/preload-owned method mutation、module / `using` / `import` /
macro / type alias / `baremodule`、opaque runtime `eval` を同じ構造的 transaction
へ載せる作業と、それらの完了後に full-recompile fallback の再注入・host mirror
を削除する作業。未対応形は silent shortcut を使わず、従来どおり保守的な full
recompile へ送る。

## 最新対応 (2026-07-12)

### Dict-op loops in the typed-loop recognizer (Issue #10560) — measured, not implemented

Split from #10477 scope item 3: `haskey`/`getindex`/`setindex!` over `Dict`
inside a recognized loop. Investigated and measured, but not implemented this
round.

Measurement (`benchmarks/dict_typed_loop_bench.jl`, verified against upstream
`julia`; release-fast, same binary, min timing): a `d[k] = get(d, k, 0) + 1`
histogram loop over 100,000 iterations takes 18.3s, a `haskey(d, k)` loop
10.4s, and a `d[k]` (getindex) lookup loop 13.8-14.9s, versus 0.2s for an
equivalent plain-scalar typed loop of the same iteration count — Dict
operations are **50-90x slower** than the interpreter's own scalar typed-loop
path today. The overhead is NOT hashing (`hash()` is already a Rust builtin)
but interpreter dispatch cost through `get`/`setindex!`/`haskey`'s
multi-level pure-Julia call chain in
`subset_julia_vm/src/julia/base/dict.jl` (`ht_keyindex2!`, `_setindex!`,
`hashindex`, ...). So unlike the String case (#10559, 2-4.5x), the *potential*
win here is large — this is a real, not speculative, opportunity, deferred
for structural reasons below, not because the win is small.

Blockers:

1. **No slot class exists** for a generic `Value::StructRef` local in the
   typed-loop IR. A `Dict` local is rejected at the very first instruction
   that touches it (`SJULIA_TYPED_LOOP_DEBUG=1` shows
   `unsupported-instr:LoadSlotStruct`). Array (`ArrayRef`) and String
   (`Rc<str>`) have dedicated `Value` variants with dedicated typed-loop slot
   classes (`array_slots`/`str_slots`); Dict does not — it was deliberately
   migrated off `Value::Dict` onto pure-Julia `StructRef` dispatch in Issue
   #6731, so there is no native Rust Dict representation left to bridge from.
2. Two implementation strategies were considered and both rejected for this
   round:
   - **(a) Reimplement the linear-probe/rehash hash-table algorithm natively
     as new Rust `TypedLoopOp`s.** Rejected: this violates design principle 3
     ("Pure Julia First — avoid new Rust intrinsics") and reverses #6731's
     deliberate migration to a single pure-Julia source of truth for Dict
     semantics. It would create a second implementation of probing/rehashing
     that must never diverge from `dict.jl` — exactly the maintenance risk
     #6731 eliminated.
   - **(b) Inline the actual `dict.jl` bytecode** (`get`/`setindex!`/`haskey`/
     `ht_keyindex`) into the typed loop via generic struct-field load/store +
     generic `Memory{T}` load/store typed-loop ops. Architecturally sound
     (keeps `dict.jl` as the single source of truth) but needs prerequisite
     typed-loop IR capabilities that do not exist yet: no generic struct-field
     slot class, no generic `Memory{T}` ops (today's array bridge is specific
     to the `Vector{T}`-from-`x[i]`-syntax shape), and no nested-loop inlining
     (`ht_keyindex`/`ht_keyindex2!` are themselves `while true` probing loops
     nested inside the recognized outer loop — nested-loop nativization is
     #10477 scope item 5 / tracked at #10561, where a related nested-loop
     entry-cache idea was already found NOT a net win).
3. Both paths risk shipping a subtly-wrong hash-table/probe/rehash
   implementation that silently returns wrong values rather than crashing —
   the same "silent corruption is worse than not implementing" concern the
   #10504 bail guard exists to prevent, and it applies to a mismatched read
   path just as much as to an unsound mutation/bail interaction.

Follow-ups filed: #10743 (generic struct-field / `Memory{T}` load-store ops in
the typed-loop IR — the shared prerequisite), #10744 (re-attempt Dict-op
typed-loop recognition once #10743 lands; also specifies deferring
`setindex!`/mutation transactionality to a later step — reject loops
containing Dict mutation for the first cut, which is sanctioned by the
#10504 bail guard's "stay REJECTED or become transactional" rule). Full
investigation notes:
`memory/project/project_10560_dict_typed_loop_not_implemented.md`.

## 最新対応 (2026-07-11)

### Qualified `Base.Bottom` / `Base.BitSigned` 等の Base const alias アクセス (Issue #10579)

upstream は非 export の Base const 型 alias にも qualified アクセスできる
(`Base.Bottom === Union{}`、`Base.BitSigned`)が、sjulia の `Base.X` メンバー
解決は prelude レベルの const 型 alias を知らず
`Compilation error: Msg("Base has no function named Bottom")` になる。
alias table が flat・非 qualified なため。関連: bare 非 export alias の
Main へのリーク (Issue #10578、`BitSigned` 等 — `Bottom` 自体は #10304 で
prelude 定義を撤去して解消)。

### struct definition nested in a top-level if/for/while/try body (Issue #10401) — 解決済み

Issue #11654 の runtime nominal template / reached-only publication により解消。
残る parametric / inner-constructor / REPL error-prefix / same-site identity は
上記 #11678 / #11679 / #11683 に分離済み。

## 最新対応 (2026-07-10)

## 最新対応 (2026-07-07)

### Invalid UTF-8 byte strings (Issue #8995) — 解決済み

`String(UInt8[0xff, 0x61])` は raw bytes を保持する byte-backed `String` として
表現できるようになった。`ncodeunits`, `codeunit`, `codeunits` は upstream と同じ byte
列を観測し、typed string slot でも invalid bytes を replacement character に潰さない。
再オープン分 (2026-07-16): iterate は upstream の byte-offset state で正確な
malformed Char (`Value::CharMalformed`) を線形 yield し、getindex / length /
splat / repr / 等値 / 1引数 isvalid / `'\xff'`・`"\xff"` リテラル / concat・
`print(io, s)` のバイト保存まで対応済み。stdout 系 sink への print は host
出力パイプラインが String 型のため replacement character 表示のまま (意図的
divergence)。`UInt8(::Char)` 等は Issue #11406 として分離。残スコープなし。

## 最新対応 (2026-07-03)

### Quoted macro definitions in quote expressions (Issue #9134) — 解決済み

`quote` body 内の `macro ... end` は upstream 形状の
`Expr(:macro, signature, body)` に lower されるようになった。AbstractAlgebra.jl
の `Assertions.jl` include path を塞いでいた
`quote for macro_definition not yet supported` は解消済み。残スコープなし。

### `MersenneTwister(seed)` random stream diverges from upstream dSFMT (Issue #8998) — 恒久 divergence として文書化

sjulia の `MersenneTwister` は MT19937-64 (`vm/rng.rs`) でバックされており、upstream Julia
の dSFMT (SIMD-oriented Fast MT、`julia/stdlib/Random/src/RNGs.jl`) とは同一 seed から
異なるビット列を生成する。実装の完全互換には dSFMT 2.2 + Julia 側の `MTCache` バッファリング
挙動の完全移植が必要であり、工数が大きいため現時点では対応を保留する。

**divergence の具体例 (seed=42):**
- upstream Julia: `0.7108238673434464`, `0.0644852510983267`
- sjulia: `0.755155532954539`, `0.6390313938546974`

デフォルト RNG (Xoshiro256++) と StableRNG は upstream 忠実移植であり影響しない。
再現性が必要なコードは `StableRNG(seed)` の使用を推奨する。

Fixture `tests/fixtures/stdlib/mersenne_twister_stream_8998.jl` が sjulia の現行ストリームを
固定しており、意図せぬ変更を検出する。dSFMT 実装時はこの fixture を upstream 値で置き換える。

**補足 (Issue #9265, 解決済み):** #8998 で付記されていた「型指定 rand の別ギャップ」
(`rand(rng, UInt32)` がエラー、`rand(rng, Int)` / `rand(Int)` が Float/0次元配列を返す) は
スカラー型指定 rand — `rand([rng], ::Type{T})` — として独立に修正済み。整数/`Bool`/浮動小数点の
具象型を、リテラル型・実行時型値(型のタプルを走査する等)の双方、明示 RNG とグローバル RNG の
双方で正しい型で返す。ストリーム値そのものの dSFMT 一致は引き続き本 issue で保留。

### Static unary negation for call-returned structs (Issue #9059) — 解決済み

Call-returned concrete structs now recover their Julia struct type before the
unary `-` fallback errors, so `Base.:-(::T)` dispatch works for direct call
operands such as `-sin(x)`. No remaining scope is tracked under #9059.

### MacroTools @assert @capture binding scope (Issue #9055) — 解決済み

Empty-binding begin-style `LetBlock` values no longer restore assignments after
evaluation, while actual macro-produced `Expr(:let, ...)` blocks keep a
synthetic scope marker. No remaining scope is tracked under #9055.

### REPL timing macro assignment persistence (Issue #9044) — 解決済み

Assignments nested inside Pure-Julia `@time` macro result capture now persist
as REPL globals, while ordinary `let` assignments remain local. No remaining
scope is tracked under #9044.

### Explicit @doc module lowering (Issue #9041) — 解決済み

Explicit doc macrocalls wrapping module or baremodule definitions now lower the
module target directly, so package module headers no longer fail on
`quote for module_definition not yet supported`. No remaining scope is tracked
under #9041.

### Test.@test_throws catch-state recording (Issue #9023) — 解決済み

`@test_throws` now records the thrown case directly from `catch`, so
catch-scope restoration no longer turns a thrown `DivideError` into a failed
test. No remaining scope is tracked under #9023.

### Numeric specialization and Rational BigInt predicate parity (Issue #8987) — 解決済み

Specialized `Int64 ^ Int64` calls and `Rational{BigInt}` zero/one predicates
now match upstream Julia; no remaining scope is tracked under #8987.

### Chained where operator assign-form lowering (Issue #8948) — 解決済み

Operator assign-form definitions such as
`*(a::Wrap{T}, b::Wrap{S}) where S<:Number where T = ...` now keep the chained
`where` clauses as separate method type parameters instead of folding `where T`
into `S`'s upper bound.

### Int64 div signed-min overflow exception parity (Issue #8896) — 解決済み

`div(typemin(Int64), Int64(-1))` now throws upstream-compatible `DivideError`
instead of aborting the VM through Rust signed-division overflow. The native
Int128 intrinsic path uses the same checked-division guard.

### Post-typemap dispatch heuristic leftovers (Issue #8999) — 解決済み

The documented leftovers are now either shrunk or retired: `DeferImprecise`
uses conservative intersection to reject proven-disjoint non-nominal candidates,
`SJULIA_DISPATCH_COMPARE=1` emits per-verdict counts for future sweeps, and the
obsolete `resolve_runtime_type_pattern_candidates*()` string-channel runtime
resolver wrappers were removed after production `CallDynamic*` paths were
confirmed to use structured `core_signature` candidates. Remaining scoring
constants still serve deferred ranking and should be retired only under new
targeted issues with sweep evidence.

### Direct Memory lattice tracking (Issue #9034) — 解決済み (tracking option 実装)

Issue #9034 offered two options — add `Memory{T}` lattice tracking or formalize
the limitation. The **tracking option is now implemented** (PR #9052):
`ConcreteType::Memory { element, ndims }` mirrors `ConcreteType::Array`, so
`ValueType::Memory` / `ValueType::MemoryOf(T)` map to a concrete lattice value
instead of widening to `LatticeType::Top`. `m::Memory{Int64}` parameter
annotations now type the slot as `MemoryOf(I64)` (previously `Any`), verified in
`sjulia_cli_dump_bytecode_tests::direct_memory_user_functions_track_memory_lattice_issue_9034`
and `bridge::test_memory_values_map_to_concrete_lattice_issue_9034`. Runtime
behavior is unchanged (`memory_direct_lattice_boundary_9034` fixture). Residual
precision gap: `Memory{T}(undef, n)` constructor calls and indexed-load *return*
types still widen to `Any` in the abstract interpreter — the same gap that
`Array{T}(undef, n)` has — because parametric built-in constructors are not
inferred; this is a shared Array/Memory follow-up, not Memory-specific.

### MustAlias narrowing for indexed loads and fresh aliases (Issue #9035) — 解決済み

The documented limitations are now formalized as intentional compatibility
boundaries: upstream Julia 1.12.6 keeps both mutable indexed-load guards and
fresh aliases after field guards conservative. The
`type_inference_mustalias_narrowing_limits_9035` fixture pins the expected
`Union{Nothing, Int64}` inference result and runtime behavior. No remaining
implementation scope is tracked under #9035; future precision work needs a new
upstream-compatible MustAlias/ConditionalsLattice design issue.

### Inference limitation tracking audit (Issue #9009) — 解決済み

The two documented-but-untracked inference limitations now have dedicated child
issues: the now-formalized direct Memory lattice precision boundary (Issue
#9034), and the now-formalized MustAlias compatibility boundary (Issue #9035).
No remaining implementation decision is tracked under #9009.

### UB detection layer (Issue #9004) — 初期ゲート実装済み

The initial UB detection layer is in place: unsafe inventory ratchet, focused
miri smoke, and FFI sanitizer harness. Remaining expansion is incremental:
audit existing unsafe sites and replace baseline entries with `Safety:`
comments, then add more focused miri tests for newly audited unsafe-heavy VM
internals.

### Int64 fld/cld zero-denominator exception parity (Issue #8901) — 解決済み

`fld(::Int64, ::Int64)` and `cld(::Int64, ::Int64)` now route through integer
`div`/`rem` rounding formulas, so zero denominators throw `DivideError` instead
of `InexactError`.

### const builtin-operator alias callable globals (Issues #8911, #8907, #8902, #8904) — 解決済み

Top-level const aliases to builtin operator/function values, including
`const lt = (<:)` and `const is_a = isa`, are now visible as callable globals
inside user function bodies. On the merged branch,
`ssa_pipeline_parity_dispatch_issue_8552` also passes again, so #8904's `lt`
import-resolution failure is no longer an open scope.

### Extended Unicode lexer coverage (Issue #8751) — 解決済み

Emoji identifiers, prime/modifier-letter identifier suffixes, miscellaneous
Unicode operators, middle-dot aliases, `⁝`, and Julia operator suffix forms
such as `+̂`, `+̂′`, `+⁽¹⁾`, and `+₍₀₎` now lex and parse in the focused parser
coverage.

No remaining parser-corpus scope is tracked under #8751. The files that still
fail after removing the lexer errors now fail on non-lexer `UnexpectedToken`
syntax families and are tracked under #8759.

### compile→VM bytecode crate split / generic backend IR (Issue #8837) — 一部進捗

`ARCHITECTURE_OVERVIEW.md` now includes the layered crate diagram and explains
how the current `src/bytecode.rs` and `src/runtime_types.rs` staging facades map
to the planned `subset_julia_vm_bytecode`, `subset_julia_vm_compile`, and
`subset_julia_vm_vm` crates. Peephole optimization is now owned by
`subset_julia_vm_bytecode`. Stack-bytecode finalization ownership now covers
both peephole optimization and slotization; the crate-internal bytecode facade
only adapts the remaining VM-owned `KwParamInfo`/`CompiledProgram` shapes during
the transition.
The runtime parametric-constructor fallback now uses the
`runtime_types::parametric` owner for `infer_parametric_type_args`, and
`ExceptionType` plus `Effects`/`EffectBit` are owned below `compile/`; the direct
`vm_to_compile` audit baseline remains 0 runtime references plus 4 test-only
references.

Remaining scope is implementation, not overview documentation: extract the
bytecode facade into its planned crate without changing serialized bytecode or
cache enum order, and make the stack and register VM backends lower from one
shared generic IR.

### Parser additional corpus gaps (Issue #8759) — 一部進捗

The representative #8759 syntax list now parses: named-tuple `for`
destructuring, inline-semicolon abstract type declarations, `function @main`,
nested/typed tuple `for` bindings, `;;`, keyword `@nospecialize(type)`,
operator-suffix imports such as `import Base.<`, range splatting, comma-separated
`=` bindings including newline-split loop headers, generator comma-bindings, and
labeled `break` / `continue`. This slice also accepts statement forms inside
grouped quote/parenthesis contexts, including `:(const ...)`, `:(global ...)`,
`:(export ...)`, and `(@eval (using ...))`, plus operator-like quoted symbols
such as `:(.)`, `Base.:(:)`, `:+=`, and adjacent-identifier symbols like
`:maximum!_fast`. Statement forms now also parse in ordinary expression bodies,
including short-form function RHS and arrow bodies such as
`f(a) = for ... end`, `c -> for ... end`, and `() -> global loaded = true`.

The parser corpus allowlist for #8759 was reduced from 96 to 38 entries after
the representative parser slice, #8751 reattribution, dotted/broadcast operator
follow-up (`.===`, `.!==`, `.<<`, `.∈`, `.≈`), and type-position postfix
`where` support, then to 37 entries after tuple-destructuring parameters with
default values removed `base/regex.jl`, and to 35 entries after adjacent `∘`
composition forms removed `base/precompilation.jl` and `test/operators.jl`.
It then fell to 34 entries after Unicode assignment quoted-symbol forms removed
`stdlib/REPL/test/docview.jl`, and to 33 entries after `≲` comparison operator
support removed `base/version.jl`, and to 32 entries after qualified parameter
metadata annotations removed `base/Base_compiler.jl`, and to 31 entries after
bang-mid field-name support removed `base/sysinfo.jl`, then to 30 entries after
nested RHS tuple-assignment support removed `base/strings/util.jl`, and to 28
entries after invalid-byte character literal support removed
`test/strings/search.jl` and `test/strings/util.jl`, then to 27 entries after
short `\x` / `\U` character escape support removed `test/char.jl`, and to 25
entries after ternary then-branch assignment/pair support removed
`test/project/Rot13/src/Rot13.jl` and `test/sets.jl`, then to 24 entries after
identifier-suffix juxtaposition support removed `test/rational.jl`. Those
remaining #8759 entries then fell to 22 after parametric/interpolated primitive
type support removed `base/Enums.jl` and `test/intrinsics.jl`, then to 20 after
interpolated `for` binding/import name support removed `base/cartesian.jl` and
`stdlib/TOML/src/TOML.jl`, and to 18 after function-head type-expression
parameter support removed `base/iterators.jl` and
`test/testhelpers/OffsetArrays.jl`, then to 17 after macro comma-newline
arguments removed `test/llvmcall2.jl`, and to 16 after do-block vararg
parameter support removed `test/opaque_closure.jl`, then to 15 after
line-leading binary continuation removed `stdlib/Sockets/src/IPAddr.jl`.
Removing seven already-clean stale rows from the same allowlist section brings
it to 8 files, and typed-comprehension newline support removed
`stdlib/Dates/src/parse.jl`, bringing it to 7 files. Typed trailing ncat
separator support removed `test/fastmath.jl`, bringing it to 6 files. The
doc-macro/empty-quote/return-delimiter slice removed `base/shell.jl`, bringing
it to 5 files. Quoted operator-symbol support removed `base/show.jl`, bringing
it to 4 files. Local tuple declarations and macrocall comprehension bodies then
removed `stdlib/Sockets/test/runtests.jl`, bringing it to 3 files. Newline-split
`for` headers and untyped hvncat separators then removed
`test/abstractarray.jl`, bringing it to 2 files. Follow-up quoted-name and
quoted-expression slices then reduced `test/show.jl` from 22 remaining
divergence records to zero and removed it from the allowlist. The remaining
#8759 allowlist scope is now `test/syntax.jl`, which still fails on follow-on
parser families exposed after the initial representative gaps.

## 最新対応 (2026-07-02)

### Float remainder zero-denominator NaN parity (Issues #8895, #8892) — 解決済み

Float-involved `rem` / `mod` / `%` calls now return NaN for zero denominators,
including `Float64 % Int64(0)` and `Int64 % Float64(0.0)`. Pure integer
remainder by zero remains `DivideError`.

### Bare tuple expression statements (Issue #8908) — 解決済み

Statement-level comma tails without `=` now parse as tuple expressions, while
comma tails followed by `=` remain tuple assignments. No remaining parser scope
is tracked under this issue.

### Parser implicit line continuation corpus gap (Issue #8753) — 一部進捗

The parser now handles the issue's representative `import`, arrow-function,
`let`, and `return a,\n b` forms, plus split `export` / `public` lists and
binary / pair operator RHS continuation, multi-line signature defaults, and
final newlines before closing `)` / `}` in parameter and type-parameter lists.
It also handles delimited-context ternary continuation before `?` / `:` and
generator binding continuation after `in` / `=` / `∈`. The remaining #8753
allowlist entries still need reduction: several are true newline-continuation
gaps, while others are separate parser gaps exposed by the corpus sweep. The
separated bare tuple expression statement blocker is resolved under Issue
#8908.

### Parser minor corpus gaps (Issue #8756) — 解決済み

The parser now accepts the #8756 corpus families that upstream Julia parses:
`const global`, `function in(...)`, parenthesized `for`/`while` block
expressions, splatted tuple loop bindings, and const aliases for Unicode
operators. No remaining parser-corpus scope is tracked under this issue.

### Higher-order print/println show dispatch (Issue #8878) — 解決済み

`print` and `println` used as function values now dispatch through
user-defined `show(io::IO, ::T)` in `sprint(print, x)`,
`sprint(println, x)`, and `f = print` / `g = println` IOBuffer calls.

### Module-qualified abstract ancestry collision (Issue #8858) — 解決済み

The fixture harness no longer lets a package-local abstract family share Base
ancestry just because it has the same bare name. `AbstractAlgebra.Set` remains
separate from Base `Set`, so `AbstractAlgebra.Integers{BigInt}` is not treated
as an `AbstractSet` during `show` dispatch.

### `var"..."` non-standard identifier parser compatibility (Issue #8754) — 解決済み

Parser-corpus `var"..."` identifier forms now parse in assignment, call,
quoted-symbol, module-qualified quoted-field, and function-parameter contexts.
The parser merges `var"name"` into a full-span `Identifier` leaf and name
extraction strips the wrapper (`strip_var_quotes`), so lowering, dispatch,
`struct`/`abstract`/`module` names, `:var"..."` symbols, and `Meta.parse`
treat it as an ordinary identifier end-to-end. No remaining scope is tracked
under this issue.

### `where` soft-keyword parser compatibility (Issue #8755) — 解決済み

`where` can now be used as an ordinary identifier outside where-clause
positions. No remaining parser scope is tracked under this issue.

### `Vector{Any}` erased-element dispatch compatibility (Issue #8848) — 未解決

sjulia still treats a plain `Vector{Any}` method parameter as an erased-element
catch-all in method applicability: `f(::Vector{Any}); f(["a"])` reaches the
`Vector{Any}` method, while upstream Julia falls through because array element
parameters are invariant. Issue #8806 fixed ordinary invariant element slots
(`Vector{Number}`, nested `Vector{Complex{Real}}`) but preserved this behavior
as documented workaround W-52 until Base/package broad receiver signatures can
be audited.

### Function definition named `e` (Issue #8852) — 解決済み

Top-level and method-body calls to a user-defined function named `e` now resolve
through the user method table instead of loading stale `Base.MathConstants.e`
global type metadata. The fixture `function_e_name_shadowed_global_8852` covers
direct calls, wrapper-method calls, and a Float64 overload. A bare ASCII `e`
without a user binding remains undefined, matching upstream Julia.

### Nested where binding inside parametric slot (Issue #8853) — 未解決

sjulia can select a method such as `nested_where(x::Box{Wrap{T}}) where
{T<:Number}`, but the method body cannot read `T` and raises
`UndefVarError(:T)`. Simpler top-level parametric bindings (`Box{T}` and
`Complex{T}`) work; the gap is recovering a nested static parameter from inside
another invariant parametric slot for body use.

### Complex Mandelbrot hot loop performance bug (Issue #8796) — 解決済み

`c::Complex` 注釈付き escape loop は runtime specialization と executable block
recognizer の両方で処理される。#8796 に残スコープはない。

### broadcast HOF Complex escape performance bug (Issue #8797) — 解決済み

`mandelbrot_escape.(C, maxiter)` は runtime callable 経由でも `ComplexF64`
executable block を利用する。#8797 に残スコープはない。

### Long-session runtime heap growth bug (Issue #8610) — 解決済み

runtime cache 上限と `ExprArgs` cycle guard を実装済み。#8610 に残スコープはない。

### HCubature non-Float64 endpoint support (Issue #8541) — 解決済み / 性能は #8603

The accepted correctness scope for non-Float64 bundled package endpoints is now
implemented: QuadGK `cachedrule(Float32, 7)` preserves `Vector{Float32}`,
HCubature's one-dimensional Gauss-Kronrod path uses the floated endpoint scalar
type, and BigFloat package execution has the required `float`, `sqrt`,
boxed-array assignment, and StaticArrays copy support. Remaining BigFloat
cubature runtime cost is not a correctness gap and is tracked separately by
Issue #8603.

### Fused Int64 global-slot reads inside typed functions (Issue #8598) — 解決済み

`LoadAddI64` / `LoadSubI64` / `LoadMulI64` / `LoadModI64` now read
module-level globals through the same slot-aware current/global lookup as
`LoadI64`. No remaining scope is tracked for #8598; the fixture
`scope_fused_i64_global_slot_8598` guards the affected fused integer ops.

### Partial-return if inference joins implicit tail (Issue #8600) — 解決済み

Non-final `if` statements with only partial explicit returns now preserve the
fallthrough path as `MaybeReturn`, so later implicit tail values stay in the
inferred function return type. No remaining scope is tracked for #8600; the
fixture `type_inference_partial_return_if_implicit_tail_8600` guards the
behavior.

### Parametric default-constructor reflection re-inference (Issue #8638) — 解決済み

Reflection return-type inference now re-runs the matched body when a fully typed
method uses a parametric default constructor, and the bytecode fallback now
recognizes concrete `NewStruct(...); ReturnAny` tails. This recovers constructor
returns such as `PW9_8638{Float64}`. No remaining scope is tracked for #8638;
the fixture `reflection_parametric_default_ctor_reflection_8638` guards the
typed, untyped, and post-execution query paths.

## 最新対応 (2026-07-01)

### Register VM prototype and cross-target measurements (Issue #8448) — 一部進捗

The #8448 design-decision slice is implemented in
[REGISTER_VM.md](./REGISTER_VM.md): SubsetJuliaVM should pursue a register VM as
the preferred long-term iOS/WebAssembly interpreter shape, while keeping the
current stack VM as the default until measurements justify switching. The first
host-only prototype foundation also exists as `subset_julia_vm::register_vm`,
covering a small straight-line `Int64` stack-bytecode subset plus local metrics.
Remaining #8448 scope: lower real compiled fixtures, run at least one fixture on
host, iOS Simulator, and WebAssembly, and publish bytecode-size,
dispatch-count, VM-only timing, and frame/register-memory comparisons.

### Precise world-age backedge invalidation (Issue #8442) — 一部進捗

The first #8442 slice is implemented: method mutation invalidation now has an
explicit callee-to-cache-key backedge index for return-type, partial-struct,
tentative, limited, and seeded return-cache entries. The index preserves the
existing precise method-edge signature filtering and prunes expired keys after
invalidation. Remaining #8442 scope: persist/reuse CodeInstance-like identity
across the Base cache boundary, replace the `promote_rule`/iterator/dict-view
Base-cache bypasses with targeted invalidation, and add coverage for method
deletion if/when deletion is supported.

### SSA IR optimization layer (Issue #8440) — 一部進捗

The SSA pipeline is now the default (Issue #8832 default flip; set
`SJULIA_SSA_PIPELINE=0` for the legacy path). The durable representation
(`SsaFunction`/`SsaBlock`/`SsaValue`/`PhiNode`), full optimization passes
(constant folding/DCE/CSE), phi-copy coalescing, and stack-bytecode lowering
are implemented. The temporary `ir_opt` bridge (`fold_identical_branch_assignments`)
has been retired. Remaining scope: lift the opaque-barrier constructs
(`for`/`try`/closures/destructuring) and the runtime-specializer fallback
(source-named slots needed).

### Parser default `::Type` argument in where signatures (Issue #8514) — 解決済み

Short-form and block-form signatures such as
`f(v::Val{N}, ::Type{T}=Float64) where {N,T<:Real}` now parse and run with both
the omitted default type argument and an explicit type argument.

### Typemap matcher migration (Issue #8438) — 一部進捗

Callable-value dispatch now uses the shared CoreType-native signature matcher
for `where`-candidate diagonal and explicit-bound gates, so the local
callable-value diagonal/bounds helper predicates are gone. `Type{Any}` singleton
matching is also exact in the shared JuliaType/CoreType matchers, so the
transitional non-exact scoring penalty and duplicate helper predicates are gone.
Remaining #8438 scope: design the overlay method-table shape, retire the
production `Bottom` placeholder path, and continue replacing score-first
selection with an upstream-compatible typemap/morespecific ordering.

### Eliminate hand-coded subtyping and upstream divergence (Issue #8439) — 解決済み

The accepted #8439 scope is implemented: the upstream-comparison subtype
harness is in place, production dispatch no longer accepts unknown struct names
as subtypes of arbitrary abstract bounds, and the former
`morespecific` case-H divergence is inverted to assert upstream parity. Broader
subtype algorithm unification can continue as future incremental parity work.

### Constant propagation beyond literal arithmetic (Issue #8443) — 解決済み

The accepted #8443 slice is implemented: optimized `f(41)` add-one calls fold
to an immediate integer return, pure constant tuple returns remain tuple-literal
bytecode, and literal `typeof` calls fold to static `DataType` objects. Broader
SSA-IR/interpreter-style constant propagation remains tracked by Issue #8440.

### HCubature full upstream parity (Issue #8524) — n-D 解決済み / non-Float64 は #8541 で解決済み

Generic n-dimensional cubature is now supported (Issue #8524): Genz-Malik point
generation uses upstream's `combos`/`signcombos` (Combinatorics + gray-code sign
flips) for any `n >= 2`, endpoint conversion builds `SVector{n,F}` for arbitrary
`n`, and the adaptive subdivision / `initdiv` loops are dimension-generic
(`packages_hcubature_ndim_8524` covers 3-D/4-D/5-D integrals and 3-D/7-D
Genz-Malik evaluation counts). The former non-Float64 endpoint follow-up
(`Float32`, `BigFloat`) is resolved by Issue #8541; BigFloat cubature
performance remains tracked by Issue #8603. The `length(a)` dimension
workaround (W-51) was retired when Issue #8539 fixed value type parameters in
call-argument and range-endpoint positions.

### `include(...)` eval with `using` statements (Issue #8474) — 解決済み

Runtime eval now accepts the `usingstatement` Expr head produced while
evaluating included files. `sjulia -e 'println(include("file.jl"))'` works when
the included file contains `using LinearAlgebra` followed by a final expression.

## 最新対応 (2026-06-30)

### iOS full test suite restarts (Issue #8489) — 解決済み

The iOS FFI bridge now tolerates older native result struct layouts by using
optional C accessors for post-prefix artifact fields, and normal iOS unit tests
skip sample performance benchmarks unless `SJULIA_IOS_PERF_TESTS=1` is set. The
full iOS Simulator unit suite completes without app restarts.

### AoT CodeInstanceKey / InferenceCacheKey split (Issue #8372) — 解決済み

AoT specialization state still keeps `StaticType` arguments for backend layout,
but its cache identity is now the shared compile-side `InferenceCacheKey`.
Literal call-site keys are built from `CacheArgType` using the same
const-specialization policy as compile inference, so no separate AoT
`CodeInstanceArgKey` scope remains for this issue.

### Base cache schema/version invalidation (Issue #8444) — 解決済み

The Base cache now carries schema and compiler-build fingerprints and rejects
same-version stale schema payloads before decoding bytecode sections. The
schema fingerprint snapshot is pinned to `CACHE_VERSION` by
`scripts/audit_base_cache_schema_fingerprint.sh`; CI wiring for that audit is
tracked by Issue #8491 because this automation token cannot update workflows.
The broader `promote_rule`/iterator-hook extension fallback is still
intentionally tracked by the precise invalidation work in Issue #8442.

### Parser diagnostic quality baseline (Issue #8454) — 解決済み

Parser errors now expose stable line/column span text, multi-line source
context underlining, and recovered multi-error formatting through
`ParseErrors::format_all`. Broader Julia diagnostic wording parity can continue
incrementally, but the issue's baseline parser diagnostics gap is closed.

### iOS code completion state crash (Issue #8487) — 解決済み

Code and Unicode completion state no longer store stale `String.Index` ranges
or rely on the crashing `@Published` teardown path. The focused
`CodeCompletionProviderTests` suite now completes on iOS Simulator without the
`SubsetJuliaVMApp quit unexpectedly` crash.

### FFI structured result inspection (Issue #8455) — 解決済み

Native FFI detailed/streaming results now expose typed value JSON, stable C
value tags, direct complex/array accessors, dictionary entry JSON accessors,
artifact accessors, and an audited C/C++ header. No remaining scope is tracked
for the #8455 C ABI header-completeness slice.

### WASM typed result API (Issue #8456) — 解決済み

The web API now exposes `typed_value` on execution results plus
`run_from_source_typed`, with array/complex/artifact regression coverage.
Implemented macro definitions are no longer listed as unsupported.

### iOS sample catalog consistency (Issue #8457) — 解決済み

The iOS sample catalog is validated against Swift category/difficulty enums,
backing `.jl` files, and README counts. The current catalog is 38 samples across
9 categories.

### Cached Base with user-main block-local functions (Issue #8469) — 解決済み

Programs that define local functions inside the user main block, including
macro bodies such as `@testset`, now bypass cached Base so block-local methods
remain visible during compilation. The `kwargs_typed_default_preservation`
cached-only failure has no remaining scope.

### Clippy all-targets Function literal build (Issue #8468) — 解決済み

The test-only synthetic `Function` initializers now include the runtime-eval
flag, so the all-targets clippy gate has no remaining build gap from the
world-age IR field addition.

### LinearAlgebra in-place Cholesky work state (Issues #8411, #8465) — 解決済み

The in-place factorization fixture now follows upstream Julia expectations for
`eigen!`, `eigvals!`, and failing `isposdef!`. `cholesky!` / `isposdef!` leave
the attempted upper-Cholesky work matrix on the covered failure path, so there
is no remaining gap for the #8411 fixture blocker or the #8465 MWE.

### `sjulia -e include(...)` expression-position file paths (Issue #7766) — 解決済み

Native `sjulia -e 'println(include("/tmp/file.jl"))'` now evaluates the file via
the Base `include(path)` / `evalfile(path)` path instead of reporting a
sandboxed lowering-time include error. iOS/WASM still intentionally restrict
runtime filesystem access to embedded package/include registries.

### QuadGK focused milestone slice (Issue #8140) — 対応済み範囲を拡張

The bundled upstream QuadGK source now covers the milestone scalar slice plus
Gauss/Kronrod rule construction and finite-domain buffer APIs: finite scalar
`quadgk`, `cachedrule(Float64, 7)`, `gauss(Float64, 3)`, interval-rescaled
`gauss(Float64, 3, 0.0, 2.0)`, `kronrod(Float64, 3)`, finite multi-domain
calls, semi-infinite scalar intervals, `quadgk_segbuf`/`eval_segbuf`,
`segbuf=` reuse from a concrete segment buffer, `quadgk!`, and direct
`BatchIntegrand` calls with keyword forwarding.

Remaining broader QuadGK scope: two-sided infinite intervals, complex and
broader vector-valued integrands, weighted rules, broader Kronrod/Gauss
construction orders and element types, tighter tolerance edge cases, and full
upstream API parity beyond the focused fixture surface.

### OrdinaryDiffEq Tsit5 keyword solve dispatch (Issue #8396) — 解決済み

The current OrdinaryDiffEq linear-solve fixture now dispatches
`solve(prob, Tsit5(); kwargs...)` through the resolved method table, including
keyword-vararg methods. Broader SciML solver API parity remains outside this
fixture slice.

### Symbolics Num Dict-key lookup (Issue #8397) — 解決済み

`Dict` lookup through imprecise receivers now handles struct keys such as
`Symbolics.Num`, so the current `packages_symbolics_substitute` fixture no
longer has a remaining Dict-key scope.

### AbstractAlgebra YoungTableau linear indexing (Issue #8400) — 解決済み

The bundled YoungTableau MVP now supports linear indexing through
`getindex(::YoungTableau, ::Integer)`. Broader Young tableaux algorithms remain
limited to the existing MVP surface.

### AbstractAlgebra alias where-bound dispatch (Issue #8406) — 解決済み

Alias bounds such as `RingElement` in `where` clauses now participate in method
matching for the bundled polynomial MVP fixture. This closes the current
`packages_abstract_algebra_poly_mvp_7491` gap without expanding full upstream
AbstractAlgebra polynomial coverage.

### AbstractAlgebra union-alias typevar binding (Issue #8409) — 解決済み

The fraction/residue MVP no longer fails with `UndefVarError: T not defined`
when a union alias admits a user-defined ring element type. Broader
AbstractAlgebra fraction-field parity remains outside the fixture scope.

### Typed Matrix(::SymTridiagonal) materialization (Issue #8395) — 解決済み

The public typed matrix constructor `Matrix{Float64}(::SymTridiagonal)` now
materializes a dense `Matrix{Float64}` through normal Julia constructor dispatch.
Broader LinearAlgebra structured-matrix coverage remains limited to the existing
fixture surface.

### AbstractAlgebra module-private type objects (Issue #8410) — 解決済み

Runtime-specialized method bodies now resolve module-private type objects such
as `AbstractAlgebra.AAPerm`. This closes the current permutation MVP fixture gap;
the broader permutation scope remains as documented for Issue #8306.

### Dynamic DataType call-site return inference (Issue #8414) — 解決済み

Calls through `Any`-returning methods that branch on runtime `DataType` values
now keep their runtime result instead of being narrowed to `nothing`. This closes
the `fit_mle(Binomial, 5, data)` regression covered by
`distributions_fit_suffstats_7326`.

### DataStructures heap helper validation (Issue #8365) — 解決済み

The QuadGK dependency slice for bundled DataStructures array-backed heap
helpers now has explicit validation coverage in
`packages_data_structures_heap_validation_8365`. The fixture covers
`heapify!`, `heapify`, `heappop!`, `heappush!`, `isheap`,
`percolate_down!`, and `percolate_up!` for both `Forward` and `Reverse`
orderings, including the bounded `Reverse` active-prefix percolation used by
QuadGK batch refinement. No broader DataStructures collection types were added.

### Base / Parser / Lowering milestone 55 gaps (Issues #8298, #8300, #8301, #8303, #8304, #8305, #8307, #8313) — 解決済み

The milestone 55 parser/lowering/Base gaps are covered by the new
`milestone55::chunk_000` fixture category. The resolved slice includes Unicode
superscript identifier suffixes, statement-position `@views` / `@.`,
statement-position `Base.@pure`, field broadcast assignment, multiline return
tuple continuation, the public `Matrix` constructor, and bare calls to imported
parametric inner constructors.

### AbstractAlgebra.Generic Young diagram namespace (Issue #8302) — 解決済み

The bundled `AbstractAlgebra` now exposes `AbstractAlgebra.Generic.Partition`
and `AbstractAlgebra.Generic.YoungTableau` as qualified aliases for the
iOS-safe Young diagram/tableau MVP. The original `Generic.Partition` issue MWE
is covered by `packages_abstract_algebra_young_tableau_mvp_8302`.

## 最新対応 (2026-06-29)

### iOS AbstractAlgebra.jl sample (Issue #8295) — 解決済み

The iOS app now ships an `AbstractAlgebra.jl` sample that exercises the bundled
MVP package surface for polynomial rings, polynomial quotient rings, residue
rings, exact dense matrices, permutation groups, and Young diagrams/tableaux.
It is registered in both the resource-backed sample catalog and the Swift
fallback catalog. Known residual scope for broader AbstractAlgebra API coverage
remains tracked by the existing AbstractAlgebra phase issues; #8295 has no
remaining sample-catalog scope.

### AbstractAlgebra polynomial residue rings (Issue #8299) — MVP 対応済み

The bundled package now supports the iOS sample's quotient-ring shape
`R, x = polynomial_ring(ZZ, :x); Q, alpha = residue_ring(R, x^2 + x + 1)`.
The MVP covers monic polynomial moduli, generator arithmetic, reduction,
`data`/`lift`, `modulus`, display, and scalar coercion.

Remaining broader quotient-ring scope:

- Non-monic polynomial moduli, canonical maps, ideal-backed constructors,
  broader coefficient rings, exact division through non-unit leading
  coefficients, and full upstream Generic residue ring integration.
- The previous unsupported-feature gap #8299 is closed for the iOS sample MVP,
  but not for the complete upstream AbstractAlgebra quotient-ring surface.

### AbstractAlgebra permutation groups (Issue #8306) — MVP 対応済み

The bundled package now supports the iOS-safe permutation group basics from the
upstream docs: `SymmetricGroup`, `Perm([..])`, composition, inverse, powers,
`sign`, `parity`, `permtype`, parent metadata, generator count, and cycle-style
display. Covered by `packages_abstract_algebra_perm_mvp_8306`.

Remaining broader permutation scope:

- Public `Perm{T}` element type parity is not exposed yet; the bundled package
  uses an internal `AAPerm` element for MVP stability. The VM/simple-name
  imported parametric inner constructor gap previously tracked by #8313 is now
  resolved.
- Callable parent coercion (`G([..])`), full validation/coercion semantics,
  `AllPerms`, permutation strings/macros, random permutations, and deeper
  Generic/character-table integration remain outside this MVP.

### AbstractAlgebra Young diagrams/tableaux (Issue #8302) — MVP 対応済み

The bundled package now supports the iOS sample's Young diagram/tableau shape:
`Partition([4, 2, 1, 1, 1])` and `YoungTableau([4, 3, 1])`, including partition
metadata, tableau shape, docs-compatible linear indexing, equality, and compact
ASCII diagram helpers. Covered by `packages_abstract_algebra_young_tableau_mvp_8302`.

Remaining broader Young tableaux scope:

- Full upstream `Generic.YoungTabs` algorithms beyond the qualified MVP aliases:
  Unicode boxed display, `conj`, `partitionseq`, `AllParts`, skew diagrams,
  `matrix_repr`, `fill!`, hook lengths, dimensions, and character computations.
- `length(::Partition)` currently remains outside the sample MVP because the VM
  routes that AbstractVector fallback through an internal typed slot path; use
  `size(p)` in sample code.

### LinearAlgebra stdlib module loading (Issue #8276) — 解決済み

`LinearAlgebra` is handled as a root stdlib module loaded by `using
LinearAlgebra`, matching upstream Julia. Public `Base.LinearAlgebra` access is
now rejected; LinearAlgebra's bundled wrappers reach VM numerical kernels only
through a private compiler bridge. `det`, `inv`, `svd`, and `eigen` smoke tests
cover the stack-overflow regression. Known residual scope for broader
LinearAlgebra API completeness remains tracked by existing linalg issues.

### QuadGK / DataStructures package support (Issues #8140/#8141) — scalar slice closed

QuadGK が依存する DataStructures の array-backed heap helper MVP は実装済みで、
`packages_data_structures_heap_8141` と
`packages_data_structures_quadgk_segment_heap_8141` が upstream Julia と
sjulia の両方で通る。QuadGK v2.11.3 の `DataStructures` 参照は
`heapify!`/`heappop!`/`heappush!` と bounded
`percolate_down!`/`percolate_up!` に限定されるため、#8141 の QuadGK
dependency slice は完了。

QuadGK.jl 本体は upstream source を bundle し、
`packages_quadgk_scalar_integrals_8140` で有限区間 scalar integration
(`quadgk(x -> x^2, 0.0, 1.0)`, `quadgk(sin, 0.0, 1.0)`) と
`cachedrule(Float64, 7)` が upstream Julia / sjulia の両方で通る。2026-06-30
時点で `kronrod(Float64, 3)`, finite multi-domain calls, semi-infinite scalar
intervals, `quadgk_segbuf`/`eval_segbuf`, concrete `segbuf=` reuse, `quadgk!`,
and direct `BatchIntegrand` keyword dispatch are also covered by package
fixtures.

残スコープ:

- QuadGK.jl の broader surface: two-sided infinite intervals, complex and broader
  vector-valued integrands, weighted rules, broader generated Gauss/Kronrod rule
  construction, tighter tolerance edge cases, and broader API parity beyond the
  focused finite/semi-infinite/buffer/batch fixture path.
- DataStructures.jl 全体 (#8141) の broader collection surface:
  `BinaryHeap`/`MutableBinaryHeap`/priority queue、ordered collections、
  deque/stack/queue、accumulators、Fenwick/disjoint-set/trie/sorted containers
  など。現時点では QuadGK が使う flat-array heap helpers に限定。

### Nested module and Base.Order binding (Issue #8269) — 解決済み

Nested submodule binding and `Base.Order` direct access/import are implemented
and covered by `modules_nested_module_order_binding_8269`。既知の残スコープなし。

### AbstractAlgebra Phase 3/4 seed (Issues #7489/#7490) — 一部対応

`ZZ`/`QQ` parent objects and the core trait/exact-arithmetic seed needed by the
MVP are implemented and covered by `abstract_algebra_core_traits_7489_7490`.
The broader Phase 3/4 issue scope remains open for the next tranche: full
upstream `Attributes` restoration, additional `AbstractAlgebra` trait surface,
complete `Integer.jl`/`Rational.jl` algorithms, and downstream polynomial
integration.

Known active VM/package gaps discovered during this tranche:

- #8253: `Rational{T}(x)` can construct malformed `Rational{BigInt}` values
  when `T` comes from a parametric method.
- #8254: same-module `const` function aliases such as `is_zero` are not visible
  inside later method bodies.
- #8255: rational-over-rational `//` fails in sjulia.
- #8256: package-defined `Base.showerror` methods are ignored for custom
  exceptions.

### AbstractAlgebra Phase 5 polynomial/fraction/residue MVP (Issue #7491) — 一部対応

Univariate dense polynomial rings over `ZZ`/`QQ` are implemented for the MVP
fixture surface (`polynomial_ring`, arithmetic, display via direct `show`,
degree/coeff/evaluate/derivative/divexact). Constructor-level fraction/residue
support is also present for `fraction_field(::GenericPolyRing)` and
`residue_ring(ZZ, n)`, with small arithmetic covered by
`packages_abstract_algebra_fraction_residue_7491`. Remaining Phase 5 scope:

- Full upstream `Poly.jl` / `generic/Poly.jl` / `SparsePoly.jl` coverage,
  including sparse representation, gcd/content/primitive-part algorithms, and
  broader coercion/promotion behavior.
- Full fraction-field simplification/coercion, polynomial gcd-backed reduction,
  and callable parent construction (`F(num, den)` is tracked by #8264).
- Broader residue rings/residue fields beyond integer modulus rings, quotient
  maps, inverses, and zero-divisor/annihilator algorithms.
- Display routing through `println`/`string` for custom polynomial `show`
  remains blocked by #8263.

### AbstractAlgebra Phase 6 matrix/module/map MVP (Issue #7492) — 一部対応

Dense matrix spaces over the current MVP rings are implemented for the fixture
surface (`matrix_space`, `matrix`, `zero_matrix`, `identity_matrix`, indexing,
arithmetic, transpose, determinant, trace, and small rank). A small free-module
and map layer is also present (`free_module`, module generators/arithmetic,
`identity_map`, `hom`, `domain`, `codomain`) and covered by
`packages_abstract_algebra_matrix_module_map_7492`. Remaining Phase 6 scope:

- Full upstream `Matrix.jl` / `generic/Matrix.jl` support, including typed
  dense storage restoration after #8266, views/slices, mutation APIs,
  diagonal/block constructors, normal forms, solving, nullspace, and broad
  `rank` beyond the MVP small-matrix cases.
- Full `MatRing.jl`, matrix-ring elements, and richer integration with
  LinearAlgebra algorithms beyond determinant/trace/small rank.
- Full `Module.jl`, `FreeModule.jl`, submodule, quotient-module, and module
  homomorphism behavior, including kernels/images and relation handling.
- Full `Map.jl`, `MapCache.jl`, `MapWithInverse.jl`, and advanced map
  composition/inverse/cache semantics.
### Open bug sweep: Rational / BigInt / display / callable dispatch (Issues #8253, #8254, #8255, #8256, #8262, #8263, #8264, #8266) — 解決済み

Rational parametric constructors, same-module function aliases, Rational-over-Rational `//`,
package-defined `showerror`/`show`, BigInt array storage, and abstract-parent callable object
dispatch は対応済み。既知の残スコープなし。

### array-like wrapper equality / inference drift prevention (Issue #8246) — 対応済み

`view` / `reshape` などの array-like wrapper が compile-time 推論と runtime equality
normalization でずれる問題は、#8240 MWE と非 `SubArray` wrapper を含む contract fixture、
`view(Vector, UnitRange)` の concrete `SubArray` 推論 unit、CHECKLISTS の guardrail で
予防する。既知の残スコープなし。

### SubArray view equality (Issue #8240) — 解決済み

`view(Vector{T}, UnitRange) == view(...)` / `view == Vector` / `Vector == view` は
1D contiguous `SubArray` を logical array view に正規化して要素比較するようになった。
この issue に残スコープなし。2D 以上の `SubArray` equality 拡張は、必要になった時点で
別 issue として扱う。

## 最新対応 (2026-06-27)

### Rotations.jl の MVP 外 surface (Issue #7434, milestone #38) — 残スコープ

Rotations.jl MVP ([ROTATIONS.md](./ROTATIONS.md)) は決定的な型・演算サーフェスのみ
対応し、以下は意図的に未対応:

- ~~**`Base.getproperty` オーバーロード全般 (#8127)**~~: **対応済み (Issue #8127)**。
  構造体の `x.f` は、その型に対しユーザー定義 `getproperty` が dispatch される場合
  `getproperty(x, :f)` 呼び出しへ経路付けされ、計算プロパティ・宣言フィールド双方が
  override 経由で解決される(specializer の直接 `GetField` も override 検出時は抑止)。
  `QuatRotation` の `.w/.x/.y/.z` 実フィールド化は引き続き MVP の選択(置換不要)。
- **StaticArrays スカラ除算（非正方/大型行列, #8125）**: 実行時パラメータ
  `SMatrix{M,N}` コンストラクタが必要。ベクトルと正方 2×2/3×3/4×4 は対応済み。
- **StaticArrays Phase 4–5 の保留範囲 (#7460/#7461)**: `adjoint`/`A'`（#8132、
  実 MVP では `transpose` を使用）、型一致する `collect`/`Array`/`Vector`（#8131、
  値は正しいが `Vector{Any}` になる）、`det` 4×4 以上・`inv` 3×3 以上（LU 分解が必要）、
  非インライン形状の行列 broadcast/map/transpose（#8125）。`MArray`/`MVector`/
  `MMatrix`（可変）、`SizedArray`、`FieldArray`、分解系 LinearAlgebra（lu/qr/eigen/
  svd/cholesky）、拡張・網羅的上流テストは post-MVP。
- **`RotMatrixGenerator`**（dense SMatrix ジェネレータ）とジェネレータの exp/log マップ。
  `Angle2dGenerator`/`RotationVecGenerator`/`skew`/`isrotationgenerator` は対応済み。
- **exp/log マップと回転誤差演算**: `expm`/`logm`/`⊖`/`⊕`/`error_maps.jl`/
  `rotation_error.jl`。
- **全 Euler 順序変種** (`RotXYZ`/`RotZYX`… 2軸・3軸合成)。単軸 `RotX/Y/Z` のみ対応。
- **`rotation_between` の N 次元** (N≠2,3) — SVD が必要。
- **乱数コンストラクタ** (`rand(RotMatrix)` 等) — 広範な `Random` 分布パリティ依存。
- **`eigen`/分解パリティ**、**`nearest_rotation`**、quaternion 構築経路を超える
  **`principal_value`**。
- **ForwardDiff 微分パリティ**、**RecipesBase プロット**、**Unitful** 要素型。

### Optim.jl の MVP 外 surface (Issue #7432, milestone #39) — 残スコープ

Optim.jl MVP ([OPTIM.md](./OPTIM.md)) は決定的な no-AD / ユーザ勾配ワークフローのみ
対応し、以下は意図的に未対応:

- **完全 AD**: ADTypes/ForwardDiff（`ADTypes` は marker type stub のみ）。`BFGS` の
  no-gradient 経路は中心差分 (`autodiff = :finite`) で対応済み (Issue #8059)。
- **準ニュートン**: `BFGS` は対応済み (Issue #8059, [OPTIM.md](./OPTIM.md))。残りは
  `LBFGS` / `ConjugateGradient` / `AcceleratedGradientDescent` /
  `MomentumGradientDescent` / `Adam` / `AdaMax` / `NGMRES`。
- **二次**: `Newton` / `NewtonTrustRegion` / `KrylovTrustRegion` と Hessian 経路。
- **制約付き**: `Fminbox` / `IPNewton` / `SAMIN` / `LBFGSB`。
- **確率的**: `ParticleSwarm` / `SimulatedAnnealing`。
- **完全 LineSearches**: `MoreThuente` / `StrongWolfe` と残りの initial-step guesser。
  `HagerZhang` + `InitialStatic`（BFGS 既定）と `BackTracking`（GradientDescent）は対応済み。
- **trace**: `store_trace` / `show_trace` 履歴と `x_trace` / `f_trace` /
  `simplex_trace` 等のクエリ。
- `MathOptInterface` 拡張、`SparseArrays` 固有経路、上流テスト完全パリティ。

## 最新対応 (2026-06-26)

### AbstractAlgebra macro/lowering Phase 2 driver (Issues #7723/#7488) — 解決済み

`AliasMacro.jl` / `Aliases.jl` / `Assertions.jl` / `Attributes.jl` /
`AbstractTypes.jl` / `ConcreteTypes.jl` は bundled `AbstractAlgebra` の include order で
parse/lower/compile できる。`@req` macro import、`PolynomialElem` / `MatrixElem` type
aliases、macro-expanded `UniversalRing`、`MatSpace` は `using AbstractAlgebra` 後に解決し、
`names(AbstractAlgebra)` / `isdefined(AbstractAlgebra, ...)` の driver gate も通る。

### AbstractAlgebra attribute/runtime follow-up gaps (Issue #7948) — 残スコープ

Phase 2 package-load driver から外した runtime semantics は個別 issue に分離した。
未実装のまま追跡するのは macro bindings の `isdefined(::Module, Symbol("@..."))`
reflection (#7948) のみ。

VM レベルでは解決済み: `@attributes Type` branch の quoted typed-parameter
interpolation (`f(x::$T) = ...`) は #7933、typed `Dict{...}` constructors with
DataType params (#7934)、`UniversalRing` inner constructor の dynamic `new{...}`
parameters (#7935)、generic `DataType` Dict keys (#7940)、guarded generic
field assignment (#7941) は #7940/#7941/#7935 修正 (DONE.md 2026-06-27)。bundled
AbstractAlgebra のパッケージ stub (`Attributes.jl` / `ConcreteTypes.jl`、
WORKAROUNDS.md W-28..W-31) は Phase 2 restoration 一括作業に残置 (#7940 のパッケージ
復元は別バグ #8068 の module-global Dict getindex が必要)。

### macro-expanded `Expr(:struct, ...)` definitions (Issue #7915) — 解決済み

macro-returned `Expr(:struct, mutable, header, body)` は top-level / module body lowering が
`StructDef` として回収し、call-site の `Program.structs` / `Module.structs` に登録する。
関数 body など非 top-level から出る struct definition は引き続き unsupported。

### package entry の no-op top-level doc statement (Issue #7913) — 解決済み

package entry file の module 宣言前にある `@doc raw"""..."""` は `Base.@doc(str)` の
documentation no-op として `nothing` に lower され、loader は top-level
`Literal::Nothing` header だけを許容するようになった。effectful な top-level
statement は package layout violation として引き続き拒否する。

### Macro-returned where bound の TypeVar 参照 (Issue #7924) — 解決済み

macro-returned `where` 型の bound 内だけに出る先行 TypeVar 参照も `UnionAll`
参照判定に含めるようになった。`Tuple{S} where {T, S<:T}` に残スコープなし。

## 最新対応 (2026-06-25)

### Quoted export statements in macro-returned blocks (Issue #7908) — 解決済み

`quote ... export $name ... end` は `Expr(:export, ...)` として roundtrip し、
macro-return statement path で `Stmt::Export` へ戻る。この unsupported-feature に
残スコープなし。

### AbstractAlgebra macro/lowering gates (Issue #7488) — 残スコープ

#7486 で upstream 0.50.1 の dependency/include map と parse-only seed baseline、
#7487 で bundled `AbstractAlgebra` skeleton と dependency shims は追加済み。
残スコープは `AliasMacro.jl` / `Aliases.jl` / `Attributes.jl` などの
nested quote、`esc`、macrocall lowering、module macro hygiene を通す Phase 2 (#7488)。

### Macro-returned quote Expr.args arrays (Issue #7898) — 解決済み

`Expr(:quote, ex.args)` を返す macro は `Expr.args` array-ref を `Vector{Any}` 相当の
array literal として再構築できるようになった。この unsupported-feature に残スコープなし。

### LinearAlgebra dispatch-first splat module calls (Issue #7896) — 解決済み

`LinearAlgebra.det((A,)...)` / `LinearAlgebra.lu((A,)...)` は dispatch-first shortcut でも
元の splat masks を保持するようになった。この bug に残スコープなし。

### OrdinaryDiffEq README MVP follow-up phases (Issue #7865) — 残スコープ

Issue #7362 で `using OrdinaryDiffEq`、`ODEProblem` / `ODESolution` の skeleton、
`Tsit5()` algorithm object は bundled package として解決可能になり、Issue #7363 で
`solve` backend と `ODESolution` 生成、Issue #7364 で `plot(sol)` /
`plot(sol, idxs=(1,2,3))`、Issue #7365 で iOS/Web/Flutter sample 登録、Issue #7366
で supported API / parity policy / completion fixture、Issue #7367 で adaptive Tsit5
backend、Issue #7360 で親 MVP 完了記録も追加済み。milestone 33 の README
visualization MVP に残スコープはない。
callbacks/events・dense output・StaticArrays・full Plots recipe pipeline などの
non-MVP gap は #7865 で追跡し、個別の実装 Issue へ昇格済み: integrator interface
(#7981, **部分実装**: `init`/`step!`/`solve!`/`reinit!`/`remake`/`successful_retcode`
済み、`step!(dt)`/任意 `tstops`/`ReturnCode` enum 残)、dense output / `sol(t)`
(#7982, **部分実装**: 線形補間 `sol(t)`/`sol(t; idxs=...)`/`sol(ts)` 済み、Tsit5 4 次
dense interpolant 残)、callbacks & events (#7983, **部分実装**: `DiscreteCallback` /
`ContinuousCallback`（bisection）/ `CallbackSet` 済み、`VectorContinuousCallback` /
adaptive 経路 / `save_positions` 残)、StaticArrays variant (#7984)、
`SecondOrderODEProblem` / symplectic (#7985, **部分実装**: `SecondOrderODEProblem` +
`VelocityVerlet` 済み、`ArrayPartition` / 高次 symplectic / refined examples 残)、
broader array surfaces (views/sparse, #7986, **view は #8003 でブロック**)、full Plots
recipe pipeline (#7987)。詳細は
`docs/vm/ORDINARYDIFFEQ.md` の "Promoted Follow-up Issues" を参照。

### Generated staging completion (Issues #7722/#5074) — 部分解決、残スコープあり

#7722 で generated body の型引数返却、vararg 型 tuple 返却、`Val{N}` からの
ntuple 風 staged tuple Expr unroll は upstream Julia と一致するようになった。
通常 runtime value specialization は generated method へ適用しないため、generated
body の `x` が `typeof(x)` を指す既存フレーム規約と compiler の戻り型推論が衝突しない。

残スコープは #5074 の full generated staging / purity audit / world-age invalidation であり、
既存の #4271/#5603 cache invalidation 系の設計に継続して載せる。

### Base macro runtime unification (Issue #7721) — 解決済み

Base registry macro は user macro と同じ `macro_runtime` path で expansion-time 実行する。
legacy `substitute_params_in_macro_expr` 経路は削除済み。Base bootstrap 前に必要な
構造 macro / metadata macro / `@view` / `@views` / multi-argument `@show` の Rust kernel は
意図的に残す bootstrap slice であり、Issue #7721 に残スコープなし。
validation 中に見つかった macro-return lowering / nested macro lookup / Base macro statement
value gaps (#7763/#7764/#7765/#7767/#7769/#7771/#7773/#7775/#7778/#7779/#7780/#7786/#7790/#7794/#7798)
も解決済み。

### Macro statement-position block tail definitions (Issue #7805) — 解決済み

statement position の macro-returned outer block が `Expr(:function, ...)` /
`Expr(:+=, ...)` で終わる場合は statement path に戻して lowering する。#7764 の
value-producing tail preservation は維持済みで、本 Issue に残スコープなし。

### Meta.parse parser-internal head roundtrip gaps (Issue #7754) — 解決済み

#7720 の roundtrip gate 追加中に、`Meta.parse` が parser-internal head を
表示・`eval`・macro-return lowering 経路へ漏らす gap を確認した。#7753 で
`:prefixedstringliteral` / keyword arg head / QuoteNode 表示を解決し、#7755 で keyword call の
runtime `eval` / macro-return lowering seed を gate 化した。#7754 では
`:letexpression` / `:letbindings` / `:elseclause` / `:elseifclause` を upstream 形の
`Expr(:let)` / `Expr(:if)` / `Expr(:elseif)` に正規化し、let / if-else / if-elseif-else の
eval・macro-return seed を `scripts/check_metaprogramming_roundtrip.sh` へ追加済み。

### Metaprogramming roundtrip gate (Issue #7720) — 解決済み

`scripts/check_metaprogramming_roundtrip.sh` が upstream `julia` と sjulia の seed corpus
pass/fail summary を比較する。source printing / runtime eval / macro-return lowering の
現在対応済み seed は gate 化済みで、Issue #7720 に残スコープなし。parser-internal head の
追加 seed も #7754 で gate 化済み。

### Expr head registry and metaprogramming bug tail (Issues #7719/#7696/#7676) — 解決済み

quoted AST construction、macro-return lowering、runtime `eval` の `Expr` head
dispatch は `ExprHead` registry で管理される。`Dict(:a => 1)` の `QuoteNode(:a)`
表示と、macro argument context の `var"@q"` Symbol 化は fixture で固定済み。
Issue #7719 / #7696 / #7676 に残スコープなし。

### Module-qualified inner constructors and escaped Base-extension calls (Issue #7631) — 解決済み

Module-qualified constructor calls such as `Plots.Animation()` can reach the
defining module's inner constructor method table. Escaped caller code emitted by
bundled package macros no longer qualifies `Base` extension methods as package
members, so REPL `@gif` loops using `push!(ps, p)` dispatch to array mutation.
Issue #7631 に残スコープなし。

### Assignment free vars same-statement shadowing (Issue #7685) — 解決済み

Assignment targets are registered as locals before RHS free-var analysis for
simple `Stmt::Assign` targets. Same-statement local shadowing no longer captures
an outer/global binding, so Issue #7685 に残スコープなし。

### MacroTools TypeBind Set splat matching (Issue #7670) — 解決済み

`Set{Any}([:call])...` は VM の function-call splat 展開で Set の要素へ展開される。
MacroTools `TypeBind` success path は `Expr(:call, ...)` を env に束縛するため、
Issue #7670 に残スコープなし。

### MacroTools upstream utils fixture mismatches (Issue #7647) — 解決済み

MacroTools upstream `test/utils.jl` v0.5.16 の typed/where `isdef`、
block/try `flatten`、`animals` ordering、`@qq` line-number check は restored fixture
として通る。dedicated `flatten_try.jl` の `eval(Expr(:try))` cases も
Issue #7683 の eval support で復元済みのため、Issue #7647 に残スコープなし。

### Top-level begin assignments in assignment RHS (Issue #7667) — 解決済み

Top-level `x = begin y = ...; value end` は `y` を global binding として残し、
`x` には block の最後の値を代入する。function 内でも `begin` は新しい scope を
作らず surrounding local scope に代入を残すため、Issue #7667 に残スコープなし。

### Any-typed infix equality dispatch after macro blocks (Issue #7643) — 解決済み

macro-generated nested block assignment 後に `Any` として読まれる struct 値でも、
infix `s == S("foo")` は user-defined `==(::S, ::S)` へ runtime dispatch する。
MacroTools upstream destruct fixture は function-call equality workaround を除去済みで、
Issue #7643 に残スコープなし。

### MacroTools selective `striplines` import visibility (Issue #7645) — 解決済み

`using MacroTools: striplines` は unqualified `striplines(ex)` を利用可能にする。
upstream utils fixture の rmlines smoke は bare `striplines` 呼び出しに復元済みで、
Issue #7645 に残スコープなし。

### MacroTools animals qualified constant lookup (Issue #7646) — 解決済み

`MacroTools.animals` は module-qualified function binding ではなく、
package data から生成された `Vector{Symbol}` constant として解決される。
upstream utils fixture は `Vector{Symbol}` / length / endpoint smoke を復元済みで、
Issue #7646 に残スコープなし。

## 最新対応 (2026-06-24)

### MacroTools @capture/@match expansion helper visibility (Issues #7569/#7603/#7604) — 解決済み

Bundled MacroTools macro expansion は package helper functions / structs /
hygiene member set を expansion-time VM と returned-AST lowering に持ち込む。
`allbindings` / `TypeBind` / `trymatch` lookup、macro-expanded `===` / `!==`、
multi-statement block tail value に残スコープなし。

### MacroTools @forward quoted generator support (Issues #7572/#7599) — 解決済み

MacroTools `examples/forward.jl` の quoted generator/splat method definition は
parse/lower 可能。package root と include の `@__DIR__` も source file directory を
指すため、bundled/実パッケージの `animals.txt` 初期化に残スコープなし。

### Nested closure module scope propagation (Issue #7591) — 解決済み

Module function から lifted された closure の内部でさらに作られる nested closure も、
qualified parent 名経由で module path を継承する。module-private helper lookup が
2段目以降で失われないため、Issue #7591 の `hidden` lookup blocker に残スコープなし。

### Base.eachline(filename) package initialization support (Issue #7593) — 解決済み

`eachline(filename)` は `Vector{String}` を返す file-line enumeration として使える。
`collect(eachline(path))` と `map(Symbol, eachline(path))` は MacroTools の
`animals.txt` loader 用に検証済み。lazy `EachLine` object そのものの完全再現は
現時点の package initialization blocker には残さない。

### Nested module relative named imports (Issues #7574/#7594) — 解決済み

Nested module 内の `import ..Parent: name` / `import ..Sibling: name` は
compile-time の module-local import resolver で可視化される。`LinearAlgebra.LAPACK`
も `import ..LinearAlgebra: inv, lu, LU` の upstream-compatible spelling を使うため、
Issue #7574 の workaround に残スコープなし。元MWEの `x() = 1` も Base/prelude
closure capture shadowing を回避して compile されるため、Issue #7594 にも残スコープなし。

### Macro body lifted lambda visibility (Issue #7584) — 解決済み

ctx-aware macro body lowering で生成された lifted arrow helper は、macro 定義直後に
compile-time function registry へ登録される。同一 source/include 内の後続 macro expansion から
参照できるため、Issue #7584 に残スコープなし。

### LinearAlgebra Sylvester array unary minus workaround removal (Issue #7577) — 解決済み

Array unary minus は `-[...]` と `-Matrix` のどちらも compile される。
`sylvester` は `_colvec(C)` に unary minus を直接適用するため、Issue #7577 の
workaround に残スコープなし。

### Function-local `size(A)` tuple comparison (Issue #7578) — 解決済み

関数内の `size(A) == size(B)` / `size(A) != size(B)` は tuple comparison として
扱われる。Issue #7578 の `Cannot convert Tuple to I64` 再現ケースに残スコープなし。

### Base matrix array addition/subtraction (Issue #7579) — 解決済み

Dense `Matrix + Matrix` / `Matrix - Matrix` は shape-preserving elementwise
結果を返す。Issue #7579 の `A + A` MethodError 再現ケースに残スコープなし。

### JSXGraph 3D/do-block artifact integration (Issues #7373, #7374, #7375) — 解決済み

`board(...) do` / `view3d(...) do`、`View3D` nested elements、`curve3d` raw JS
function marker (`JSFunction`) と web/iOS renderer の `view.create(...)` 再帰生成は対応済み。

残スコープ: 3D surface 系 (`surface3d`, `parametricsurface3d`, `functiongraph3d`) は
今回の milestone 34 phase scope に含めない。

### Persistent prelude Program cache compiler fingerprint (Issue #7544) — 解決済み

prelude Program cache は compiler/VM source fingerprint を含む key で無効化される。
lowering 変更後の stale Program reuse による `Undefined variable: x` 回帰に残スコープなし。

### LinearAlgebra factorization result objects (Issue #7463) — 解決済み

既存の `lu` / `qr` / `cholesky` / `eigen` / `svd` は raw tuple / NamedTuple ではなく
`Factorization` subtype を返す。既存 numeric behavior、field access、LU/SVD destructuring、
dispatch-first user override 互換に残スコープなし。

### LinearAlgebra in-place and values-only factorization APIs (Issue #7464) — 解決済み

`lu!` / `qr!` / `cholesky!` / `eigen!` / `eigvals!` / `svd!` / `svdvals` /
`svdvals!` / `isposdef!` は `LinearAlgebra` から利用可能。既存 factorization wrapper
と builtin numeric path を再利用し、`*!` API は sjulia が扱える work form を入力行列へ
書き戻す。Issue #7464 の values-only / in-place API surface に残スコープなし。

### LinearAlgebra diagonal/copy and mutating transpose helpers (Issue #7466) — 解決済み

`diagind` / `diagview` / `transpose!` / `adjoint!` / `triu!` / `tril!` /
`copy_transpose!` / `copy_adjoint!` / `copytrito!` は dense array subset で利用可能。
`diagview` の親行列 aliasing と `Diagonal` から dense matrix への `copyto!` も対応済み。
Issue #7466 の diagonal/view/copy/mutating-transpose helper surface に残スコープなし。

### LinearAlgebra matrix division operator calls (Issue #7467) — 解決済み

`\(A, b)` / `/(row, A)` の operator function-call form と、infix `A \ b` /
`row / A` は dense matrix subset で利用可能。`LU` / `QR` / `Cholesky` / `SVD`
factorization wrapper の vector RHS left division も対応済み。Issue #7467 の
matrix division operator call surface に残スコープなし。

### LinearAlgebra Givens rotations and reflection helpers (Issue #7469) — 解決済み

`LinearAlgebra.Givens` / `givens` / `rotate!` / `reflect!` は dense vector / matrix subset で利用可能。
`lmul!(G::Givens, A)` と `G * A` の基本適用 semantics も対応済み。Issue #7469 の
rotation/reflection helper surface に残スコープなし。

### LinearAlgebra BLAS/LAPACK module subset (Issue #7468) — 解決済み

`LinearAlgebra.BLAS` / `LinearAlgebra.LAPACK` module は利用可能。BLAS Level 1/2/3
subset (`dot` / `dotu` / `dotc` / `axpy!` / `scal!` / `gemv!` / `gemm!`) と
LAPACK subset (`gesv!` / `getrf!`) は Pure Julia dense loop / existing wrapper 経由で
対応済み。Issue #7468 の module availability と現行 decomposition 用 stable routine
surface に残スコープなし。

残スコープ: native BLAS/LAPACK binding、workspace query、追加 factorization driver は
milestone 37 の別 issue で必要になった時点で個別に扱う。

### LinearAlgebra matrix equations and low-rank updates (Issue #7470) — 解決済み

`condskeel`、dense `lyap` / `sylvester`、`Cholesky` wrapper 用
low-rank update/downdate API は利用可能。Issue #7470 の small dense matrix
behavior と wrapper-compatible low-rank surface に残スコープなし。

残スコープ: Schur/LAPACK `trsyl!` ベースの高性能 path、structured matrix classes、
pivoted/specialized Cholesky variants は別 issue の decomposition / structured matrix
scope で扱う。

### LinearAlgebra structured matrix wrappers and UniformScaling (Issue #7462) — 解決済み

`UniformScaling` / `I` と core structured wrapper constructors は利用可能。
dense subset の constructor / `size` / `getindex` / wrapper materialized
multiplication は対応済み。Issue #7462 の initial structured matrix surface に
残スコープなし。

残スコープ: packed/specialized storage、factorization-specific methods、
lazy wrapper-preserving arithmetic、broad `AbstractMatrix` integration は今後の
performance/decomposition issue で扱う。

### LinearAlgebra remaining decomposition family objects (Issue #7465) — 解決済み

remaining decomposition family names and small dense wrapper result surfaces are
available: `Schur`, `Hessenberg`, `LQ`, `LDLt`, `BunchKaufman`, and generalized
result object types. Issue #7465 の exported family surface と small dense
fixture scope に残スコープなし。

残スコープ: LAPACK-native Schur/Hessenberg/LQ/Bunch-Kaufman algorithms、
workspace-query APIs、pivoted/specialized storage mutation semantics は future
performance/decomposition issue で扱う。

### Module-local macro visibility and qualified Base.isexpr (Issues #7525, #7527) — 解決済み

同一 module body 内の macro definition は後続 statement macro call から参照可能。
`Base.isexpr(...)` も qualified call として method dispatch され、unqualified import guard
に落ちない。MacroTools.jl `@public` expansion を塞いでいた該当 scope に残スコープなし。

### Include macro context sharing (Issue #7510) — 解決済み

同一 module/package scope の sequential include 間で macro definitions が失われる問題は
解消済み。後続 include の function body / ternary branch 内 macro call も、先行 include
で登録された macro context を参照する。残スコープなし。

### Persistent Base cache compiler fingerprint (Issue #7515) — 解決済み

Base source が同じでも compiler/VM code が変わった場合、persistent / embedded Base
bytecode cache は combined compatibility hash mismatch で再生成される。stale cached
`occursin(::Regex, ...)` body が残る問題に残スコープなし。

### VM regex match arity dispatch guard (Issue #7502) — 解決済み

non-2-arg `match` calls は regex builtin handler ではなく通常 dispatch に進む。
MacroTools-style `match(pat, ex, env)` の arity preemption に残スコープなし。

### Plots Aizawa attractor push! hot path (Issue #7431) — 解決済み

Aizawa attractor animation の遅さは、Plots `push!(plt, x, y, z)` が 1 点ごとに
既存 series data を全コピーしていた O(n²) 経路として解消した。Plot-owned buffer
へ直接 append するため、live plot の伸長は O(n) になり、`plot(xs, ys)` の入力配列を
後続 `push!` が破壊しない semantics も regression fixture で固定済み。

残スコープ: Plotly renderer 側の frame JSON size はサンプルの point/frame 数に比例する。
今回の issue scope では VM/Plots push hot path を対象とし、Plotly 側の差分 frame
encoding や downsampling は別の rendering optimization として扱う。

### AoT call/control-flow contracts (Issues #7032, #7043, #7047, #7053, #7054, #7055) — 解決済み

AoT の `try` / `catch` / `finally`、varargs / splatting、broadcast fusion、
first-class functions、do-block、closures / lambdas の parent contract は
`docs/aot/CALL_CONTROL_FLOW_CONTRACTS.md` に定義済み。現行 enabled surface は
static native path に限定し、runtime helper boundary が必要な箇所は diagnostic gate する。

残スコープ: exception status helper、tuple packing / call adapter、runtime broadcast
helper、runtime callable handle、closure environment layout と call shim の codegen 接続は
後続 runtime/codegen work で扱う。これらは #7032/#7043/#7047/#7053/#7054/#7055 の
親 contract scope には残さない。

### AoT let-binding and HOF inference regressions (Issue #7495) — 解決済み

`convert(T, x)` の concrete type-name target と `reduce` inline lambda return fallback は
解決済み。full `aot_e2e_tests` の該当 let-binding regression に残スコープなし。

### AoT lowered operator-call inference (Issue #7504) — 解決済み

`%` / `mod` と `÷` / `div` calls の static result typing は binary operator inference と
同じ経路に接続済み。Collatz pipeline の `Any` condition rejection に残スコープなし。

### AoT fresh let binding slot type (Issue #7506) — 解決済み

fresh `let` local は converted value の concrete type を slot type として優先する。
parametric struct constructor local が同名 stale env entry を拾う問題に残スコープなし。

### AoT top-level for-loop compound assignment fix (Issue #7416) — 解決済み

top-level `for` loop body が DCE で空になる問題は解消済み。array `for` は
owned element iteration に統一し、`total += x` の generated Rust は body 内に残り、
borrowed element type mismatch も避ける。残スコープなし。

### AoT C ABI and runtime numeric contracts (Issues #7077, #7056) — 解決済み

AoT C ABI export の String / Array / struct return contract と任意精度 numeric family
contract は `docs/aot/ABI_AND_NUMERIC_CONTRACTS.md` に定義済み。現行 enabled surface は
scalar / `Nothing` export のまま維持し、non-scalar return は borrowed view、owned
handle、out-param、opaque runtime handle の実装が入るまで diagnostic gate する。

残スコープ: runtime helper 実体、owned handle release API、borrow lifetime enforcement、
BigInt / BigFloat backing storage、Rational runtime handle、C ABI wrapper codegen 接続は
後続 runtime/codegen work で扱う。これらは #7077/#7056 の contract scope には残さない。

### AoT map/filter generated Rust expectation refresh (Issue #7421) — 解決済み

#7070 の map/filter AoT generated-Rust expectation regression は、実装不足ではなく
owned iterator based filter codegen への期待文字列追従漏れとして解消した。
typed `Vec<T>` 出力、DCE 後の HOF callee 保持、non-Copy element clone semantics に
残スコープはない。

### Pure Julia exp(::Real) VM hot-loop fix (Issue #7455) — 解決済み

`exp(::Float64)` hot loop の 29x skew は、Pure Julia 実装内の generic
`2.0 ^ k` scale-back を整数指数 bit reinterpret helper に置き換えて解消した。
`exp(-745.0)` の subnormal 境界と `exp(::Bool)` Real forwarding (Issue #7484)
も fixture で固定済み。

残スコープ: `exp(::Float32)` / `exp(::Float16)` は既存どおり `Float64` canonical
implementation へ forward してから幅を戻す。upstream Julia の Float32 専用
range-reduction kernel を完全移植する作業は、追加の精度/性能要求が出た場合の後続
最適化として扱う。

### Cranelift milestone-29 parent surfaces (Issues #7081, #7080, #7079) — 解決済み

Cranelift `--emit-binary` parent scope (#7081) は object emission + system linker
driver 経路として解決済み。runtime `Value` rooting / safepoint parent scope (#7080) は
`CRANELIFT_GC_ROOTING_CONTRACT.md` に design contract と gate rule を定義済み。
globals / struct / enum parent scope (#7079) は scalar initialized globals、
non-parametric scalar-field struct stack layout、Int32-backed enum member lowering として
support matrix に反映済み。

残スコープ: heap-shaped globals、struct parameter/return ABI、enum display/reflection、
runtime `Value` helper 実体、GC/root stack runtime binding は下位 runtime/helper work で扱う。
これらは親 issue #7081/#7080/#7079 の未決事項ではなく、既存 gate を維持する。

### Cranelift varargs / kwargs call adapter lowering (Issue #7118) — 解決済み

Cranelift varargs / kwargs adapter contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。static splat は固定 native
signature へ展開し、true varargs tail は tuple packing、keyword calls は deterministic
keyword adapter symbol で canonicalize、dynamic keyword splat は runtime NamedTuple
packing へ落とす方針にした。

残スコープ: adapter generation の codegen 接続、`__sjulia_tuple_pack` /
`__sjulia_namedtuple_pack` runtime 実体、JIT/object symbol binding、dynamic keyword
validation の exception status propagation は後続 runtime/codegen work で扱う。これらが
入るまで Cranelift varargs / kwargs codegen は gate する。

### Cranelift Array / Vector heap lowering (Issue #7098) — 解決済み

Cranelift Array / Vector layout と memory-op lowering contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。`SjuliaArray*` は
runtime-owned header と data buffer を持つ managed handle とし、`length` /
`size`、1-based / column-major indexing、allocation、bounds failure の扱いを固定した。

残スコープ: `__sjulia_array_alloc` runtime 実体、JIT/object symbol binding、
Cranelift allocation/import emission、managed element write barrier、exception
status propagation の codegen 接続が入るまで、Cranelift Array / Vector codegen は
gate する。

### Cranelift exception / unwinding model (Issue #7108) — 解決済み

Cranelift exception propagation contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。native unwinding は使わず、
throw 可能な Cranelift function は hidden `SjuliaGcContext*` と
`SjuliaCallStatus` を使う。pending exception は `SjuliaGcContext` に保持し、
`try` / `catch` / `finally` は explicit status branch と cleanup block として扱う。

残スコープ: exception helper runtime 実体、Cranelift helper import/binding、
throw-capable function ABI への codegen 接続、bounds/type/allocation failure sites からの
status propagation は後続 runtime/codegen work で扱う。これらが入るまで Cranelift
exception codegen は gate する。

### Cranelift runtime Value / Any / Union boundary (Issue #7102) — 解決済み

Cranelift runtime `Value` boundary contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。`Any` / multi-variant
`Union` は opaque GC-managed `SjuliaValue*` handle とし、boxing/unboxing helper、
runtime tag check、rooting rule、Union narrowing / re-boxing rule を固定した。

残スコープ: `SjuliaValue` runtime helper 実体、Cranelift helper import/binding、
checked unboxing failure の exception transition (#7108)、Array / Vector heap lowering
(#7098) は後続 work で扱う。これらが入るまで Cranelift runtime `Value` codegen は
gate する。

### Cranelift String / Array ownership model (Issue #7107) — 解決済み

Cranelift non-Copy heap value ownership model は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。heap `String` / `Array`
は GC-managed pointer handle とし、handle copy は object ownership を複製しない。
borrowed string byte / array buffer pointer は safepoint をまたげない。read-only
String literal payload は object/JIT data section 所有として GC root 対象外にする。

残スコープ: Array / Vector heap lowering (#7098)、managed element write barrier、
heap String allocation/boxing は後続 work で扱う。これらが入るまで Cranelift heap
String / Array operations は gate する。

### Cranelift stack map / precise safepoint contract (Issue #7106) — 解決済み

Cranelift precise safepoint metadata contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。function-scoped
safepoint ID、root slot descriptor、frame-base offset、Cranelift stack map または
explicit root-stack fallback の許可条件を固定した。

この issue の safepoint metadata contract と root-stack fallback 範囲に残る未実装項目は
ない。managed heap value lowering の有効化は、Array/String heap lowering
(#7098 など) の後続 work で扱う。

### Cranelift heap allocation hook ABI (Issue #7105) — 解決済み

Cranelift から runtime heap allocation を呼ぶ hook ABI は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` に定義済み。`__sjulia_gc_alloc`、
`__sjulia_array_alloc`、`__sjulia_string_alloc` の C-ABI import symbols、
fixed-width parameter carriers、null-on-failure、allocating safepoint
classification を固定した。

残スコープ: hook の runtime 実体、JIT symbol binding、object/linker runtime library
連携、stack map emission / runtime lookup、null failure の exception transition
(#7108) が揃うまで Cranelift は heap allocation call emission を gate する。

### Cranelift GC/rooting and safepoint contract (Issue #7104) — 解決済み

Cranelift backend の GC/rooting design contract は
`docs/aot/CRANELIFT_GC_ROOTING_CONTRACT.md` で定義済み。native scalar、scalar
field だけの native stack aggregate、read-only data-section pointer は root 不要、
heap string / array / heap struct / `Any` / multi-variant `Union` / exception object
は managed runtime pointer として root/safepoint 対象にする。

残スコープ: allocation hook の runtime 実体と呼び出し生成、Cranelift stack map
emission / runtime lookup と tagged runtime `Value` helper 実体は後続 runtime work で
扱う。
これらが入るまで Cranelift heap-shaped values は gate する。

### Cranelift Complex aggregate arithmetic lowering (Issue #7099) — 解決済み

Cranelift backend は local `Complex` / `ComplexF64` / `Complex{Float64}` と
`ComplexF32` / `Complex{Float32}` を stack aggregate pair へ lower し、
`real` / `imag` / `abs2` と同一 element type の `+` / `-` / `*` を scalar field
arithmetic として扱える。この issue の aggregate representation と basic arithmetic
lowering 範囲に残る未実装項目はない。

残スコープ: Complex parameter / return ABI、heap/runtime `Value` object identity、
GC/rooting、non-Float element layouts、mixed Real/Complex promotion semantics は
runtime/aggregate boundary と parametric layout work の範囲で扱う。

### Cranelift String constants and values (Issue #7094) — 解決済み

Cranelift backend は local `String` literal を read-only length-prefixed UTF-8
payload へ lower し、function-local pointer carrier として扱える。`length(::String)`
は payload length を native `Int64` として読める。この issue の data-section constant
lowering と non-allocating length 範囲は解決済み。

残スコープ: String parameter / return ABI、String concatenation/comparison などの
allocating / semantic operations、runtime `Value::Str` boxing、GC/rooting と ownership model との接続はまだ
未実装。これらは #7102 runtime `Value` boundary、#7107 String ownership、#7104
rooting/safepoint contract の後続 work で扱う。

### Cranelift struct field layout lowering (Issue #7095) — 解決済み

Cranelift backend は non-parametric scalar-field struct definitions を stack slot layout
へ lower し、constructor、field load、mutable field store を byte offset memory ops として
扱える。この issue の初期 field layout / construction / load-store 範囲に残る未実装項目は
ない。struct parameter / return ABI、nested struct / heap-shaped fields、parametric
layout、runtime object identity、GC/rooting は後続の runtime/aggregate work で扱う。

### Cranelift multiple return / destructuring lowering (Issue #7117) — 解決済み

Cranelift backend は scalar tuple return を multi-result signature と `ReturnMany` /
`CallMulti` に lower し、destructuring の temp tuple + constant index path から consume
できる。この issue の multiple return / destructuring lowering 範囲に残る未実装項目は
ない。tuple parameter、runtime `Value` / `Any` boundary、heap tuple object、out-param
ABI は runtime/rooting/aggregate representation の範囲で扱う。

### Cranelift DWARF debug info output (Issue #7090) — 解決済み

Cranelift native artifact output は `--debug-info` で DWARF compile unit、subprogram、
line table section を emit できる。この issue の初期 debug info output 範囲に残る
未実装項目はない。現時点の line mapping は Core IR span から得られる function-level
行に限定される。命令単位の source span 精度、relocatable debug address の詳細化、
heap/runtime value の debug representation は、AoT IR / low-level IR の span 伝播と
runtime representation 拡張側の範囲で扱う。

### Cranelift static/shared library output (Issue #7085) — 解決済み

Cranelift backend は `--emit-library` で static archive と shared library artifact を
生成できる。この issue の library packaging 範囲に残る未実装項目はない。対象は現行の
Cranelift scalar/object subset と local/cross linker が扱える target に限る。外部公開
symbol は `--export-c-abi` の C-stable wrapper symbol で制御する。runtime `Value`、
aggregate、heap-shaped ABI、platform 固有 export map の詳細制御は runtime/rooting と
C ABI 拡張側の範囲で扱う。

### Cranelift `--emit-binary` object-to-link path (Issue #7083) — 解決済み

Cranelift backend は `--emit-binary` で object emission から system linker までを
実行し、native executable を生成できる。この issue の範囲に残る未実装項目はない。
現時点の対象は Cranelift scalar/object subset と local linker が対応する host/cross
target に限る。runtime startup の拡張、heap/runtime hooks、library packaging、
debug info はそれぞれ別 issue の範囲で扱う。

### Cranelift standalone executable entry wrapper (Issue #7084) — 解決済み

Cranelift object output は C ABI の `main() -> Int32` wrapper を emit できる。
この issue の entry point / main 生成範囲に残る未実装項目はない。`--emit-binary`
CLI から object emission、linker driver、出力 packaging を接続する作業は #7083 の
範囲で扱う。runtime initialization / teardown が実体を持つ必要が出る heap/runtime
surface は GC/rooting/runtime hook issue の範囲で拡張する。

### Cranelift system linker / lld driver planning (Issue #7089) — 解決済み

Cranelift 用の system linker / lld driver 選択、libc/libm/runtime library link order、
missing linker / launch / non-zero exit diagnostics は `aot::linker` へ分離済み。
この issue の範囲に残る未実装項目はない。`juliars --backend cranelift
--emit-binary` から object emission、standalone entry point、linker driver を接続する
CLI packaging は #7083 / #7084 の範囲で扱う。

### AoT `Dict` construction / lookup / iteration codegen (Issue #7034) — 解決済み

AoT Rust backend は静的 Pair 引数の `Dict(...)`、typed empty `Dict{K,V}()`、
`d[k]`、`get(d, k, default)`、`haskey(d, k)`、`d[k] = v`、`length`、`isempty`、
`collect(d)`、`for kv in d` を native `HashMap<K,V>` carrier で扱う。この issue の
範囲に残る未実装項目はない。非 hashable key type、dynamic `Dict{Any,Any}`、
`delete!` / `empty!` / `merge` / `keys` / `values` / Julia `Pair` runtime object
reflection は別途 collection/runtime surface の範囲で扱う。

### Cranelift ELF / Mach-O / COFF object smoke coverage (Issue #7088) — 解決済み

Cranelift object output は representative triple で ELF / Mach-O / COFF object を
emit できる。この issue の範囲に残る未実装項目はない。各 platform での実リンク、
linker discovery、runtime/libc/libm link order は #7089 以降の範囲で扱う。

### AoT `Set` construction / membership / iteration codegen (Issue #7035) — 解決済み

AoT Rust backend は静的 iterable 由来の `Set([iterable])`、typed empty
`Set{T}()`、membership、`push!`、`length`、`isempty`、`collect(s)`、`for x in s`
を native `HashSet<T>` carrier で扱う。この issue の範囲に残る未実装項目はない。
非 hashable element type、dynamic `Set{Any}`、`delete!` / `empty!` / set algebra、
Dict-backed runtime object reflection は別途 collection runtime surface の範囲で扱う。
AoT top-level `for` body mutation が生成 Rust で落ちる既存バグは Issue #7416 に分離した。

### Cranelift object target triple selection (Issue #7087) — 解決済み

Cranelift object output は `--target <triple>` を受け、明示 triple で ISA/object
target を選べる。この issue の範囲に残る未実装項目はない。実行ファイル化の linker
選択、runtime/libc/libm link order、platform packaging は #7083 / #7084 / #7089 の
範囲で扱う。

### Cranelift C ABI object export symbols (Issue #7086) — 解決済み

Cranelift object output は C-stable scalar / `Nothing` signature の
`--export-c-abi` を wrapper symbol として emit できる。この issue の範囲に残る
未実装項目はない。runtime `Value`、aggregate、heap-shaped value の C ABI surface は
Cranelift runtime/rooting/GC と aggregate lowering の各 issue の範囲で扱う。

### Cranelift relocatable object output path (Issue #7082) — 解決済み

Cranelift backend は `cranelift-object::ObjectModule` で現在の scalar subset を
relocatable object bytes へ emit できる。この issue の範囲に残る未実装項目はない。
object から executable へのリンク、linker/lld discovery、cross-target object
selection、C ABI export symbol surface はそれぞれ #7083 / #7084 / #7089 / #7087 /
#7086 の範囲で扱う。

### Cranelift scalar global constant lowering (Issue #7103) — 解決済み

Cranelift backend は initialized scalar top-level globals を read-only constant
projection として lower する。この issue の scalar global 範囲に残る未実装項目はない。
heap-shaped global initializer や runtime `Value` global state は GC/rooting/runtime
Value/data-section work の範囲で扱う。

### Cranelift tuple local field projection (Issue #7097) — 解決済み

Cranelift backend は local tuple literal を scalar field carrier に分解し、定数 tuple
index を選択 field に lower する。この issue の範囲に残る未実装項目はない。
tuple parameter、heap/runtime tuple object、runtime `Value` boundary は後続の
runtime/rooting/aggregate work の範囲で扱う。

### AoT parametric struct definition/codegen (Issue #7040) — 解決済み

AoT Rust backend は使用された user parametric struct 定義を generic Rust struct として
emit し、explicit constructor と default constructor inference を concrete Rust
instantiation へ lower する。この issue の範囲に残る未実装項目はない。inner
constructors、bounds enforcement の完全 parity、runtime-shaped parametric object
reflection は別途 struct/runtime surface の範囲で扱う。

### Cranelift `@enum` Int32-backed scalar lowering (Issue #7096) — 解決済み

Cranelift backend は AoT enum definitions を metadata として受理し、member 参照を
`Int32` backing value の scalar constant として lower する。この issue の範囲に残る
未実装項目はない。runtime enum object/display parity や `instances(Color)` などの
reflection surface は Cranelift runtime Value/rooting 側の範囲で扱う。

### Cranelift short-circuit `&&` / `||` CFG lowering (Issue #7115) — 解決済み

Cranelift backend は Bool `&&` / `||` を branch-preserving CFG として lower し、
RHS の lowering は短絡規則で必要な path に限定される。この issue の範囲に残る
未実装項目はない。値位置の non-Bool final operand preservation は Cranelift scalar
subset 外の既存 Julia semantic surface として扱う。

### Cranelift Float16 widened scalar lowering (Issue #7093) — 解決済み

Cranelift backend は `StaticType::F16` を F32 widened carrier として lower し、
F16 typed parameter / return / simple scalar binop と `sqrt` などの F32-family libm
経路は verifier/JIT まで通る。この issue の範囲に残る未実装項目はない。
Float16 固有の丸め・literal carrier・conversion parity は既存の literal/conversion
設計側の範囲で扱う。

## 最新対応 (2026-06-23)

### AoT parameterized `Complex{T}` arithmetic (Issue #7041) — 解決済み

AoT Rust backend は primitive numeric `Complex{T}` constructor と `+` / `-` / `*`、
`real` / `imag` / `abs2` を typed `Complex<T>` carrier へ lower する。この issue の
範囲に残る未実装項目はない。汎用 parametric struct 定義・constructor・field layout は
#7040 / #6975 の範囲として扱う。

### AoT `rand` / `randn` RNG codegen (Issue #7036) — 解決済み

AoT Rust backend は bare `rand()` / `randn()` と次元付き `rand(dims...)` /
`randn(dims...)` を VM 互換の `StableRng` stream から生成する。この issue の範囲に
残る未実装項目はない。明示 RNG object、seed control、`Random.seed!` の AoT builtin
surface は別途 RNG API surface の範囲で扱う。

### Cranelift I128 / U128 scalar lowering (Issue #7092) — 解決済み

Cranelift backend は `StaticType::I128` / `StaticType::U128` を Cranelift `I128`
carrier として lower し、typed parameter / return / simple scalar binop は verifier と
JIT 実行まで通る。この issue の範囲に残る未実装項目はない。128bit literal IR 拡張や
Julia conversion parity は既存の conversion gate / literal carrier 設計の範囲で扱う。

### Cranelift Char scalar lowering (Issue #7101) — 解決済み

Cranelift backend は `Char` を i32 codepoint carrier として lower し、`Char`
literal / local / parameter / return の scalar path は verifier/codegen まで通る。
この issue の範囲に残る未実装項目はない。`print` / `string` display runtime は
#7121、`Char` と整数の non-identity conversion は #7123、Julia full `Char`
carrier の invalid codepoint 表現は #6967 の範囲で扱う。

### AoT generator expression codegen (Issue #7046) — 解決済み

AoT Rust backend は generator expression を typed boxed iterator として lower し、
`collect(generator)`、`sum(generator)`、`if` filter 付き generator、range / array source
を扱える。この issue の範囲に残る未実装項目はない。generator body が外側変数を捕捉する
closure-heavy なケースは #7055、first-class function carrier は #7053 の範囲で扱う。

### AoT 3D+ array codegen (Issue #7033) — 解決済み

Static rank が分かる 3D 以上の arrays は Rust backend で nested `Vec` carrier に lower され、
`zeros` / `ones` / literal / `length` / `size` / `ndims` / direct indexing / linear indexing
を扱える。この issue の範囲に残る未実装項目はない。rank が静的に分からない一般 array
object や runtime-shaped array carrier は別途 runtime array 表現の範囲で扱う。

## 最新対応 (2026-06-21)

### Interact `@manipulate` (Issue #7275) — MVP のみ実装、Phase 2 未対応

`@manipulate` は静的 Plotly MVP として、単一コントロール(レンジ→スライダー / その他→
ドロップダウン, #7338)と複数コントロール(直積を結合ドロップダウン, #7344)を実装済み
(本体を選択ごとに 1 回評価 → Plotly 静的図。詳細は [STATUS.md](./STATUS.md) /
[DONE.md](./DONE.md))。**Phase 2 として未対応**:

- 真のリアクティビティ(スライダー移動で本体を再評価しライブ更新)。iOS/Web に
  widget→VM の双方向 FFI コールバック経路が無いため。
- 真の連続スライダー(スライダー移動で本体を再評価)。レンジ選択肢は **静的スライダー**
  として描画されるようになった(Issue #7338; 全選択肢を事前生成し可視性をステップで切替。
  ライブ再評価は依然 Phase 2)。
- N **独立**コントロール。`for a = …, b = …`(複数コントロール)は対応したが、本家の
  ように各変数へ独立した widget を与えるのではなく、**選択肢の直積を 1 つの結合ドロップ
  ダウンに畳む**静的近似(ラベル `a=<va>, b=<vb>`、Issue #7344)。独立コントロールの相互
  作用(Plotly コントロールは互いの選択を参照できず、JS コールバックが必要)は Phase 2+。
- 非プロット本体(数値/文字列/任意 HTML を返す本体)。現状は `Plot` を返す本体のみで、
  非プロット本体は **明確にエラー** になる(Issue #7338; 本家は値を表示するため意図的な非パリティ。
  リアクティブ表示は Phase 2+)。
- ネイティブコントロール(`slider`/`dropdown`/`checkbox`/`textbox` などの widget 関数)。
  本家 `widget()` の `AbstractRange→slider` / `AbstractArray→dropdown(togglebuttons)` は
  静的描画として対応済み(#7338)。残る `Bool→checkbox` / `String→textbox` /
  `Number→spinbox` / `Dict` / `Date` / `Color` は双方向入力が必要で静的図に載らないため
  Phase 3 据え置き(#7275)。

付随的な upstream 乖離(本実装では回避; lead へ MWE 報告済み): `scatter(rand(10))` が
upstream は通るが sjulia は `MethodError`(`rand(n)` の carrier が `scatter(y::Vector)` に
非ディスパッチ); `scatter(::Matrix)` 未対応; 添字スライス `a[:, 1]` 内のコロンの quote 往復
(`:(a[:,1])` の `:` が `Colon()` でなく未定義変数になる)。
### `Random.default_rng()`/`GLOBAL_RNG` と RNG 引数スレッディング (Issues #7230/#7231) — 解決済み

`Random.default_rng()` / `Random.GLOBAL_RNG` が VM グローバル RNG ハンドルを返し、
`rand(default_rng())`/`randn(default_rng())` が素の `rand()`/`randn()` と同一ストリーム
を進めるようになった (#7230)。無型 / `::Xoshiro` / `::AbstractRNG` の RNG 引数を取る
ユーザ関数で `randn(rng)`/`rand(rng)` がスカラを返し、`rand(rng,d)`/`randn(rng,d)` も
到達可能になった (#7231)。これにより、`rand(rng, d)`(明示 RNG での分布サンプリング)の
ブロッカーは解消。`MersenneTwister(seed)` の構築も対応済み (#7306; 決定的 MT19937-64
エンジンでバック。同一 seed → 同一系列・有限値・`isa AbstractRNG`・RNG 引数スレッディング
対応。ただし upstream の dSFMT とはビット一致しない)。sjulia の組込 RNG は Xoshiro /
StableRNG / MersenneTwister。

### Plots `plot3d` / `push!(plt, x, y[, z])` / `@animate ... every N` (Issues #7270/#7271/#7272) — 解決済み

upstream Plots.jl の Lorenz アトラクタ・アニメーションサンプル一式が動作。`plot3d`(エイリアス
+ 整数引数で空 series 初期化)、第1 series への 1 点追記 `push!(plt, x, y[, z])`、`@animate`/`@gif`
の末尾 `every N` / `when cond` 修飾子(パーサのブロック後追加引数収集 + 単一可変長マクロメソッド)。
詳細は [STATUS.md](./STATUS.md) / [DONE.md](./DONE.md)。付随して解消したマクロランタイムの一般ギャップ:
マクロ展開後の `obj.field`(`Expr(:.)`)変換、バンドルマクロ展開プログラムへのユーザ型定義の受け渡し。

### JSXGraph.jl iOS/Web frontend 描画 (Issue #6357) — 解決済み

`application/vnd.jsxgraph+json` は iOS `JSXGraphView` と web `renderJsxgraph` で描画可能。
2D board に加えて milestone 34 の 3D `view3d` / `curve3d` nested artifact も
同じ renderer path で扱う。残る 3D surface 系は別 scope。

## 最新対応 (2026-06-20)

### Broadcasted unary minus on array values (Issue #7212) — 解決済み

Array 型の `-A` / `Base.:-(A)` は既存の broadcast materialization 経路へコンパイルされる。
`Array{Any, Any}` として runtime に渡る broadcast result の unary minus も `DynamicNeg` で要素ごとに扱い、
fixture で固定済み。
この issue の範囲に残る未実装項目はない。

### Context-aware `let` lowering re-export build fix (Issue #7218) — 解決済み

`lower_let_expr_with_ctx` の実装を追加し、`let` binding と body に `LambdaContext` を伝播する。
この issue の範囲に残る未実装項目はない。

### Macro `Expr(head, args...)` splat in no-context lowering (Issue #7162) — 解決済み

macro definition body の no-context call lowering でも `Expr` constructor の positional splat を
`SplatInterpolation` 経由で保持するようになった。`Expr(:vect, names...)` は macro-local Vector を 1 引数として残さず、
upstream Julia と同じく要素を AST 引数へ展開する。この issue の範囲に残る未実装項目はない。

### Plots plot(p::Plot) copy semantics (Issue #7149) — 解決済み

`plot(p::Plot)` は source plot の `series` 配列を共有せず、`Series` と x/y/z データの snapshot を current
plot と戻り値に登録する。`plot!` / `scatter!` / `push!` による後続変更が元 plot に波及しないことを fixture で固定済み。
この issue の範囲に残る未実装項目はない。

### Cranelift float comparison NaN parity (Issue #7124) — 解決済み

Cranelift の Float64 comparison lowering は `NaN == NaN` false、`NaN != NaN` true、
NaN を含む order comparison false の Julia semantics と一致することを JIT regression で固定済み。
この issue の範囲に残る未実装項目はない。

### Cranelift numeric conversion parity gate (Issue #7123) — 解決済み

Cranelift はまだ Rust backend と同等の numeric conversion lowering を持たない。
non-identity `AotExpr::Convert` と `sitofp` / `fptosi` は、Julia の range / rounding / InexactError
semantics 実装まで diagnostic gate として扱う。Low-level `TypeAssert` conversion gate も #7123 を併記する。

### Cranelift display runtime parity gate (Issue #7121) — 解決済み

Cranelift はまだ Julia の display formatter / `show` dispatch runtime に接続していないため、
`print` / `println` / `string` は runtime bridge 実装まで diagnostic gate として扱う。
これにより Float64 whole-value suffix や `Inf`/`NaN` 表示が Rust default formatting に漏れる不一致を防ぐ。

### Cranelift integer division/remainder parity gates (Issue #7119) — 解決済み

Cranelift low-level integer `Div` / `Rem` は signed/unsigned carrier を分け、zero divisor を明示 trap にする。
`mod` / `fld` / `cld` など builtin division family は floored/ceiled/divisor-sign semantics の Cranelift lowering
実装まで diagnostic gate として扱う。

### Cranelift nested break/continue target coverage (Issue #7116) — 解決済み

Cranelift lowering の break/continue target は nested CFG regression で検証済み。continue は latch を通って
induction variable を更新し、inner break は outer loop を抜けず outer latch へ合流する。

### Cranelift switch coverage and type gate (Issue #7114) — 解決済み

Cranelift switch は integer/Bool/Char tag の chained branch として検証済み。empty cases、default、Bool key、
switch target から phi merge へ流れる block args を regression 化した。Float/NaN を含む非整数 key は
比較 semantics 実装まで明示 diagnostic として gate する。

### Cranelift phi placeholder removal (Issue #7113) — 解決済み

Cranelift phi は block parameter based SSA として扱い、未マップ phi destination や欠落 incoming edge を
typed zero placeholder で補わない。malformed phi は compile-time diagnostic として止める。

### Cranelift CFG loop/back-edge coverage (Issue #7112) — 解決済み

Cranelift block/terminator lowering の loop header phi は、単一 back-edge、nested loop、複数 latch からの
multi-back-edge で検証済み。現時点で #7112 起票範囲の CFG 欠落は見つかっていない。残る switch /
break・continue などの control-flow parity は個別 issue で扱う。

### Cranelift runtime-checked call/conversion gates (Issue #7111) — 解決済み

Cranelift は未実装 runtime-checked call や型変換を typed placeholder / pass-through で生成しない。
`sqrt`/`log` 等の DomainError check や `Float64 -> Int64` の InexactError check は、runtime check
bridge が入るまで明示 diagnostic として扱う。

### Cranelift integer overflow wrapping parity (Issue #7110) — 解決済み

Cranelift scalar integer `+` / `-` / `*` の overflow は Julia / Rust backend と同じ wrapping
semantics として検証済み。残る checked runtime error 系は InexactError / DomainError などの個別 issue で扱う。

### Cranelift array indexing bounds metadata gate (Issue #7109) — 解決済み

Cranelift low-level `GetIndex` / `SetIndex` の unchecked pointer load/store は削除済み。
現行 IR は bounds metadata を持たないため、Cranelift は配列 indexing/mutation を明示 diagnostic として
止める。将来の完全な BoundsError trap 生成は、bounds-aware array carrier / runtime bridge の設計で扱う。

### Cranelift Bool result / Bool-as-integer operand parity (Issue #7100) — 解決済み

Cranelift scalar binary op の `Bool` result 保持と、mixed `Bool` numeric/comparison operand の
promotion 境界は解消済み。`Bool * Bool` は `Bool` carrier のまま、`Bool + Int64` / `Bool < Int64`
は verifier error なしで lower される。残る Cranelift parity は bounds check、overflow、
DomainError などの個別 issue で扱う。

### AoT type-unstable local Value boxing boundary (Issue #7075) — 解決済み

type-unstable local の初期化/再代入が native slot 型不一致の invalid Rust に流れる主要 gap は解消済み。
slot は join 型として収集され、`Any`/multi-variant `Union` は runtime `Value` boundary を使う。
`typeof` も runtime boxed value では実値の `type_name()` を参照する。

### AoT abstract return Any boundary validation (Issue #7074) — 解決済み

抽象 return annotation 由来の `Any`/`Value` boundary が invalid Rust を生成する主要 gap は解消済み。
静的に subtype と分かる `convert(Real, value)` は boxing し、`Any` binary operation は runtime
`dynamic_binop` へ接続する。完全な抽象型 runtime conversion / arbitrary `convert(AbstractType, x)` は
静的に保証できない場合 unsupported diagnostic として残る。

### AoT Any-boxed ternary branch boxing (Issue #7166) — 解決済み

`Any`/`Value` boundary へ flow する mixed ternary の invalid Rust branch 型不一致は解消済み。
branch ごとに boxing するため、`::Real` return などの抽象 return 経路でも native binary 生成が通る。

### AoT Bool power DomainError / Float64 boundary parity (Issue #7073) — 解決済み

`Bool ^ signed integer` の `Bool` 結果型、`false^-1` DomainError、`Bool ^ Float64` の
Float64 `powf` 境界は AoT Rust backend で解消済み。残る power parity は、個別の numeric/Cranelift issue で扱う。

### AoT print / println collection display parity (Issue #7072) — 解決済み

AoT Rust backend の主要 collection display gap は、typed formatting expression により
`Vector` / nested vector / `Array{T,2}` / tuple / Float / String element で解消済み。
custom `show` method dispatch や全 DataType/struct 表示の完全 parity は、該当する個別 issue で扱う。

### AoT static dispatch ambiguity / no-method diagnostics (Issue #7071) — 解決済み

AoT Rust backend の static dispatch が ambiguous/no-method を silent に 1 method へ解決する主要 gap は解消済み。
残る高次の Julia method lattice / abstract type specificity の拡張は、個別の AoT dispatch parity issue で扱う。

### AoT generic `::Any` method dispatcher integration (Issue #7158) — 解決済み

明示 `::Any` parameter を含む user-level generic methods は、AoT IR converter が overload ごとの typed signature
を選ぶようになり、Rust backend の generated dispatcher に載る。`f(::Int64, ::Any)` /
`f(::Any, ::Int64)` の ambiguity と single-method no-method diagnostic は E2E regression で固定済み。
この issue の範囲に残る未実装項目はない。

### AoT HOF named function / non-Copy element support (Issue #7070) — 解決済み

`map`/`filter`/`reduce`/`foldl`/`sum(f, xs)`/`mapreduce(f, op, xs)` の named function
引数と `String` など非 Copy 要素の主要 AoT Rust backend gap は解消済み。inline lambda の
expression-position begin/let 制限は既存の Issue #7014 の残スコープとして継続管理する。

## 最新対応 (2026-06-19)

### AoT Cranelift full high-level backend coverage (Issue #6927)

`juliars --backend cranelift` now reaches the experimental Cranelift generator
when the binary is built with the `cranelift` feature, but the adapter is limited
to scalar, straight-line AoT IR. It does not replace the Rust backend and does
not emit a standalone native binary through `--emit-binary`.

残スコープは、高レベル `AotProgram` 全体を低レベル `IrModule` に正しく lower
する control-flow / heap value / runtime call / rooting-aware bridge と、Cranelift
生成物の executable output contract を設計して、Issue #6927 の experimental 制限を
外すことである。

### AoT parametric struct representation/codegen (Issue #6975)

Parametric struct definitions and unresolved constructor-like calls are currently
gated, except for the existing `Complex` special-case. Without a parametric type
carrier, field layout specialization, constructor instantiation, and dispatch
metadata, AoT Rust codegen can otherwise emit invalid Rust such as `Box(1i64)`.

残スコープは、Julia-compatible parametric type instantiation, specialized field
layout, constructor lowering, method dispatch metadata, and display/type identity
を AoT IR/runtime/codegen に導入し、Issue #6975 の diagnostic gate を外すことである。

### AoT panic-free pipeline audit (Issue #6933)

This slice removes several non-test `.unwrap()` calls from the AoT converter and
inference path, but the full panic-free guarantee is not complete. Remaining
work includes auditing generated Rust snippets that still contain `.unwrap()` /
`.expect()` / `panic!()` and deciding whether they should become runtime
`RuntimeResult` propagation, classified AoT diagnostics, or deliberately
documented abort-on-error helpers.

残スコープは、non-test AoT pipeline / generated-code templates / runtime support
を分類し、user input や valid Julia execution path から raw panic に到達しないことを
CI audit と regression tests で固定することである。

### AoT expression-position sequence blocks (Issue #7014)

Expression-position `begin` / `let` blocks with bindings or multiple statements
are currently gated. AoT IR can represent statement-position blocks, but it does
not yet have a sequence expression that can emit preceding statements for side
effects and still return the final expression value inside a larger expression
such as a call argument.

残スコープは、AoT IR/codegen/optimizer/rooting walkers に sequence expression
carrier を追加し、`println(begin println("side"); 1 end)` のような code を
Julia と同じ evaluation order で生成できるようにして Issue #7014 の gate を外すことである。

### AoT `Dict` / `Set` collection codegen (Issues #6971, #6972, #7016)

AoT Rust backend does not yet have a `Dict` / `Set` representation or helper
surface compatible with the VM/Pure Julia collection implementation. Local
`Dict(...)` / `Set(...)` construction is now rejected at the converter boundary
with `UnsupportedInstruction` instead of leaking into unrelated `Any` condition
codegen failures.

残スコープは、Julia-compatible Dict/Set runtime carrier、construction from pairs
/ iterables, lookup / membership (`haskey`, `in`), mutation helpers, iteration,
display, and dispatch-visible parametric type metadata を AoT runtime/codegen に
導入し、Issue #6971 / #6972 の diagnostic gate を外すことである。

### AoT parameterized Complex codegen (Issue #6965)

Rust backend の generated `Complex` struct は現時点で `Float64` field layout の
monomorphic 型である。`Complex` / `Complex{Float64}` / `ComplexF64` はこの
layout へ投影するが、upstream に存在しない旧 `Complex64` alias は受け付けない
(Issue #9695)。`Complex{Float32}` / `Complex{Int64}` など
non-`Float64` parameterized Complex の static `+` / `-` / `*` は diagnostic gate
にしている。

残スコープは、Julia upstream の `Complex{T<:Real}` と同じく field type `T` を
保持する generated Rust representation と、`real(z)` / `imag(z)` / `promote`
に基づく mixed `Real` / `Complex{T}` arithmetic lowering を導入することである。

### AoT full Julia `Char` carrier (Issue #6967)

Rust backend の `StaticType::Char` は現在 Rust `char` に投影されるため、valid
Unicode scalar literal は生成できる。一方、Julia `Char` は `UInt32` 的な carrier
として invalid Unicode code point も保持でき、`Char(0xd800)` のような値は Rust
`char` では表現できない。

現時点では conversion-to-Char を diagnostic gate にする。残スコープは、Rust
`char` ではなく `u32` などの Julia `Char` carrier を AoT ABI/codegen に導入し、
表示・比較・integer conversion と invalid-codepoint semantics を upstream Julia
に合わせることである。

### AoT dynamic tuple indexing (Issue #6962)

Rust tuple field access は compile-time literal field (`.0`, `.1`, ...) しか受け付けない。
AoT Rust backend は static tuple type + constant in-range index の場合だけ field access
を生成し、dynamic `t[i]` や out-of-bounds literal index は diagnostic gate にする。

残スコープは、heterogeneous tuple element result を `Union` / runtime `Value` boundary
として表現し、Julia の bounds error と dynamic index semantics を保った helper lowering
を設計することである。

### AoT general N-dimensional array representation (Issue #6960)

Rust backend の public array codegen は 1D `Vec<T>` と 2D nested `Vec<Vec<T>>` を主対象にする。
2D `length` / `size` / `ndims` は static rank から生成するが、3D+ arrays は Julia の
column-major shape/indexing semantics と nested `Vec` 表現のずれが残るため diagnostic gate
にする。

残スコープは、一般 N-D Array carrier、shape metadata、linear indexing、`size(A,d)` /
`length(A)` / multidimensional indexing を一貫した representation で実装することである。

### AoT Bool division/modulo/power runtime parity (Issue #6980) — 解決済み

`Bool` を含む static `+` / `-` / `*` と mixed comparison は generated Rust で
Julia の numeric promotion に合わせる。`if` / `while` / ternary condition も
`Bool` のみ許可する。

`Bool`/`Bool` と mixed `Bool`/integer の `÷` / `%` / `^` は Rust backend で
Julia result surface に合わせるようになった。signed integer exponent の `Bool ^ n`
は `Value` boundary で `Bool` / `Float64` / `DomainError` を表し、Bool denominator
false は `DivideError` を投げる。この issue の AoT Bool/Int arithmetic scope に
残る unimplemented item はない。

### AoT VM-compatible RNG contract (Issue #6964)

Rust backend は `rand` / `randn` の ad hoc codegen を行わない。`rand` の
undeclared `rand::random::<f64>()` と `randn` の constant fallback は削除し、
VM と同じ seed / RNG state / distribution contract が定義されるまで diagnostic
付き gate とする。

残スコープは、VM の Random/StableRNG 方針に沿った AoT runtime RNG API を定義し、
generated Rust から明示的な RNG state を受け渡せるようにすることである。

### AoT checked numeric conversion runtime plumbing (Issue #6968)

Rust backend は `AotExpr::Convert` / `fptosi` のうち、Julia `InexactError`
parity に runtime check が必要な conversion を direct Rust `as` では lower
しない。現時点では float->integer、integer narrowing、符号境界、
numeric->Bool を diagnostic 付き gate にする。

残スコープは、AoT expression codegen が `RuntimeResult` を伝播できる形にし、
`Int64(1.0)` のように成功する checked conversion は実行時に値を返し、
`Int64(1.5)` / 範囲外 / 非有限値は本家 Julia と同じ `InexactError` 系を返す
実装へ進めることである。

### AoT Cranelift managed strings and unsupported scalar carriers (Issues #6948/#6949)

Cranelift low-level backend は `ConstValue::String` をまだ lowering しない。
managed string は runtime `Value` / rooting / safepoint contract を必要とするため、
現時点では dedicated diagnostic で拒否する。

また、Cranelift type mapper は `I128` / `U128` / `F16` / `Missing` の native
carrier を未実装として拒否する。これらは heap/rooting gate ではなく、低レベル
Cranelift 型対応の残スコープ。

### AoT low-level `IrFunction` optimizer gaps (Issue #6944)

`subset_julia_vm/src/aot/optimizer/pass.rs` の `OptimizationPass` trait は
低レベル `IrFunction` / CFG 用の pass interface を持つが、現行 `juliars`
pipeline の主経路は高レベル `AotProgram` optimizer である。低レベル
`StrengthReduction::optimize_function` と `Inlining::optimize_function` は明示的に
no-op で、低レベル IR 上の strength reduction / inlining は未実装。

現時点で有効な最適化は `optimize_aot_program_with_strength_reduction`、
`optimize_aot_program_with_inlining`、および `-O0..-O3` から呼ばれる
高レベル AoT IR pass 群である。Cranelift など低レベル backend は、この追加
`IrFunction` pass が走る前提を置かないこと。

## 最新対応 (2026-06-14)

### Public Array construction/native carrier routing (Issues #6649/#6653) — 解決済み

Untyped array literal、typed empty literal、typed non-empty literal、
single / tuple-destructuring / multi-iterator comprehension materialization、
`Vector{T}()` / `Array{T}()` empty constructors、および direct `collect`/range
materialization (`collect(1:3)`, step/float range, tuple collect,
`collect(array)`)、non-empty generator/HOF-backed collect (`collect(x + 1 for x in
1:3)`, `Base.Generator` runtime/named callable、filtered generator、
tuple-splat generator) は `Memory{T}` + Pure Julia `Array{T,N}` wrapper
construction へ移行済み。新規 public construction body は原則 `NewArray*` /
`PushElem*` / `FinalizeArray*` を emit せず、`NewMemory` / `MemorySet` /
`wrap(Array, mem, dims)` を使う。`NewArray*` 命令は cache-compatible decode /
VM boundary fallback として残す。

`Array{T,N}` wrapper operation surface (#6650-#6652) と最終 native carrier
demotion / benchmark (#6653) も完了済み。旧 `Value::Array` は #4568 で退役し、
残る `Value::NativeArray(ArrayRef)` は precompiled cache、VM instruction fallback、
formatting/REPL/host boundary 用の互換 carrier としてのみ残す。

### Array wrapper indexing/shape over MemoryRef storage (Issue #6650) — 解決済み

`Array{T,N}` wrapper の `getindex` / `setindex!` / `length` / `size` /
`ndims` / `axes` / `eltype` は `ref::MemoryRef{T}` + `size::NTuple{N,Int}` を
読む Pure Julia method / VM wrapper boundary で処理する。`axes(A)` /
`axes(A,d)` は upstream と同じ `OneTo` surface、0-dimensional array は
`axes(A) == ()`、`getindex(A)` / `setindex!(A,v)` で同じ scalar slot を読む/書く。

final native carrier demotion / benchmark は #6653 で完了済み。

### Array wrapper mutation/iteration over MemoryRef storage (Issue #6651) — 解決済み

`Array{T,N}` wrapper の `push!` / `pop!` / `pushfirst!` / `popfirst!` /
`append!` / `insert!` / `deleteat!` / `resize!` / `empty!` / `iterate` は
`ref::MemoryRef{T}` + `size::NTuple{N,Int}` を直接更新する。offset wrapper では
親 `Memory` の head/tail capacity を使い、grow/shrink/shift 後も upstream と同じ
sharing semantics を保つ。`iterate(a)` / `iterate(a,state)` は upstream と同じ
next 1-based index state を返す。

final native carrier demotion / benchmark は #6653 で完了済み。

### Array wrapper HOF/broadcast/materialization over MemoryRef storage (Issue #6652) — 解決済み

`Array{T,N}` wrapper source に対する `map` / `map!` / `broadcast` /
`broadcast!` / `reduce` / `mapreduce` / `collect` / `filter` / `filter!` /
`sort` / comprehension materialization は upstream と同じ value/shape を返し、
public result surface も `MemoryRef` backed `Array` として保持する。
offset `MemoryRef` source、binary `map`、matrix broadcast、filtered comprehension
は `array_hof_broadcast_wrapper_6652.jl` で固定済み。

final native carrier demotion / benchmark は #6653 で完了済み。

### Array native carrier demotion / benchmark (Issue #6653) — 解決済み

public Array construction / materialization / HOF / broadcast / `similar` /
`reshape` は `MemoryRef` backed `Array{T,N}` wrapper を返す。`NativeArray`
converter / handler は old bytecode/cache compatibility と VM/host boundary fallback
として残す。`array_native_carrier_demoted_6653.jl` と Rust bytecode guard で、
public route が `NewArray*` / `PushArrayValue` / `AllocUndef*` carrier builder を
emit しないことを固定済み。

`vm_array_benchmark` を追加し、#6649 直前 baseline `2404f188e` に同 bench を
一時適用して比較。index/mutation は約 3.4x 遅い一方、HOF/broadcast は約 8.0x
速い。後続の性能改善は typed Memory storage / intrinsic hot loops で扱い、
native carrier を public default へ戻す残スコープは残さない。

### `Dict{K,V}` Memory-backed storage foundation (Issue #6617) — 部分解決

Pure Julia `Dict{K,V}` struct の field storage は upstream 形の
`slots::Memory{UInt8}`, `keys::Memory{K}`, `vals::Memory{V}` へ移行済み。
typed `_new_dict_kv(K, V, n)` と `rehash!` は `K`/`V` を維持する。

Public construction routing は #6619 で Pure Julia `Dict{K,V}` struct へ移行済み。
`Dict{K,V}` public operation/display parity は #6620 で補強済み。
legacy `Value::Dict` / `NewDict*` / Rust Dict builtin の public route demotion は
#6621 で完了済み。Pure Julia `Dict{K,V}` の VM-only benchmark と退行係数の
可視化は #6622 で完了済み。
既存 `NewDict*` bytecode と public Dict builtins は cache-compatible decode /
VM-boundary fallback 用として残す。

### Generic `Dict` constructors (Issue #6618) — 部分解決

Ordinary Julia method 経由の `Dict(ps::Pair...)` / `Dict(kv)` は
Memory-backed `Dict{K,V}` struct を返すようになり、Pair splat と tuple/zip
iterable entries から upstream-compatible に `typejoin` した key/value 型を選ぶ。

Public construction routing は #6619、struct-backed op/display parity は #6620、
native `Value::Dict` route demotion は #6621 で解決済み。残スコープは #6622 に
集約し、performance measurement / docs finalization も解決済み。

### Public `Dict` construction routing (Issue #6619) — 解決済み

`Dict()` / `Dict(pairs...)` / `Dict(kv)` / `Dict{K,V}(...)` と
literal/comprehension/generator 由来 construction は Pure Julia `Dict{K,V}` struct
method 経路へ移行済み。`NewDict*` は新規 public construction では生成せず、
既存 cache/bytecode decode 互換用に残す。

### `Dict{K,V}` op/display parity (Issue #6620) — 解決済み

`keys` / `values` lazy views、Pair membership、`filter` / `filter!`、
`==` / `isequal` / `hash`、compact `repr` / `string`、mutation reference、
mixed Float/Type/Symbol keys、rehash lookup は struct-backed `Dict{K,V}` で
upstream-visible parity を固定済み。残りは #6622 の benchmark/documentation。

### native `Value::Dict` route demotion (Issue #6621) — 解決済み

新規 public construction / struct-backed Dict operation は `NewDict*` や public
`BuiltinId::Dict*` fallback を emit せず、Pure Julia `Dict{K,V}` method route を
通る。`Value::Dict`、`DictValue`、`_dict_*` intrinsics、`NewDict*` decode/exec、
public Dict builtins は旧 bytecode/cache 互換と VM boundary 用として残置済み。
#6622 の performance measurement / docs finalization も完了済み。

### Pure Julia `Dict{K,V}` performance measurement (Issue #6622) — 解決済み

`vm_dict_benchmark` で typed Int/String key Dict の insert / lookup / iterate /
delete / post-delete insert を VM-only 測定できるようにし、#6619 直前の
legacy `Value::Dict` route と現行 Pure Julia struct route の退行係数を STATUS/DONE
へ記録済み。#6571 Dict migration の残スコープは完了。今後の性能改善は新規
follow-up issue として扱う。

## 最新対応 (2026-06-13)

### User macro expansion-time execution (Issue #6616) — 解決済み

User-defined macro bodies are now executed during lowering with upstream-shaped
hidden `__source__` / `__module__` arguments and unevaluated AST inputs. The
Symbolics-style `@variables` helper-call shape and `Expr.args` mutable aliasing
have no remaining unimplemented scope in this issue. Broader Symbolics.jl
package compatibility remains outside this issue and should be tracked by
separate package-support issues.

### subtype engine 統合 (Issue #5915) — 部分解決

runtime string subtype は既に `CoreSubtypeEngine` に一本化済み。今回、
enum-level `JuliaType::is_subtype_of` から `AbstractString` / `AbstractChar` /
`IO` / `Function` / `Type` の built-in family 局所 arms も削除し、core engine
の判定を正とする範囲を拡大した。さらに `CoreType` 側の dense array family
(`Vector` / `Matrix` / `Array`) は `AbstractUser(AbstractVector/AbstractArray/...)`
親を builtin abstract に正規化できるようになった。残スコープは、
`JuliaType::is_subtype_of` に残る array / tuple / NamedTuple / AbstractUser /
TypeVar / UnionAll などの局所 arms を、ユーザー階層・名前のみ NamedTuple・
unresolved bound の互換性を保ちながら `CoreSubtypeEngine` へ寄せること。
wrapper array family 全般の `AbstractUser` alias 拡張は method specificity /
runtime candidate 選択との相互作用を見ながら段階移行する。

### 型表現変換の重複削減 (Issue #5916) — 部分解決

`compile::abstract_interp::engine` に残っていた `LatticeType` / `ConcreteType`
→ `JuliaType` 変換コピーは正準 `runtime_types::bridge::lattice_to_julia_type` へ
委譲済み。これで engine 内の戻り型 cache invalidation と `Pair{K,V}` 型名補助は
同じ bridge を使う。`TypeExpr` → `JuliaType` projection も
`TypeExpr::{to_julia_type_lossy, substitute_to_julia_type_lossy}` へ集約済みで、
compile context と runtime type-object reflection の局所コピーは退役した。今回、
compile 層の `type_expr_to_string` helper も退役し、nested display は
`TypeExpr::Display`、`Dict{K,V}` / `Set{T}` / `Union{...}` 用の simple-name 解決は
`TypeExpr::as_simple_type_name` へ集約した。compile pipeline の struct field table も
`TypeExpr::to_julia_type_lossy` を直接使うようになり、`Union` だけを別 arm にした
local projection は削除済み。`TypeExpr` パラメータ列表示も
`TypeExpr::{render_param_list, format_parameterized}` へ寄せ、compile 層の局所
join 実装は退役済み。さらに AoT struct field annotation の
`TypeExpr → StaticType` projection も `StaticType::from_type_expr_lossy` へ移し、
`CoreType` name parser 経由で backend 型へ落とすようにした。残スコープは、
`ValueType` carrier loss、AoT 側に残る `StaticType` 固有の型演算 / unsupported
value-param 診断、型表現間の境界でまだ local arm を持つ箇所の棚卸しと委譲統合。

### runtime dispatch entry unification (Issue #6502) — 部分解決

`CallDynamic` / `IterateDynamic` の production fallback tier は `core_signature` /
`CoreType` ベースの slice resolver へ移行済みで、旧 `usize::MAX` native-iterator
sentinel も文字列候補ではなく `CoreType` 候補として扱う。`runtime_candidate_core_type`
の `AbstractUser` / `Module` legacy parse fallback も削除済みで、exact/subtype tier は
`CoreType` の nominal bridge で維持する。`CallTypedDispatch[OrBuiltin*]` の候補 cache も
`RuntimeCandidateCoreSignature` 化済みで、`call_dynamic_typed.rs` の production string
resolver 呼び出しも structured `RuntimeTypedCoreCandidate` resolver へ置換済み。
typed dispatch の final winner ladder も `selection::select_typed_dispatch_candidate` へ
移管済み。旧 string resolver API は production から退役し、`#[cfg(test)]` parity oracle
としてのみ残る。typed resolver 内部の specificity tie-break も `CoreType` slots 由来に
移行済みで、production は rendered type-name specificity helper を使わない。
covariant-bound fallback も CoreType slot matching へ移り、string-only helper は
test-only oracle になった。typed resolver の primary/fallback tier split も bounded
`CoreType` slot の explicit bound 判定へ移行し、rendered `<:` marker 依存は外れた。今回、
`find_best_method_index_from_candidates` に signature-wide strict
subtype dominance precheck を追加し、`ReshapedArray` collect/map の runtime candidate
選択も structured signature で pin した。runtime value-channel の
`where T<:UserAbstract` 境界チェックも `CoreSubtypeEngine::with_hierarchy` へ移行済みで、
compile/core-signature 経路との user hierarchy 非対称は解消済み。value-position
scoring の `value_param_base_specificity` も `AbstractUser` 親を `JuliaType::from_name`
で再パースする legacy 経路を廃し、`CoreType::AbstractUser { parent }` の構造化親を直接
読む形へ移行済み(Issue #6594、`user_abstract_parent_is_boostable` で builtin 親のみ
boost、`Any`/未解決 user 親は flat floor を維持)。native-array wrapper fence も
selection core の policy へ吸収済み(Issue #6595、`selection::signature_is_broad_wrapper_fence`
+ `selection::wrapper_fence_name_channel_repair`):value channel の broad-`Any`/`Function`
catch-all winner を検出して name channel の non-broad 候補で repair する制御フローが
`call_dynamic_typed.rs` のインラインから structured core helper へ移った。残る広義の dispatch
統合スコープは、value-channel dominance ladder の score-winnow / tie-break 本体(現状
`find_best_method_index_from_candidates` 内の `selection::pick_best` クロージャ)を
`pick_scored_match` 相当の structured ladder へさらに寄せること。
boost、`Any`/未解決 user 親は flat floor を維持)。さらに family-fallback の
matcher `runtime_core_family_fallback_matches` も `CoreType::nominal_family_name`
accessor で構造化照合へ移行し、family tier 内に残っていた `to_julia_name()`
round-trip + base 名再パースを撤去済み(Issue #6593)。残る広義の dispatch
統合スコープは、value-channel dominance ladder 全体をさらに structured selection core へ
寄せること。

### open bug issue cleanup (Issues #6544, #6547, #6548, #6550) — 解決済み

`import Base: *, ==, +` の parser gap、inner constructor の `where` 上限境界 enforcement、Base numeric wrapper 推論 snapshot の過広化、`map(abs, ::Vector{Any})` の runtime callable dispatch は全て解決済み。これら 4 bug issue のスコープに残る未実装項目はない。

## 最新対応 (2026-06-12)

### lazy specialization `IndexAssign` / `FieldAssign` typed fast path (Issue #6346) — ほぼ解決

1D `Vector{Int64}` / `Vector{Float64}` の `a[i] = x` は lazy specialization と
`ExecutableBlock::TypedLoop` の対象。2026-06-13 に可変 struct の `obj.field` 読み出しと
`obj.field = value`(typed `GetField`/`SetField` + フィールド型強制)、および連鎖積
`k * b.x * dt` を含む n-ary 演算子呼び出しの typed fold も対象化した(DONE.md 参照)。
型不一致・多次元 index・immutable struct・非数値オペランドは意図的に generic fallback を
維持。`DestructuringAssign` IR 自体は lowering が `a, b = ...` を temp tuple + indexed
`Assign` に desugar 済みで現パイプライン未生成だが、desugar 後の swap
(`temp = (b, a%b); a = temp[1]`)の型安定化は **Issue #6561 で解決済み**(specializer の
tuple-element 型追跡 + 定数 index の typed sharpen, DONE.md 参照)。

### Callable-value n-ary `+` / `*` narrow integer fold (Issue #6512) — 解決済み

Callable operator values now preserve same-type narrow integer results through
the runtime intrinsic fold. The `::Function` exact-name workaround in
`type_matches` has been removed, so this issue has no remaining unimplemented
scope. Broader mixed-width promotion parity remains covered by the existing
mixed-width arithmetic fixtures and is outside this bugfix.

## 最新対応 (2026-06-11)

### Bool div result type (Issue #6486) — 解決済み

`div(::Bool, ::Bool)` now returns Bool for true denominators and throws
`DivideError` for false denominators, matching upstream Julia. This issue's
Bool division result-type scope has no remaining unimplemented items.

### signed/unsigned primitive-width fallback conversions (Issue #6494) — 解決済み

The VM fallback for `signed` / `unsigned` now covers all primitive integer
widths with same-width reinterpretation, matching the Pure Julia public methods.
This issue's concrete primitive-width fallback conversion scope has no remaining
unimplemented items.

### Mixed-width integer div result types (Issue #6477) — 解決済み

Mixed-width integer `div` and lowered `÷` now preserve upstream concrete result
types instead of widening through the generic Float64 fallback. This issue's
primitive and BigInt mixed integer division scope has no remaining unimplemented
items. Same-type Bool division remains tracked separately by Issue #6486.

### BigInt narrow integer promote conversion (Issue #6489) — 解決済み

The VM `BigInt` constructor now accepts Bool and all primitive signed/unsigned
integer widths, so value-level `promote(BigInt, narrow integer)` can realize the
concrete `BigInt` target type restored by Issue #6487. This issue's concrete
promotion-conversion scope has no remaining unimplemented items.

### Mixed integer promote_type concrete results (Issue #6487) — 解決済み

Mixed concrete integer `promote_type` now returns upstream concrete result types
for signed/unsigned integer and BigInt pairs, and value-level `promote` now
converts signed/unsigned primitive integer pairs through that concrete result.
This issue's concrete mixed-integer `promote_type` scope has no remaining
unimplemented items. BigInt/narrow value conversion is covered by Issue #6489,
and broader parametric `Union{Type{...}}` dispatch generalization remains
outside this bugfix scope.

### Legacy native-array carrier compatibility isolation (Issue #6337) — 解決済み

`legacy_array` named VM helpers have been removed, and the remaining transitional
native-array carrier compatibility predicates are isolated in
`vm/native_array_compat.rs`. This issue's cleanup scope has no remaining
unimplemented items. Full native-array producer retirement to Memory + Pure
Julia `Array{T,N}` wrappers continues under the broader #3908/#4189 migration.

### `for outer i` modifier mis-lowering (Issue #6465) — 解決済み

The parser-recognized `for outer i in itr` modifier form is now rejected during
lowering instead of being executed as `for outer in i`. The concrete bug where
top-level code produced `UndefVarError` or stale global values has no remaining
scope. Full local-scope `outer` loop-variable semantics remain outside this
bugfix scope.

### StatsPlots 分布プロット (Issue #7262) — 解決済み

同梱パッケージ `StatsPlots` を追加し、`using Distributions, StatsPlots;
plot(Normal(0, 1))` が pdf 釣鐘曲線(連続=`:line`、離散=`:bar`)を既存 Plots
アーティファクト経路で描画するようになった。サンプリング範囲は
`quantile(d, 0.0001) … quantile(d, 0.9999)`。#7235 のクロスモジュール抽象ディス
パッチ問題を避けるため、具象分布型ごとの typed wrapper → untyped ヘルパー委譲で
実装している。

この concrete scope に残る未実装項目はない。以下は別スコープ(別 issue 候補):
`@df` マクロ / DataFrame 連携、`corrplot` / `marginalhist` / `boxplot` / `violin`
/ `density` 等の高機能レシピ、標本ベース `histogram(rand(d, n))`。明示 RNG
`rand(rng, d)` のブロッカー(#7230 / #7231)は解決済み(上記参照)。なお、qualified type access `Plots.Plot`(=
`Module.Type` でのエクスポート型参照)が VM で未対応であることが本作業で判明した
(unqualified `Plot` は export 経由で動作)— 別途チームへ MWE 報告済み。

### Plots `bar` / `bar!` support (Issue #6358) — 解決済み

Bundled `Plots` now supports `bar` / `bar!` for vector y data, explicit x/y
data, and vectors of `(x, y)` pairs, producing `:bar` Series that iOS/Web render
as Plotly bar traces. This issue's concrete app-facing bar plot scope has no
remaining unimplemented items. Full upstream Plots.jl attribute parity,
including per-bar styling semantics beyond accepting display kwargs, remains
outside this scope.

### Mixed-width integer `DynamicPow` stack overflow (Issue #6390) — 解決済み

Mixed-width primitive integer powers now stay on the inline VM integer-power
path, preserve the base type, and raise catchable `DomainError` for negative
integer exponents. This issue's concrete stack-overflow and integer-power parity
scope has no remaining unimplemented items.

### Legacy return inference retirement (Issue #6335) — 解決済み

`infer_function_return_type` 系 legacy 経路の本番呼び出し元はなくなり、
引数型付き call-site return refinement も shared abstract-interp engine 経由に
統一された。この issue の concrete scope に残る未実装項目はない。AoT inference
engine との共通部吸い上げは別 issue scope として扱う。

### Pair expression bare-operator RHS (Issue #6461) — 解決済み

Pair expressions now preserve a bare operator RHS, so `:f => +` lowers to a Pair
whose value is callable as `+`; function returns of such Pair expressions keep
the concrete Pair struct metadata. This issue's concrete parser/lowering scope
has no remaining unimplemented items.

### `IOContext(io, context)` property inheritance (Issue #6467) — 解決済み

`IOContext(io, existing_ctx)` now exposes inherited context properties through
`get` and `haskey`, matching this issue's upstream-observable property behavior.
The concrete constructor-storage bug has no remaining unimplemented scope.

### Empty `IOContext(io)` constructor (Issue #6468) — 解決済み

`IOContext(io)` now creates an empty-property context, and
`IOContext(existing_ctx)` returns the existing context unchanged. This issue's
one-argument constructor scope has no remaining unimplemented items; the
two-argument context inheritance constructor remains tracked by Issue #6467.

### IOContext get/haskey fixture parity (Issue #6408) — 解決済み

`iocontext_get_haskey.jl` now uses upstream-compatible `IOContext(...)`
constructors rather than the sjulia-only `iocontext(...)` helper. The concrete
fixture parity bug has no remaining unimplemented scope.

### Direct `IOContext` pair constructors (Issue #6409) — 解決済み

`IOContext(io, :key => value)` と
`IOContext(io, :compact => true, :limit => true)` now support direct Pair
properties for `get` / `haskey`, matching the upstream constructor behavior for
this issue's concrete scope. The adjacent empty constructor and context
inheritance constructor gaps remain separate bugs tracked by Issues #6468 and
#6467.

### Contextual `outer` for-loop variable (Issue #6414) — 解決済み

`for outer in itr` now parses and runs with `outer` as the loop variable,
matching upstream Julia's contextual keyword behavior. This issue's concrete
parser/runtime scope has no remaining unimplemented items. Full
`for outer i in itr` modifier semantics are tracked separately by Issue #6465.

### `-e` semicolon-separated bare operator statements (Issue #6394) — 解決済み

`sjulia -e` now accepts semicolon-separated statements where an assignment RHS is
a first-class bare operator value, e.g. `f = +; f(1, 2)`. The same boundary rule
also handles newline-separated statements while keeping unary forms like `+ 1`
unchanged. This issue's concrete parser/CLI scope has no remaining
unimplemented items.

### Plots `heatmap` support (Issue #6360) — 解決済み

Bundled `Plots` は `heatmap` / `heatmap!` を export し、`heatmap(z)` と
`heatmap(x, y, z)` を `:heatmap` series と Plotly `"type":"heatmap"` trace へ
変換するようになった。matrix z orientation と 2D `aspect_ratio` layout 反映も
covered。この concrete scope に残る未実装項目はない。Full upstream Plots.jl の
recipe/attribute parity は別スコープとして扱う。

### Plots `contour` support (Issue #9940) — 解決済み

Bundled `Plots` は `contour` / `contour!` を export し、`contour(z)`、
`contour(x, y, z)`、`contour(x, y, f::Function)` を `:contour` series と
Plotly `"type":"contour"` trace へ変換するようになった。`levels` keyword と
bang append もこの issue の concrete scope で covered。この scope に残る未実装項目はない。
Full upstream Plots.jl の recipe/attribute parity は別スコープとして扱う。

### Plots histogram `weights(...)` wrapper (Issue #6451) — 解決済み

Bundled `Plots` は `weights(w)` を export し、`histogram(...; weights=weights([...]))`
を既存の weighted histogram path へ通すようになった。weighted histogram は `:bar`
series と Plotly `"type":"bar"` trace を生成する。この concrete scope に残る
未実装項目はない。StatsBase.jl 全体や full Plots.jl recipe parity は別スコープとして扱う。

### Plots `aspect_ratio` keyword (Issue #6353) — 解決済み

Bundled `Plots` は `aspect_ratio` と alias 群を受け付け、2D Plotly artifact へ
axis scaling lock を反映するようになった。この concrete scope に残る未実装項目はない。
Full upstream Plots.jl の recipe pipeline や未対応 plot attributes は既存の広い Plots
parity scope として扱う。

### VM eval-breaker-style boundary checks (Issue #6342) — 解決済み

毎命令の cancellation atomic load と call-depth 比較は VM dispatch loop から外れ、
cancellation は backward jump / call-frame push 境界、call-depth overflow は call setup
完了後の pending raise で処理するようになった。`vm_mandelbrot/run_only` は main
`37.467ms` から `36.471ms`、`vm_calc_pi_large/base_gcd_run_only/1000` は neutral
(`3.9518s` → `3.9512s`)。この issue の concrete scope に残る未実装項目はない。
より大きい eval-breaker flag 集約や time-slicing は別スコープとして扱う。

### CompiledProgram Base cache decode profiling (Issue #6449) — 部分対応

Base cache 内の `CompiledProgram` を sub-section 化し、persistent/embedded Base cache
では warm compile が再構築する `specializable_functions` を保存しないようにした。
これにより Base cache は `8.58MB` → `5.66MB`、`cache.deserialize.compiled` は
`~28-30ms` → `~18.7-20.7ms`、`cache.get_or_init_base_cache` は `~39-41ms` →
`~29.8-31.8ms` まで改善した。残る主な decode cost は `compiled.code`
(`~13-15ms`, `3.83MB`) と `compiled.functions` (`~5-6ms`, `1.11MB`) で、
さらなる削減は VM instruction/function metadata の compact format、または code prefix
の two-segment/lazy linking として分けて扱う。

### Base cache bincode decode profiling (Issue #6440) — 部分対応

Base cache の外側を section envelope にして decode 内訳を profile できるようにし、
serialized `MethodTable` から再構築可能な per-table hierarchy projection maps を除外した。
これにより Base cache は `13.6MB` → `8.58MB`、method table decode は `~31-33ms`
→ `~4.2-4.6ms`、`cache.get_or_init_base_cache` は `~65-69ms` → `~39-41ms`
まで改善した。この時点で残った `CompiledProgram` section (`~28-30ms`, `7.87MB`)
は #6449 でさらに内訳化・削減した。

### Warm-start compile overhead reduction (Issue #6348) — 部分対応

compile phase timing、Base IR pass の user-slice 化、cached Base prefix を避ける
peephole fast path、shared inference engine の二重 clone 削減、cached Base top-level
scan の一部省略、cached method table の COW 化、persistent/embedded Base cache からの
inference snapshot 省略、cached Base bytecode prefix の最終 assemble 化で warm CLI は
改善した。#6440/#6449 で Base cache decode 内訳化、method-table payload 削減、
persisted specialization IR 省略も進み、残る主な cost は `compiled.code` decode
(`~13-15ms`)、`compiled.functions` decode (`~5-6ms`)、inference function clone
(`~16-19ms`)、method table setup (`~29-35ms`)、最終 cached prefix assemble
(`~11ms`) で、2セグメント Base linking / code-function metadata compact format /
per-script `.sjvmbc` cache は今後の #6348/#6349 スコープとして残る。

### Resolved/direct I64 slot-call fusion (Issue #6315) — 解決済み

resolved/direct call sites の `LoadSlotI64(arg)...; CallResolved/CallInbounds` は
`CallResolvedI64Slots` / `CallInboundsI64Slots` へ畳まれ、I64 slot sidecar から
引数を直接読めるようになった。Base `gcd` と user `mygcd` の VM-only gap は縮小したが、
残差は Base `gcd` 固有の `abs` prefix 認識を増やして埋めるのではなく、generic
`I64Function` block / direct-call interpreter の保守可能な高速化として扱う。この issue の
concrete scope に残る未実装項目はない。

### Generalized resolved-call I64 function blocks (Issue #6314) — 解決済み

generic `I64Function` executable block は、Base `abs(::I64)::I64` 固定opだけでなく、
shape guard を満たす小さな resolved/direct I64 callee を nested block として保持し、
callee frame なしで実行できるようになった。Guard は generated/vararg/keyword/type-param/
非I64 signature/unsupported opcode/深い再帰を除外し、miss 時は従来の frame 実行へ戻る。
この concrete performance issue に残る未実装スコープはない。Base `gcd` と user
`mygcd` の小さく残った差の調査は #6315 で継続する。

### sjulia VM bytecode CLI execution path (Issue #6317) — 解決済み

`sjulia --compile-vm` / `--run-vm-bytecode` と `.sjvmbc` 拡張子自動実行により、
ユーザープログラムの parse/lower/VM bytecode compile を事前化して、一回実行 CLI でも
保存済み `CompiledProgram` を直接 `Vm::run()` に渡せるようになった。この issue の
concrete scope に残る未実装項目はない。残る性能差は CLI process startup 自体、
または VM-only hot path の個別最適化として扱う。

## 最新対応 (2026-06-10)

### Base gcd resolved-call I64 function blocks (Issue #6312) — 解決済み

Base `gcd(::Int64, ::Int64)` の resolved direct call は、callee frame 作成前に
I64 stack arguments から gcd / generic `I64Function` executable block を試すようになった。
Base `gcd` 先頭の `abs(::I64)` prefix も Base/prelude signature guard 付きの `AbsI64`
op として扱う。この concrete performance issue に残る未実装スコープはない。
より広い resolved-call opcode coverage は #6314 で解決済み。Base `gcd` と user
`mygcd` の小さく残った差の調査は #6315 で継続する。

## 最新対応 (2026-06-08)

### Tuple bounded fallback after diagonal miss (Issue #6251) — 解決済み

`Tuple{T,T}` の repeated `where` binding は homogeneous real tuple を diagonal method に dispatch し、
mixed real tuple は independent `Tuple{<:Real,<:Real}` fallback に dispatch する。
anonymous bounded TypeVar `_ <: Real` は repeated binding ではなく、各 tuple slot の独立 bound として扱われる。
non-Real element を含む tuple は `MethodError` になる。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractArray rank-TypeVar diagonal specificity (Issue #6249) — 解決済み

`Type{T}, AbstractArray{T,N}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Vector{Int64}` または `Type{Int64}, Matrix{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractArray{<:Real,N}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractArray{Int64,N}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractArray rank-omitted diagonal specificity (Issue #6247) — 解決済み

`Type{T}, AbstractArray{T}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Vector{Int64}` または `Type{Int64}, Matrix{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractArray{<:Real}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractArray{Int64}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractArray rank-1 diagonal specificity (Issue #6245) — 解決済み

`Type{T}, AbstractArray{T,1}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Vector{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractArray{<:Real,1}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractArray{Int64,1}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractArray rank-2 diagonal specificity (Issue #6243) — 解決済み

`Type{T}, AbstractArray{T,2}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Matrix{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractArray{<:Real,2}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractArray{Int64,2}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractMatrix diagonal specificity (Issue #6240) — 解決済み

`Type{T}, AbstractMatrix{T}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Matrix{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractMatrix{<:Real}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractMatrix{Int64}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/AbstractVector diagonal specificity (Issue #6239) — 解決済み

`Type{T}, AbstractVector{T}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Vector{Int64}` のように一致する場合に fixed
`Type{Integer}, AbstractVector{<:Real}` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, AbstractVector{Int64}` method は
Julia と同じ優先順位を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/matrix diagonal specificity (Issue #6237) — 解決済み

`Type{T}, Matrix{T}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Matrix{Int64}` のように一致する場合に fixed `Type{Integer}, Matrix{<:Real}`
より specific として扱われる。abstract `Type{Integer}` binding と exact
`Type{Int64}, Matrix{Int64}` method は Julia と同じ優先順位を維持する。
この concrete bug に残る未実装スコープはない。より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/vector diagonal specificity (Issue #6235) — 解決済み

`Type{T}, Vector{T}` の repeated `where` binding は、actual pair が concrete
`Type{Int64}, Vector{Int64}` のように一致する場合に fixed `Type{Integer}, Vector{<:Real}`
より specific として扱われる。abstract `Type{Integer}` binding と exact
`Type{Int64}, Vector{Int64}` method は Julia と同じ優先順位を維持する。
この concrete bug に残る未実装スコープはない。より広い morespecific 完備化は #5072 の残スコープとして継続。

### Type/value diagonal specificity (Issue #6233) — 解決済み

`Type{T}, T` の repeated `where` binding は、actual pair が concrete `Type{Int64}, Int64`
のように一致する場合に fixed `Type{Integer}, Integer` より specific として扱われる。
abstract `Type{Integer}` binding と exact `Type{Int64}, Int64` method は Julia と同じ優先順位を維持する。
この concrete bug に残る未実装スコープはない。より広い morespecific 完備化は #5072 の残スコープとして継続。

### Union specificity (Issue #6231) — 解決済み

finite `Union` method は、actual argument が入る Union arm が competing supertype method より
狭い場合に specific として扱われる。`Union{Real,String}` vs `Integer` のように actual arm が
より広い場合は narrower supertype method を維持する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### Vector diagonal specificity (Issue #6229) — 解決済み

`Vector{T}, Vector{T}` の repeated `where` binding は、actual element type が一致する場合に
independent `Vector{<:Real}` bounds より specific として扱われる。mixed element types は
diagonal binding を満たさないため independent-bound method を選ぶ。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### nested Matrix literal rank-aware projection (Issue #6227) — 解決済み

nested array literal の inner rank を保持して、`[[1 2], [3 4]]` は
`Vector{Matrix{Int64}}` として `typeof` / runtime dispatch で扱われる。
この concrete bug に残る未実装スコープはない。より広い array rank / element type
完備化は collection/type preservation 系の残スコープとして継続。

### nested Vector literal runtime dispatch (Issue #6225) — 解決済み

`[[1], [2]]` は outer array の logical element type として `Vector{Int64}` を保持し、
`typeof` / `Any` slot 経由 runtime dispatch で Julia と同じ `Vector{Vector{Int64}}` として扱われる。
この concrete bug に残る未実装スコープはない。より広い morespecific / array element type
完備化は #5072 および collection/type preservation 系の残スコープとして継続。

### invariant Vector TypeVar runtime specificity (Issue #6222) — 解決済み

`f(::T, ::Vector{T}) where {T<:Real}` と `f(::Integer, ::Vector{<:Real})` の competing methods は、
wrapper 経由の runtime dispatch でも Julia と同じ fixed `Integer` / `Vector{<:Real}` method を選ぶ。
この concrete bug に残る未実装スコープはない。より広い morespecific / TypeVar ordering 完備化は
#5072 の残スコープとして継続。

### tuple vararg ambiguity filtering (Issue #6220) — 解決済み

`Tuple{Vararg{Integer}}` と `Tuple{Int64,Vararg{Any}}` の competing methods は、
all-Int actual tuple で Julia と同じ曖昧 `MethodError` になる。empty tuple と mixed tail の
unique dispatch は維持される。この concrete bug に残る未実装スコープはない。
より広い morespecific / ambiguity ordering 完備化は #5072 の残スコープとして継続。

### tuple vararg specificity by actual shape (Issue #6218) — 解決済み

`Tuple{Vararg{Int64}}` と `Tuple{Int64,Vararg{Any}}` の competing methods は、
all-Int actual tuple で Julia と同じく `Tuple{Vararg{Int64}}` method を選ぶ。
mixed tail と same-tail fixed-prefix の挙動も upstream と一致する。この concrete bug に残る未実装スコープはない。
より広い morespecific 完備化は #5072 の残スコープとして継続。

### empty vararg element specificity (Issue #6216) — 解決済み

`xs::Int64...` と `xs::Integer...` の competing unbounded vararg methods は、
空 vararg 呼び出しでも宣言された element type を比較し、Julia と同じく `Int64...` method を選ぶ。
この concrete bug に残る未実装スコープはない。より広い morespecific 完備化は #5072 の
残スコープとして継続。

### generated direct body type-argument execution (Issue #6214) — 解決済み

direct generated body expression の argument name は generated-time type object として実行される。
IR inliner は `@generated` method を通常 runtime call として展開しない。returned Expr payload の
runtime argument evaluation は維持される。この concrete bug に残る未実装スコープはない。
より広い generated staging 完備化は #5074 の残スコープとして継続。

### empty vararg unbound type parameter matching (Issue #6212) — 解決済み

空の `xs::T... where T` 呼び出しで body が `T` を読む場合、`T` は未束縛として
Julia と同じ `UndefVarError` になる。value-only の空 vararg 呼び出しは引き続き `()` を返す。
この concrete bug に残る未実装スコープはない。より広い generated/static parameter 完備化は
#5074 の残スコープとして継続。

### generated Array static parameter body binding (Issue #6210) — 解決済み

generated body で `Array{T,N}` signature 由来の `T` / `N` static parameters を参照できる。
この concrete bug に残る未実装スコープはない。より広い generated staging 完備化は #5074 の
残スコープとして継続。

### generated vararg `$args` interpolation (Issue #6208) — 解決済み

generated syntactic-unquote body の `$args` interpolation は generated-time concrete type tuple として
評価される。この concrete bug に残る未実装スコープはない。より広い generated staging 完備化は
#5074 の残スコープとして継続。

### generated syntactic-unquote default arguments (Issue #6206) — 解決済み

generated syntactic-unquote 済み method を optional positional default wrapper 経由で呼んでも、
runtime 引数は generated-time type object に差し替わらない。この concrete bug に残る未実装スコープはない。
より広い generated staging 完備化は #5074 の残スコープとして継続。

## 最新対応 (2026-06-07)

### generated mixed interpolation/runtime argument refs (Issue #6204) — 解決済み

generated returned code の quote 内で `$arg` interpolation と裸の同名 runtime argument 参照が
同居するケースは、returned-Expr eval へ委譲して Julia と同じ runtime frame lookup を行う。
この concrete bug に残る未実装スコープはない。より広い generated staging 完備化は #5074 の
残スコープとして継続。

### runtime bounded dispatch from Any containers (Issue #6202) — 解決済み

`Any` container 由来の `Type{T}` / `Vector{T}` runtime dispatch は、`where {T<:Integer}` と
`where {T<:Real}` の bounded method specificity を upstream Julia と同じく保持する。
この concrete bug に残る未実装スコープはない。より広い morespecific 完備化は #5926 / #5072 の
残スコープとして継続。

### VM Mandelbrot F64 slot superinstructions (Issue #4301) — 解決済み

Mandelbrot inner loop 向けの `Float64` slot square / load-op superinstructions と runtime fast path は実装済み。
この slice に残る未実装スコープはない。さらなる高速化は slot 型推論改善、loop-local registerization、
precomputed bytecode harness の正式ベンチ化として別 scope。

### Reflection `hasmethod` for LinearAlgebra Diagonal Base extensions (Issue #6124) — 解決済み

`hasmethod(*, Tuple{typeof(F.U), typeof(Diagonal(F.S))})` は upstream Julia と同じく
`true` を返す。Base extension reflection alias に関して、この Issue に残る未実装スコープはない。

### Web Playground startup warmup before first Run (Issue #6127) — 解決済み

Web Playground は Run button を有効化する前に `run_from_source` warmup を完了する。
この Issue に残る未実装スコープはない。

### Android Flutter native build cache embedding (Issue #6126) — 解決済み

Android Flutter native build script は Base bytecode cache と prelude Program cache を生成し、
各 ABI の `.so` build に埋め込む。この Issue に残る未実装スコープはない。

### Flutter mobile Editor Plotly artifact display (Issue #6118) — 解決済み

Flutter mobile Editor は `compile_and_run_detailed` の `CExecutionResult` artifact を
`ExecutionResult.plotlyJSON` として保持し、output pane の `PlotlyView` で表示する。
この Issue に残る未実装スコープはない。

### Android Flutter LinearAlgebra SVD sample failure (Issue #6117) — 未解決

Android Flutter Editor の `Linear Algebra (SVD)` sample は
`U * Diagonal(S) * V'` で `MethodError: no method matching operator(Matrix{Float64},
LinearAlgebra.Diagonal{Float64})` を表示する。この Issue は未解決で、Android runtime /
mobile packaged base / dispatch parity の切り分けが残る。

### Flutter mobile REPL Plotly artifact display (Issue #6115) — 解決済み

Flutter mobile REPL は native `CREPLResult` の `application/vnd.plotly+json` artifact を
履歴へ伝搬し、2D Plotly trace を `PlotlyView` で表示する。この Issue に残る未実装スコープはない。

### Android Flutter REPL UI-thread blocking (Issue #6113) — 解決済み

Flutter mobile REPL の native evaluation は dedicated Dart isolate 上の long-lived
`REPLSession` で実行される。UI isolate は worker response を受け取って履歴を更新するだけなので、
`1 + 1` の初回評価が数秒かかっても ANR を起こさない。この Issue に残る未実装スコープはない。

### Android Flutter REPL keyboard submit (Issue #6110) — 解決済み

Android mobile の Flutter REPL 入力は IME send / physical Enter で既存の評価経路を呼ぶ。
`1 + 1` はキーボード送信だけで `2` を表示できるため、この Issue に残る未実装スコープはない。

### native word-size `Int` / `UInt` alias identity (Issues #6097, #6105) — 解決済み

`Int` / `UInt` の bare type object、constructor、parametric alias は native word size の
concrete type へ解決される。この Issue 群に残る alias identity scope はない。
整数リテラル carrier の全面的な 32-bit native 化は別 scope。

## 最新対応 (2026-06-06)

### direct static no-method call の runtime MethodError (Issue #6007) — 解決済み

statically mismatched direct method call は compilation error ではなく runtime `MethodError` を発生させる。
この Issue に残る未実装スコープはない。

### statically Any argument の lone specific method dispatch (Issue #5984) — 解決済み

`Any` typed value を lone specific method に forward する call は runtime dispatch に残り、runtime value が
method signature と一致しない場合は catchable `MethodError` になる。この Issue に残る未実装スコープはない。

### iOS REPL multiline paste の改行保持 (Issue #6004) — 解決済み

複数行 pasteboard 文字列は `REPLTextField.paste(_:)` で LF 正規化したうえで選択範囲へ挿入され、
REPL 入力欄で 1 行に潰れない。この Issue に残る未実装スコープはない。

### iOS Editor histogram bar trace (Issue #6005) — 解決済み

`histogram(..., bins=...)` は Plotly `bar` trace として生成され、Editor の
`compile_and_run_detailed` FFI 経路でも `"type":"bar"` と bin counts を返すことを回帰テストで固定済み。
この Issue に残る未実装スコープはない。

### REPL import-only 入力の表示値漏れ (Issue #6000) — 解決済み

`using Plots` / `using LinearAlgebra` だけの REPL 入力は `Nothing` result として扱われ、
package load 内部値は表示値、artifact、`ans` に流れない。この Issue に残る未実装スコープはない。

### Plots.surface function-valued z (Issue #5986) — 解決済み

`surface(x,y,zf::Function)` は x/y grid 上で `zf` を sample して matrix-backed `:surface` Series に変換する。
Plotly renderer は既存の matrix surface path で JSON を生成できるため、この Issue に残る未実装スコープはない。

### REPL matrix comprehension global persistence (Issue #5995) — 解決済み

matrix comprehension 結果の `NativeArray` Any carrier は REPL global 注入時に `ArrayLiteral` として
再構成されるようになり、次の評価でも `z[2,3]` / `surface(x,y,z)` が `UndefVarError` にならない。
この Issue に残る未実装スコープはない。

### iOS REPL surface Plotly artifact (Issue #5987) — 解決済み

`surface(x,y,z)` の REPL/FFI artifact 欠落は `LinRange` 座標展開と Array wrapper 経由の
`Plot.series` 抽出で解決済み。この flow が必要とする matrix comprehension global persistence は #5995 で
固定する。
この Issue に残る未実装スコープはない。

### Web test runner の ESM / timeout 互換問題 (Issue #5994) — 解決済み

`web/test-runner.js` は ESM package 内で起動でき、現行 Playwright の timeout options を正しく渡す。
`npm ci` で Playwright devDependency も復元できるため、この Issue に残る未実装スコープはない。

### Web Dice Simulation sample の Float64 `die` slot add 失敗 (Issue #5990) — 解決済み

Web playground の Dice Simulation sample は `integer_floor` helper 経由で `die` を `Int64` に保つようになり、
`sum += die` の integer slot add と衝突しない。この Issue に残る未実装スコープはない。

### `@testset` 内 `pi` ローカルshadowing (Issue #5991) — 解決済み

`@testset` body 内の `pi = ...` は compiler-managed local scope として扱われるようになり、
`Base.pi` / `Main.pi` const と衝突しない。top-level const 再代入エラーは維持済み。
この Issue に残る未実装スコープはない。

### Plots histogram / surface scope (Issue #5575) — 解決済み

Issue #5575 の `surface` は既存の `surface(x,y,z)` / Plotly `surface` trace でカバー済み。
今回 `histogram` / `histogram!` を追加したため、同 Issue としての残 scope はない。
Full upstream Plots.jl の recipe pipeline、`StatsBase.Histogram` object recipe、`histogram2d`、
`barhist`/`stephist`/`scatterhist` などは bundled lightweight Plots の別 parity scope として残る。

## 最新対応 (2026-06-05)

### precise world/backedge invalidation (Issue #5603) — 部分対応

method-table call と function-table call が precise argtypes で dispatch 成功する場合、callee backedge は
`DispatchedMethodEdge { callee, arg_types }` として stamp され、method mutation 時に変更
`MethodSig` と照合されるようになった。これにより `callee(::Float64)` の変更で
`callee(::Int64)` にだけ dispatch した caller cache を retire しない。
method-table no-method/ambiguous/imprecise argtypes、arity/type binding が明確でない
function-table call、method-table `Top` から function-table fallback する経路は引き続き
bare-name edge の安全側 invalidation を使う。
残りは full `MethodInstance` identity、ambiguity graph、world-age、upstream backedge lattice に
相当するさらに細かい invalidation precision を #5603 に継続する。

### subtype engine の CoreType migration (Issue #5615) — 部分対応

compile-time struct parent registry が declared parent template と child type parameter 名を
保持するようになり、`Pairs{K,V,I,A} <: AbstractDict{K,V}` のような non-array
parametric abstract container relation の一部は CoreType 側で判定できるようになった。
`Base.Pairs{Symbol,Int64,...} <: AbstractDict{Symbol,Int64}` は true、K/V の invariant mismatch は
false として固定済み。さらに registered parent Struct-to-Struct pair の invariant mismatch は
runtime CoreType gate が `Some(false)` として確定し、legacy string fallback へ落ちない。
Array / Dict / Set / Range / Ref family が nested `Tuple{...}` covariance の element に現れる場合も、
既存の top-level CoreType authoritative 判定を再利用して true/false を確定し、legacy tuple
string recursion への依存を縮小した。
さらに runtime-decidable な `Union{...}` subtype pair は true だけでなく false も CoreType gate が
`Some(false)` として返すため、legacy Union string parser への依存も一部縮小済み。
concrete `@NamedTuple{...}` は CoreType が field name/type を保持して parse し、field type invariant
subtype と runtime CoreType gate の true/false 判定を担うようになったため、concrete NamedTuple pair も
legacy `JuliaType` fallback への依存を縮小済み。
さらに `NamedTuple{(:a, :b), Tuple{T1, T2}}` の type-level concrete spelling も
`CoreType::NamedTuple` へ正規化し、literal names-only marker への runtime true/false も
CoreType gate で確定するようになった。
対応する NamedTuple 専用 `JuliaType::from_name_or_struct` fallback は `comparison.rs` から削除済み。
さらに CoreType gate 前の `Foo{...} <: Foo` early string shortcut も削除し、bare-family 判定は
nominal/abstract hierarchy 側へ集約済み。
この集約で露出した `Irrational{:π} <: Irrational` の CoreType family gap も修正済み（Issue #5904）。
`Base.Fix1` / `Base.Fix2` も CoreType の known struct family に追加済みで、callable
partial-application 型の bare-family dispatch は generic string shortcut に依存しない（Issue #5127）。
さらに `BitVector` / `BitMatrix` / `BitArray{N}` は CoreType の builtin array-family lattice に入り、
`AbstractVector{Bool}` / `AbstractMatrix{Bool}` / `AbstractArray{Bool,N}` への true と element/rank
mismatch / dense-layer false を runtime CoreType gate が確定するようになった。
対応する BitArray 専用 string fallback は `comparison.rs` から削除済み。
さらに `Type{...}` singleton subtype の string fallback も削除し、`TypeOf` pair は CoreType gate に
一本化済み。
さらに `Tuple{...} <: Tuple` 専用 string shortcut も削除し、bare `Tuple` への runtime true は
CoreType gate が authoritative に確定する。
さらに bare `AbstractArray` / `AbstractVector` / `AbstractMatrix` 専用 string bridge も削除し、
runtime-decidable な array rank relation は CoreType gate が担う。
さらに bare `UnitRange` / `OneTo` / `StepRange` / `StepRangeLen` / `LinRange` から
`AbstractUnitRange` / `AbstractRange` への専用 string shortcut も削除し、
Julia の `LogRange <: AbstractRange == false` を含む bare range family subtype 判定は
CoreType gate が担う。
さらに `Vector{Int64} <: Array{T}` / `Array{<:Real}` のような array-family
UnionAll subtype 判定も CoreType gate が authoritative に扱うようになり、runtime
`check_subtype` の array projection fallback は削除済み。
さらに `Dict` / `Set` / `Ref` family の TypeVar pattern params も
runtime-decidable な CoreType gate に含まれ、key mismatch / bounded TypeVar mismatch の
false も legacy string heuristic に落ちず確定する（Issue #5949 の bounded
`Set`/`Ref` UnionAll false を含む）。
残りはより広い non-array parametric abstract container、`where` / `UnionAll` /
`typeintersect` を含む full subtype relation を CoreType 側へ統合し、文字列 heuristic をさらに
縮小する scope として #5615 に継続する。

## 最新対応 (2026-06-04)

### VM formatting panic-free audit (Issue #5869) — 解決済み

`format_type_name_alias` の non-test `.unwrap()` は non-panicking pattern に置き換え済み。
panic-free VM audit に残る #5869 スコープはない。

### nested `length(copy(::Dict))` legacy Dict mismatch (Issue #5867) — 解決済み

`copy(::Dict)` の runtime 表現と型推論のズレで `length(copy(Dict{String,Int64}()))` が
struct field access に落ちる問題は解決済み。`collections_copy_type_preservation` も upstream Julia 1.12 に
存在しない `copy(::Tuple)` 期待を外したため、この Issue に残る未実装スコープはない。

### subtype engine の CoreType migration (Issue #5615) — 部分対応

runtime `check_subtype` は同一 built-in parametric struct family の invariant false を
CoreType で確定するようになった。さらに primitive / abstract / `Any` / `Union{}` の
built-in lattice leaf は CoreType の true/false を runtime bridge が採用する。
rank-specialized `Array{T,N}` / `DenseArray{T,N}` から bare `AbstractVector` /
`AbstractMatrix` への判定も CoreType に移った。さらに built-in array family 同士の
parametric abstract relation と built-in `Dict` / `Set` の parametric abstract parent relation も
runtime CoreType gate で確定し、built-in range family の parametric abstract parent relation も
CoreType gate で確定する。さらに built-in `RefValue` / `Ref` family の parametric relation も
CoreType gate で確定し、direct built-in parent chain を持つ built-in struct から built-in abstract
への relation も CoreType gate で確定する。さらに runtime `Type{...}` subtype pair も
CoreType gate で確定し、invariant false と `Type{<:Bound}` covariant true/false が legacy
string helper に落ちない。built-in-only `Tuple{...}` covariance pair も CoreType gate で確定し、
さらに compile-time registry に登録された user nominal subtype relation と user struct を含む
`Tuple{...}` covariance pair も CoreType gate で確定する。さらに concrete user-parametric
struct から登録済み user abstract parent への relation と、それを含む `Tuple{...}` covariance
pair も CoreType gate で確定する。さらに user-parametric abstract parent の where-right
supertype relation（`MyVec{Int} <: Wrapper{S} where S`）は runtime reflection supertype chain を
CoreType solver に再投入して判定する。さらに `SubArray` / `ReshapedArray` wrapper-array relation も
CoreType gate に移り、`AbstractArray{T,N}` / rank alias への true と dense layer への false を
structured solver 側で固定する。さらに built-in range family も CoreType gate に寄せ、
`LogRange{T} <: AbstractRange` の upstream false を authoritative に固定した。さらに
`AbstractRange <: AbstractVector <: AbstractArray` を built-in hierarchy に反映し、range family の
`AbstractVector{T}` / `AbstractArray{T,1}` relation も invariant element/rank 付きで CoreType gate に
移した。残りは他の non-array
parametric abstract container、`where` / `UnionAll` /
`typeintersect` を含む full subtype relation を
CoreType 側へ統合し、文字列 heuristic をさらに縮小する scope として #5615 に継続する。

### CFG/worklist production inference path (Issue #5602) — 部分対応

CFG observation pass は block input/output を記録し、`if` / `while` の branch edge metadata と
edge transfer により `split_env_by_condition` の then/else narrowing を successor input へ流すようになった。
さらに worklist backedge の再訪問で loop-carried variable state を join する observation
case も固定した。構造化制御フローを含まない単一 basic block の return は CFG payload order
を authoritative path として使うようになり、CFG lowering は `return` を terminal edge として扱う。
all-returning `if` の小さい slice では multi-block CFG return も authoritative path に移った。
positional call payload は既存 call 推論を使って CFG return path に乗るようになった。残りは
`isa` condition の CFG edge narrowing も authoritative return path に反映されるようになった。
`nothing` identity condition の CFG edge narrowing も authoritative return path に反映されるように
なった。`typeof` condition の CFG edge narrowing も authoritative return path に反映されるように
なり、short-circuit ではない binary expression payload と `-x` / `!x` の unary expression payload、
supported condition/arm だけからなる ternary payload も CFG return path に乗るようになった。
`obj.field` field-access payload と scalar/slice/range index payload も CFG return path に乗るようになった。
named keyword call payload も CFG return path に乗るようになった。
static tuple positional splat call payload も CFG return path に乗るようになった。
static NamedTuple keyword splat call payload も CFG return path に乗るようになった。
array literal payload も CFG return path に乗るようになった。
tuple/NamedTuple literal payload も CFG return path に乗るようになった。
Pair/Dict construction payload も CFG return path に乗るようになった。
simple LetBlock payload も CFG return path に乗るようになった。
残りは dynamic positional/keyword splat call、より複雑な expression payload を含む multi-block return、
`while` / `break` / `continue` / `for` / `try` / `&&` / `||` の total control-flow lowering、
SSA linearization と explicit Phi value typing を #5602 に継続する。

### Matrix `view` の 2D SubArray surface (Issue #5137, Bugs #5812/#5814) — 部分対応

Matrix の range/range view と Colon-backed view (`view(A, 1:2, :)`, `view(A, :, 2:3)`,
`view(A, :, :)`) は 2D `SubArray{T,2,...}` として型 surface、indexing、parent aliasing、
2D `collect(::SubArray)` の Matrix shape copy を固定済み。coarse `Array` inference で
public `collect(view(...))` が `collect(::Array)` に落ちる bug #5812 も修正済み。
Colon dimension は `Slice(OneTo(size(A, dim)))` に正規化する。実装中に発見した
`OneTo` equality bug #5814 は別途 open。
1D range parent の `view(r, inds::UnitRange)` は upstream と同じ range slice (`r[inds]`) に
対応済み。
残りは arbitrary-rank / Colon 以外の non-UnitRange indices、full `ReshapedArray` behavior
として #5137 に継続する。Bool value parameter `false` の constructor bug は #5779 で修正済み。

### `merge(::NamedTuple, ::NamedTuple...)` (Issue #5687) — 解決済み

静的に field 名が分かる `@NamedTuple{...}` 引数について、compiler が本家 generated
merge 相当を合成するようになった。`merge((a=1,b=2), (b=20,c=3))`、変数束縛経由、
3 引数、空 NamedTuple（`NamedTuple{()}(())`、Issue #5776 workaround）、`typeof` は
`tuple/namedtuple_merge_5687.jl` で固定済み。
残りの完全動的 `NamedTuple{names}(vals)` construction（computed `names`）は別 scope
として継続する。

### predicate function negation `!f` (Issue #5672) — 解決済み

`!iseven` は Bool ではなく `(!) ∘ iseven` 相当の callable として扱うようになった。
`filter(!iseven, [1,2,3,4])`、`map(!isnothing, Any[1,nothing,2])`、`map(!!iseven, ...)`
の代表ケースは `hof/function_negation_5672.jl` で固定済み。残りの exact
`typeof(!f) == ComposedFunction{typeof(!), typeof(f)}` surface は runtime 表示/型名の
精密化 scope に残す。

### Vector/Matrix as Array{T,N} dimensional aliases (Issue #5593) — 解決済み

`Array` / `Vector` / `Matrix` と `DenseArray` / `DenseVector` / `DenseMatrix` の
runtime type-object surface は本家と同じ次元エイリアスを保持するようになった。
`Base.unwrap_unionall(Vector)` は `Array{T, 1}`、`Base.unwrap_unionall(Array)` は
`Array{T, N}`、`DenseVector{Int} === DenseArray{Int,1}` も成立する。
fixture `reflection/array_dimensional_alias_5593.jl` で固定済み。

### function/lambda parameter destructuring (Issue #5760) — 解決済み

`function f((a,b)) ... end`、`f((a,b)) = ...`、`((a,b),)->...` の
parameter destructuring は lowering の synthetic parameter prologue で対応済み。
Tuple と Pair の代表ケースは `functions/parameter_destructuring_5760.jl` で固定した。

## 最新対応 (2026-06-03)

### SubArray reshape regression の解消を確認 (Issue #5611) — 解決済み

five-parameter `SubArray` surface の `reshape(::SubArray, dims...)` regression は
current worktree で再現しない。direct fixture と `subarray::chunk_000` は pass 済み。
残りの arbitrary-rank `SubArray` と full `ReshapedArray` behavior は broader #5137 後続
scope に集約する。1D range parent の `view(r, inds::UnitRange)` は upstream と同じ range
slice に対応済み。

### typed integer value-parameter carriers を保持 (Issue #5616) — 解決済み

`Val{UInt8(1)}` / `Val{Int32(2)}` の static type construction、`.parameters`
reflection、`Base.infer_return_type` binding は carrier type を保持するようになった。
残りの value-parameter 表現拡張は float / arbitrary bits value parameter 全般と、
broader subtype/dispatch consumer sweep の後続 scope に集約する。

### abstract sibling dispatch unit expectation を更新 (Issue #5617) — 解決済み

親情報なしの concrete struct は sibling user abstract に保守的に一致しない。
`struct_parents` がある場合のみ `Car <: MotorVehicle` の ancestry を使って正しい
sibling method を選ぶ。full method specificity の残件は #5072 後続 scope に集約する。

### while-loop LICM zero-iteration guard を修正 (Issue #5620) — 解決済み

`while` body expression は condition が false の場合に実行されないため、IR optimizer
は body expression を condition より前へ hoist しない。残りの LICM 拡張は、
loop が少なくとも 1 回実行されることを静的に証明できる限定ケースに集約する。

### expression-context macro gensym local binding を修正 (Issue #5625) — 解決済み

expression context の user macro でも macro body assignment は展開時 local binding
として評価される。`tmp = gensym("tmp")` と quote 内 `$tmp` interpolation は同じ
hygienic generated variable に解決される。残りの macro expansion gap は未対応
`Expr` head / evaluation primitive ごとの個別 Issue に集約する。

### deg2rad/rad2deg irrational/Real 経路を修正 (Issue #5626) — 解決済み

`deg2rad` / `rad2deg` は `AbstractFloat` と `Real` overload を持ち、irrational
singleton は `float` 変換後に計算される。残りの angle / trig parity gap は
個別の math fixture Issue に集約する。

### isapprox irrational scalar 経路を修正 (Issue #5630) — 解決済み

`isapprox` は `AbstractIrrational` を scalar float boundary へ変換して比較する。
残りの tolerance / keyword parity gap は個別 Issue に集約する。

### math category timeout を解消 (Issue #5629) — 解決済み

`math::chunk_000` の timeout は、残っていた irrational / range / power / `mod2pi`
fixture failure を解消したことで再現しなくなった。残りの math category blocker は
新しい個別 bug Issue として扱う。

### duplicate mod2pi irrational dispatch report を吸収 (Issue #5632) — 解決済み

`math/mod2pi_rem2pi.jl` の duplicate irrational-Float64 dispatch report は
#5633/#5634 の対応で解消済み。残りの `rem2pi` 完全 parity は single-argument
sjulia extension ではなく upstream-compatible two-argument API の個別拡張として扱う。

### log(base, x) と irrational power を修正 (Issue #5631) — 解決済み

`DynamicPow` は `Irrational` singleton を Float64 境界へ変換し、`log(b, x)` は
upstream と同じ整数境界結果を返す。残りの transcendental exactness gap は個別の
math parity Issue に集約する。

### mod2pi irrational-Float64 dispatch を修正 (Issue #5633) — 解決済み

`mod2pi(::AbstractFloat)` は `2.0 * pi` などの Float64 入力を numeric path で処理する。
残りの `rem2pi` kernel parity は upstream API surface の別 Issue として扱う。

### mod2pi fixture upstream parity を修正 (Issue #5634) — 解決済み

`math/mod2pi_rem2pi.jl` は upstream Julia で成立しない `mod2pi(pi)` と one-argument
`rem2pi` expectations を持たなくなった。Fixture は upstream-compatible `mod2pi`
Float64 cases に限定する。

### pythagorean identity の irrational range conversion を修正 (Issue #5635) — 解決済み

`Irrational{:π}` singleton は VM numeric stack coercion で Float64 として扱われ、
`-pi:0.01:pi` の lazy range construction が通る。残りの irrational conversion gap は
個別の numeric conversion Issue に集約する。

### approximate comparison hang の解消を確認 (Issue #5636) — 解決済み

`asec(-1.0) ≈ pi` の reduced probe は 60 秒 timeout guard 内で pass する。
残りの `isapprox` keyword / tolerance parity は個別 Issue に集約する。

### Memory dynamic Any scalar multiplication を修正 (Issue #5627) — 解決済み

`Memory * scalar` / `scalar * Memory` は `Any` 経由でも VM dynamic multiplication
path に入る。残りの Memory-array lattice gap は #3908 / Memory-first migration の
後続 scope に集約する。

### matrix-matrix dispatch rank preservation を修正 (Issue #5628) — 解決済み

`zeros` / `ones` / `fill` の rank-aware JuliaType 推論と linalg `*` candidate rank
filter により、`Matrix * Matrix` は `AbstractMatrix * AbstractVector` extension に
誤 dispatch しない。残りの full array-lattice / method specificity gap は
#5072 / #5048 後続 scope に集約する。

## 最新対応 (2026-06-02)

### quote 内 const declaration lowering を追加 (Issue #5578) — 解決済み

macro が返す `quote ... end` block 内の `const $(esc(sym)) = value` は lowering
可能になった。quote constructor は `Expr(:const, Expr(:(=), ...))` を生成し、
macro expansion で const metadata declaration + assignment に戻す。残りの macro
quote gaps は未対応 head ごとの個別 Issue に集約する。

### fixture category `Union` binding blocker の解消を確認 (Issue #5581) — 解決済み

`type_inference::` / `macros::` category を止めていた `Undefined variable: Union`
regression は current worktree で再現しない。Issue に挙がっていた direct fixtures と
`type_inference` 3 chunks / `macros::` は pass 済み。残りの category-wide blockers は
個別 bug Issue に集約する。

### nullable ternary callee inlining regression を修正 (Issue #5589) — 解決済み

`f(x::Union{Int,Nothing}) = x === nothing ? 0 : x + 1` のような branchy body を
`f(nothing)` call-site に small pure IR inline し、未到達 `else` arm を
`Nothing + 1` として compile していた regression は解消済み。`?:` body は
branch elimination 対応まで inline candidate から外す。残りの inlining 拡張は
branch elimination / branch-local argument narrowing を伴う #5184 後続 scope に集約する。

### user-defined function `nameof` compile regression を修正 (Issue #5580) — 解決済み

Small pure IR inlining が Base の higher-order 関数内ローカル `f(...)` を
user-defined top-level `f(x) = x + 1` と誤認する regression は解消済み。
`nameof(::Function)` の user-defined function surface は
`reflection/nameof_user_function_5580.jl` で固定した。残りの `nameof` 課題は
既存の `nameof(::Module)` / `Core.TypeName` 完全モデルに集約する。

### bare TypeVar where body collapse を修正 (Issue #5570) — 解決済み

`T where T` は `Any`、`T where T<:Real` は `Real` へ upstream と同じく縮約する。
残りの `where`/`UnionAll` 課題は broader subtype.c parity (#5047/#5048) と
array-family surface quirks に集約する。

### compile-time `isa` false-fold for structs を修正 (Issue #5579 / #5585) — 解決済み

`Struct(_)` の non-identical `isa` を compile-time に false へ畳み込まないようにした。
`Irrational{:π}() isa Irrational` の order-dependent regression と、
`types_tests::chunk_000` の #5585 blocker は解消済み。残りの broader 課題は
MethodInstance/world-age/cache semantics (#4271) と full method specificity (#5072) に集約する。

### `SubArray` Int8 1D view surface を修正 (Issue #5583) — 解決済み

`view(::Vector{Int8}, ::UnitRange)` は `SubArray{Int8,1,Vector{Int8},...}` として
materialize し、`eltype` / `isa AbstractVector{Int8}` / `collect` も upstream と一致する。

### bit-packed `BitArray` backend と higher-rank materialization を追加 (Issue #5498) — 解決済み

`BitArray{1} === BitVector`、`BitArray{2} === BitMatrix`、`BitVector <: AbstractVector{Bool}`、
`BitMatrix <: AbstractMatrix{Bool}`、bit-packed Bool storage、0D / higher-rank `trues` /
`falses`、typed `similar(..., Bool, dims...)`、および Bool-result broadcast の
rank-aware BitArray-family materialization は対応済み。

### direct power return widening regression を修正 (Issue #5608) — 解決済み

`N^2` の direct compile case は `Int64` result を保持するようになった。Untyped parameter を含む
power return は `Any` return に残し、runtime `DynamicPow` result を Float64 へ強制しない。
残りの power / numeric promotion 課題は broader numeric dispatch parity scope に集約する。

### ternary nullable `nothing` branch narrowing を修正 (Issue #5609) — 解決済み

ternary expression は then/else branch codegen 時に flow-sensitive narrowing を使うようになった。
`x === nothing ? 0 : x + 1` は else 側で `x::Int64` として compile されるため、
`Nothing` を `I64` へ変換しようとしない。残りの flow-sensitive inference 課題は CFG/worklist
branch transfer の #4267 後続 scope に集約する。

### IR inliner local call-target hygiene を修正 (Issue #5612) — 解決済み

Small pure IR inliner は local/bound call target と同名の top-level function を誤 inline しない。
残りの inlining 拡張は branch elimination と branch-local argument narrowing を伴う #5184 後続 scope
に集約する。
### method_table abstract sibling no-parent expectation を修正 (Issue #5617) — 解決済み

親 metadata のない method_table unit が concrete struct を sibling abstract methods に
仮マッチさせて ambiguity を期待する古い前提は解消済み。親 metadata が入るまでは
`NoMethodFound`、入った後は具体 parent chain で dispatch 成功を期待する。

### LICM の dispatchable arithmetic preheader hoist を修正 (Issue #5618) — 解決済み

IR-level LICM が `while x !== nothing; return x + 1; end` の body expression を
条件判定前へ巻き上げ、`f(nothing)` で `Nothing + 1` を評価する regression は解消済み。
型証明なしの dispatchable expression は `while` preheader hoist せず、残りの LICM 拡張は
typed/no-throw proof を伴う optimization scope に集約する。`for` / `foreach` の
pure-expression LICM は維持する。

### macro-local `gensym` interpolation を修正 (Issue #5621) — 解決済み

user-defined macro の multi-statement expansion でも expansion-local assignment を追跡し、
compile-time `gensym` result を quote interpolation へ反映する。残りの macro gaps は
未対応 Expr head / hygiene case ごとの個別 Issue に集約する。

### `rad2deg(π)` / unary `AbstractIrrational` math を修正 (Issue #5622) — 解決済み

`-(::AbstractIrrational)` と `rad2deg(π)` の compiled Base path は upstream parity を
回復した。残りの irrational arithmetic は broader numeric promotion / dispatch parity に集約する。

### `math::chunk_000` timeout blockers を修正 (Issues #5629/#5631/#5632/#5635/#5636) — 解決済み

`log(b, x)` の整数近傍丸め、`mod2pi` / `rem2pi` の irrational `pi` conversion、
lazy range の irrational endpoint conversion、および irrational `abs` / `min` / `max` /
`isapprox` delegation は対応済み。`math::chunk_000` は timeout せず pass する。

### Memory scalar dynamic multiply を修正 (Issue #5623) — 解決済み

Memory-scalar / scalar-Memory `*` は `DynamicMul` に進む。Memory-Memory `*` は未対応のまま、
GenericMemory / AbstractVector arithmetic の broader parity scope に残す。

### array constructor rank inference と matrix dispatch を修正 (Issue #5624) — 解決済み

`zeros` / `ones` / `fill` の rank-aware JuliaType inference と array-array `*` の
runtime rank-aware dispatch により、matrix RHS は `AbstractVector` overload に誤 dispatch しない。
残りは full array-constructor shape/value-parameter inference parity に集約する。

### IR-level LICM の nested hoist temp 未定義参照を修正 (Issue #5592) — 解決済み

内側 LICM が生成した `__sjulia_licm_*` temp は外側 loop の mutation set に再反映されるようになった。
外側 LICM は inner-loop-local temp 依存式を preheader へ巻き上げないため、
FFI Mandelbrot sample の `UndefVarError: __sjulia_licm_* not defined` は解消済み。

### top-level `idx = [...]` の Base-local 型汚染を修正 (Issue #5590) — 解決済み

Base prelude と user main の境界を compiler が保持し、user main で代入される名前を
Base/stdlib 関数と Base main の型収集・compile 中だけ global 型 map から shadow する。
そのため user global `idx::Vector{Int64}` は Base-local `idx::Int64` slot に流れず、
no-`using Test` の `idx = [1, 2, 3]` と `idx = findall(...)` は正常に compile/run する。

### numeric value parameter / SubArray reshape / stale view dispatch regressions を修正 (Issues #5586, #5587, #5588) — 解決済み

explicit parametric constructor 内の numeric value parameter は値として bytecode 化されるようになり、
`reshape(::SubArray, dims...)` は `ReshapedArray` wrapper として parent/shape/indexing を提供する。
また local array 名の再代入では古い precise JuliaType metadata を削除するため、
`Vector{Int64}` から `Vector{Int8}` へ再利用した後の `view` は正しい `Int8` element type で dispatch する。

### nested inner `end` と `Iterators.take` collect trait dispatch を固定 (Issues #5604, #5605) — 解決済み

nested array の inner `end` は VM-native Array の `lastindex` / `getindex` として処理され、
Pure Julia struct field access へ誤って落ちない。`collect(Iterators.take(1:4, 2))` も
`UnitRange` + `Take` + iterator trait の `_collect` dispatch で compile/run する。

### Complex Array wrapper の `valtype` が `Any` に落ちる退行を修正 (Issue #5606) — 解決済み

Memory-first Pure Julia Array wrapper に対する retained `valtype` builtin fallback は、
wrapper の `Array{T,N}` projection から `T` を返すようになった。
`valtype(zeros(Complex{Float64}, 2)) == Complex{Float64}` は本家と一致する。

### Any 経由 `!=` on Type values の dispatch-first equality 退行を修正 (Issue #5555) — 解決済み

`a::Any, b::Any` の `a != b` でも Type object equality method を検出し、
`==` + `NotBool` 経由で user-defined `==(::Type{T}, ::Type{T})` を反映するようになった。
残りは #4298 の broader fallback audit（String ordering / generic DataType・UnionAll など）に集約する。

## 最新対応 (2026-06-01)

### predicate / Bool broadcast の `BitVector` materialization を追加 (Issue #5484) — 解決済み

predicate broadcast と Bool-result generic broadcast は `BitVector` container type を返すようになった。
bit-packed `BitArray` storage と `BitMatrix`/broader BitArray API は #5498 で解消済み。
### #5165 VM 高速化 第2弾の残スコープ (#5173/#5184/#5185 実装後)

#5173 により `StoreSlot` で immutable struct を inline 保持し、#5184 により
top-level/module-qualified small pure expression function の bounded IR inlining を追加し、
#5185 により bytecode CFG / instruction effects / duplicate-load local CSE / IR-level
pure expression CSE / loop-invariant pure expression hoist を追加した。`performance` label
付きの直接 Issue は #5173/#5184/#5185 の実装で解消可能な状態になった。

今後 #5165 で別途扱う余地のある拡張:

- nested/local function の inlining、複数メソッドの型一致に基づく selective inlining、
  kwargs/varargs body の展開、call を含む callee body 全般の effect-propagated inlining。
- bounds-proven `getindex` の CSE / LICM、multi-block SSA 形式の value numbering、
  cost model と benchmark-driven threshold tuning。
- #5165 Epic 全体の checklist 整理・子 Issue 完了確認。

### union-split specialize_block の expression statement emit を追加 (Issue #5077, Epic #5097)

`specialize_block` の context-free emitter は typed に lower できる expression statement も
`Pop` 付きで生成できるようになった。残りは `SpecializedPath` を実際の本体 codegen pipeline に接続し、
control-flow / slot allocation / method dispatch を伴う block emission と merge を有効化する作業である。

### qualified reduction HOF の `init=` keyword rewrite を追加 (Issue #5541, #5094, Epic #5097)

`Base.reduce` / `Base.foldl` / `Base.foldr` と
`Base.mapreduce` / `Base.mapfoldl` / `Base.mapfoldr` の `init=` keyword 呼び出しも、
unqualified form と同じ positional rewrite に入るようになった。
残りは reduction loop 本体の devirtualize、非代表 callable の自動特殊化、および broader MethodInstance-style
multiversioning である。

### `Base.oneto(length/lastindex(a))` loop の bounds-check 除去 proof を追加 (Issue #5089, Epic #5097)

`Base.oneto(length(a))` / `Base.oneto(Base.lastindex(a))` の loop index も
`Base.OneTo(...)` と同じ proven in-bounds path に入るようになった。
残りは多次元 shape-aware proof、非 1-based axes、alias 解析を伴う mutation-aware proof である。
### union-split specialize_block の Bool 比較 bytecode emit を追加 (Issue #5077, Epic #5097)

`specialize_block` の context-free emitter は Bool の `==` / `!=` も typed bytecode として
生成できるようになった。残りは `SpecializedPath` を実際の本体 codegen pipeline に接続し、
control-flow / slot allocation / method dispatch を伴う block emission と merge を有効化する作業である。

### `Base.OneTo(length/lastindex(a))` loop の bounds-check 除去 proof を追加 (Issue #5089, Epic #5097)

`Base.OneTo(length(a))` / `Base.OneTo(Base.lastindex(a))` の loop index も
`1:length(a)` / `1:lastindex(a)` と同じ proven in-bounds path に入るようになった。
残りは多次元 shape-aware proof、非 1-based axes、alias 解析を伴う mutation-aware proof である。

### qualified `Base.map` / `Base.broadcast` の HOF return inference を追加 (Issue #5094, Epic #5097)

qualified `Base.map` / `Base.broadcast` 呼び出しも bare form と同じ HOF return inference に入り、
inline lambda の result type が `Vector{T}` metadata に反映されるようになった。
残りは nested broadcast、一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。

### qualified reduction HOF の return inference を追加 (Issue #5094, Epic #5097)

qualified `Base.reduce` / `Base.foldl` / `Base.foldr` と
`Base.mapreduce` / `Base.mapfoldl` / `Base.mapfoldr` も bare form と同じ HOF return inference に入り、
inline lambda reducer / mapper の result type が function return metadata に反映されるようになった。
残りは reduction loop 本体の devirtualize、非代表 callable の自動特殊化、および broader MethodInstance-style
multiversioning である。

### `typeof(x) === T` guard の branch codegen narrowing を追加 (Issue #5077, Epic #5097)

`typeof(x) === T` / `typeof(x) == T` / reversed operand と lowering 後の `!(typeof(x) === T)` は、
`isa` guard と同じ branch-local codegen narrowing に入るようになった。
残りは `union_split` モジュールの `SpecializedPath` を本体 codegen pipeline に接続し、
specialized block emission / merge を実際の関数コンパイルで消費する作業である。

### ユーザー定義型の多段 abstract 祖先チェーン (Issue #5056) — dispatch 解決済み

多段のユーザー abstract 型を経由する祖先チェーン (`struct Tiny <: MyInt`,
`MyInt <: MyNum`, `MyNum <: Number`) からの `f(::Number)` dispatch が解決された
(STATUS.md / DONE.md を参照)。`supertype` / 推移的 `<:` / `isa` は従来から動作。

残課題: 本対応は `compile/method_table.rs` の祖先ウォークにユーザー abstract 親
リンク (`abstract_parents`) を追加するもので、構造化された統一 subtype エンジン
(Issue #5047) には依存していない。#5047 が入れば、この dispatch 専用ウォークと
推論コア側の registry walk (Issue #5383) を統合して重複を解消できる。
### 型演算子 builtin の引数個数検証における残課題 (Issue #5493)

- 変数束縛/第一級関数値経由の `<:` / `>:` / `isa` 呼び出しは引数個数を検証するようになった（過少/過多で `<:`/`isa` は `ArgumentError`、`>:` は `MethodError`）。
- ただし構文形 `isa(...)` を直接書いた場合は、コンパイル時の専用チェック（`compile/expr/builtin.rs`）が `"isa requires exactly 2 arguments"` のコンパイルエラーを出すため、upstream の実行時 `ArgumentError` とは別経路のまま。
- `>:` の `MethodError` メッセージは引数型名を `DataType` と表示し、upstream の `Type{Number}` 表記とは差異がある（型オブジェクトの `get_type_name` の既存制約）。
### 解決済み: 型アサーション失敗時の `TypeError` expected/got・メッセージ (Issue #5146)

`x::T` / `typeassert(x, T)` 失敗時に `TypeError.got` が型を保持し、メッセージが生表現で
露出していた問題を解消。`got` に値を保持し、`showerror` を本家準拠に整形（残課題なし）。
### `axes(a, 1)` loop の bounds-check 除去 proof を追加 (Issue #5089, Epic #5097)

`axes(a, 1)` / `Base.axes(a, 1)` の単一配列 loop は証明済み index として
`IndexLoad*Inbounds` / `IndexStoreInbounds` に lower されるようになった。
残りは multi-dimensional axes proof、mutation invalidation の精密化、および user-extensible dispatch を保った
generic indexing call path への証明伝播である。

### `Random.Xoshiro` qualified call と `randn(rng)` を修正 (Issue #5436, Epic #5097)

`Random.Xoshiro(seed)` と `Random.randn(rng)` は bare `Xoshiro(seed)` / `randn(rng)` と同じ builtin path に入り、
`randn(rng)` は `Float64` result を返せるようになった。
残りの Random parity は distribution coverage、array-valued RNG methods、および upstream Random API の未実装範囲である。

### direct `@inbounds` indexed assignment lowering を追加 (Issue #5523, #4286, Epic #5097)

`@inbounds a[i] = v` は inbounds context 付き direct `setindex!` call へ lower されるようになった。
残りは statement/block body への `@inbounds` context 伝播、`@boundscheck` body の inference/effect 統合、
`@propagate_inbounds` の call-chain 伝播、`@nospecialize` / `@specialize` の cache-key 統合である。

### local `@inbounds` indexing codegen を追加 (Issue #4286, Epic #5097)

`@inbounds a[i]`、direct `getindex`、direct `setindex!` の supported single-index path は
inbounds 命令へ lower されるようになった。
残りは statement/block body への `@inbounds` context 伝播、
`@boundscheck` body の inference/effect 統合、`@propagate_inbounds` の call-chain 伝播、
`@nospecialize` / `@specialize` の cache-key 統合である。

### direct `getindex` call path の typed bounds-check 除去を追加 (Issue #5089, Epic #5097)

`getindex(a, i)` / `Base.getindex(a, i)` の builtin path も typed array では
`IndexLoadTyped` / `IndexLoadTypedInbounds` を emit するようになった。
残りは multi-dimensional axes proof、mutation invalidation の精密化、および user-extensible dispatch を保った
generic indexing call path への証明伝播である。

### `firstindex` / `lastindex` loop の bounds-check 除去 proof を追加 (Issue #5089, Epic #5097)

`firstindex(a):lastindex(a)` / `Base.firstindex(a):Base.lastindex(a)` の単一配列 loop も
証明済み index として `IndexLoad*Inbounds` / `IndexStoreInbounds` に lower されるようになった。
残りは multi-dimensional axes proof、mutation invalidation の精密化、および user-extensible dispatch を保った
generic indexing call path への証明伝播である。

### HOF binary `map` / `broadcast` の `min` / `max` typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(min/max, A, B)` は same-eltype numeric/Bool vectors で typed binary map helper へ入り、
`broadcast(min/max, A, B)` は same-length numeric vectors で typed binary map path へ入るようになった。
Bool-result broadcast の container parity は #5484 で解消済み。
残りは nested broadcast、一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。

### qualified / `lastindex` loop の bounds-check 除去 proof を追加 (Issue #5089, Epic #5097)

`Base.eachindex(a)`、`1:Base.length(a)`、`1:lastindex(a)` / `1:Base.lastindex(a)` の単一配列 loop も
証明済み index として `IndexLoad*Inbounds` / `IndexStoreInbounds` に lower されるようになった。
残りは multi-dimensional axes proof、mutation invalidation の精密化、および user-extensible dispatch を保った
generic indexing call path への証明伝播である。

### HOF n-ary `map` / `broadcast` の `min` / `max` typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(min/max, A, B, C, As...)` と `broadcast(min/max, A, B, C, As...)` は same-eltype Vector varargs でも
typed min/max loop へ入るようになった。残りは nested broadcast、一般 closure の loop devirtualize、
および非代表 callee の自動特殊化である。

### `setindex!` call path の証明済み index store bounds-check 除去を追加 (Issue #5089, Epic #5097)

direct `setindex!(a, v, i)` call も証明済み単一 index では `IndexStoreInbounds` に lower されるようになった。
残りは multi-dimensional axes proof、mutation invalidation の精密化、および user-extensible dispatch を保った
generic indexing call path への証明伝播である。

### 証明済み index store の bounds-check 除去を追加 (Issue #5089, Epic #5097)

`for i in eachindex(a)` と `for i in 1:length(a)` の同一 array `a[i] = v` は
`IndexStoreInbounds` に lower されるようになった。残りは multi-dimensional axes proof、
mutation invalidation の精密化、generic `getindex` / `setindex!` call paths への証明伝播である。

### HOF n-ary `map` / `broadcast` の `*` typed helper 特化を追加 (Issue #5094, #5505, Epic #5097)

`map(*, A, B, C, As...)` と `broadcast(*, A, B, C, As...)` は same-eltype Vector varargs でも
typed multiplication loop へ入るようになった。`dynamic_mul` の `Int32 * Int32` gap (#5505) も解消し、
generic n-ary fallback 経由の Int32 multiply が runtime error にならないようになった。
残りは `+`/`*` 以外の n-ary callable、nested broadcast、および一般 closure の loop devirtualize である。

### HOF n-ary `broadcast(+, ::Vector...)` の 5 operand 以上を typed helper 化 (Issue #5094, Epic #5097)

`broadcast(+, A, B, C, As...)` は same-eltype Vector varargs でも typed plus loop へ入るようになり、
5 operand 以上と singleton expansion の代表ケースを generic materialize pipeline から外した。
残りは `+`/`*` 以外の n-ary callable、nested broadcast、および一般 closure の loop devirtualize である。

### n-ary Int32 broadcast singleton fallback の dynamic add を修正 (Issue #5499, Epic #5097)

`broadcast(+, ::Vector{Int32}, singleton ::Vector{Int32}, ::Vector{Int32})` の fallback path は
runtime dynamic add の `Int32 + Int32 -> Int32` により Julia 互換の `Vector{Int32}` result を返せるようになった。
残りは `+` 以外の n-ary callable、nested broadcast、および一般 closure の loop devirtualize である。

### HOF n-ary `broadcast(+, ::Vector...)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(+, A, B, C/D)` は representative same-length Int32/Float32/Bool vectors で typed n-ary `map`
経路へ入り、direct broadcast 呼び出しの result metadata も typed helper path と一致するようになった。
残りは `+` 以外の n-ary callable、nested broadcast、および一般 closure の loop devirtualize である。

### HOF n-ary `map(+, ::Vector...)` の call-site inference を追加 (Issue #5094, Epic #5097)

`map(+, A, B, C...)` は representative same-element numeric/Bool vectors で n-ary HOF call-site inference に入り、
direct vararg map 呼び出しの result metadata も typed helper path と一致するようになった。残りは `+` 以外の
n-ary callable、一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。

### HOF binary `map(::Vector, ::Vector)` の call-site inference を追加 (Issue #5094, Epic #5097)

`map(f, A, B)` は representative numeric/Bool callable combinations で binary HOF call-site inference に入り、
direct map 呼び出しの result metadata も typed helper path と一致するようになった。残りは n-ary map の
一般化、一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。
### nested parametric container への `convert` の要素型保持 (Issue #5111, umbrella #5073)

`convert(Vector{T}, x)` の再帰実装は完了 (Issue #5111, DONE.md 参照)。ただし要素型自体が
parametric container の場合（例 `convert(Vector{Vector{Float64}}, [[1,2],[3,4]])`）は、
element-type carrier が `Vector{Float64}` を要素型として保持できず結果が `Vector{Any}` に縮退する。
scalar 要素型 (`Vector{Float64}` / `Vector{Int}` 等) は厳密に保持される。type-loss umbrella #5073 で追跡。

### HOF binary `broadcast(/, ::Vector{Float*}, ::Vector{Float*})` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(/, A, B)` は Float32/Float64 same-length vectors でも typed binary `map`
経路へ入り、Float32 入力では `Vector{Float32}`、Float64 入力では `Vector{Float64}` metadata へ揃うようになった。
残りは nested broadcast、一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。

### HOF binary `broadcast(/, ::Vector, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(/, A, B)` も代表 concrete integer/Bool same-length vectors では typed binary `map`
経路へ入り、result metadata は `Vector{Float64}` へ揃うようになった。残りは nested broadcast、
一般 closure の loop devirtualize、および非代表 callee の自動特殊化である。

### union-split specialize_block の assignment bytecode emit を追加 (Issue #5077, Epic #5097)

`specialize_block()` は straight-line assignment + return の primitive fragment を typed bytecode に
emit できるようになった。残りは分岐/loop を含む block、method table を要する call、
array access、slot allocation を共有する main compiler との統合である。

### 証明済み typed index load の bounds-check 除去を追加 (Issue #5089, Epic #5097)

`for i in eachindex(a)` と `for i in 1:length(a)` の同一 typed array `a[i]` は
`IndexLoadTypedInbounds` に lower されるようになった。残りは multi-dimensional axes proof、
mutation を伴う loop のより精密な invalidation、および generic `IndexLoad` / store 側への展開である。

### HOF binary `broadcast(::Vector, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(+/-/*, A, B)` も代表 concrete same-length vectors では typed binary `map` 経路へ入り、
generic `Broadcasted` materialize pipeline を避けるようになった。singleton expansion などの shape fallback は
従来 pipeline に残している。残りは一般 closure の loop devirtualize、nested broadcast の同等特化、
および非代表 callee の自動特殊化である。

### HOF unary `broadcast(::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(identity/abs/abs2/-, A)` も代表 concrete vectors では typed unary `map` 経路へ入り、
generic `Broadcasted` materialize pipeline を避けるようになった。残りは一般 closure の
loop devirtualize、binary/nested broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF predicate `broadcast(::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`broadcast(iszero/isone/signbit, A)` と `broadcast(iseven/isodd, A)` も代表 concrete vectors では
Bool result の direct typed helper 経路へ入り、compile-time metadata も Bool array に揃うようになった。
Bool result の `BitVector` container parity は #5484 で解消済み。
残りは一般 closure の loop devirtualize、binary/nested broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `mapreduce(identity, min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`mapreduce(identity, min, A)` / `mapreduce(identity, max, A)` も concrete numeric vectors では direct typed
helper 経路へ入り、`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。
残りは一般 closure の loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `mapfoldr(identity, min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`mapfoldr(identity, min, A)` / `mapfoldr(identity, max, A)` も concrete numeric vectors では direct typed
helper 経路へ入り、`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。
残りは一般 closure の loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `mapfoldl(identity, min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`mapfoldl(identity, min, A)` / `mapfoldl(identity, max, A)` も concrete numeric vectors では direct typed
helper 経路へ入り、`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。
残りは一般 closure の loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `foldr(min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`foldr(min, A)` / `foldr(max, A)` も concrete numeric vectors では direct typed helper 経路へ入り、
`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。残りは一般 closure の
loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `foldl(min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`foldl(min, A)` / `foldl(max, A)` も concrete numeric vectors では direct typed helper 経路へ入り、
`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。残りは一般 closure の
loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF `reduce(min/max, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`reduce(min, A)` / `reduce(max, A)` も concrete numeric vectors では direct typed helper 経路へ入り、
`Vector{Int32}` などの result metadata を入力要素型に揃えるようになった。残りは一般 closure の
loop devirtualize、broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF Bool-return predicate `filter(::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`filter(iszero, A)` / `filter(isone, A)` / `filter(signbit, A)` も concrete primitive vectors では
direct typed helper 経路へ入り、result eltype を入力 vector と同じ concrete type に保つようになった。
残りは一般 closure の loop devirtualize、reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF parity predicate `filter(::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`filter(iseven, A)` / `filter(isodd, A)` も concrete integer vectors では direct typed helper 経路へ入り、
result eltype を入力 vector と同じ concrete type に保つようになった。残りは一般 closure の
loop devirtualize、reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF unary predicate `map(isodd, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(isodd, A)` も concrete integer vectors では `Vector{Bool}` result の typed helper 経路へ入り、
compile-time metadata も `Vector{Bool}` に揃うようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF unary predicate `map(iseven, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(iseven, A)` も concrete integer vectors では `Vector{Bool}` result の typed helper 経路へ入り、
compile-time metadata も `Vector{Bool}` に揃うようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF unary predicate `map(signbit, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(signbit, A)` も concrete primitive vectors では `Vector{Bool}` result の typed helper 経路へ入り、
compile-time metadata も `Vector{Bool}` に揃うようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF unary predicate `map(isone, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

`map(isone, A)` も concrete primitive vectors では `Vector{Bool}` result の typed helper 経路へ入り、
compile-time metadata も `Vector{Bool}` に揃うようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### `map(iszero, ::Vector{Int32})` の return type 推論を `Vector{Bool}` に修正 (Issue #5469)

runtime では `Vector{Bool}` だった `map(iszero, ::Vector{Int32})` の compile-time metadata も
`Vector{Bool}` に揃った。#5469 は DONE へ移動。

### HOF unary predicate `map(iszero, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

代表的な unary predicate map として、`map(iszero, A)` も concrete primitive vectors では
`Vector{Bool}` result の typed helper 経路へ入るようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。
compile-time return metadata の `Vector{Bool}` 精密化は #5469 で追跡する。

### HOF unary `map(identity, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

代表的な copy-preserving unary map として、`map(identity, A)` も concrete primitive vectors
では typed helper 経路へ入るようになった。残りは一般 closure の loop devirtualize、
filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### HOF unary `map(-, ::Vector)` の narrow integer 特化を追加 (Issue #5094, Epic #5097)

#5462 の修正を前提に、narrow integer unary `map(-, A)` も typed helper 経路へ入るようになった。
残りは一般 closure の loop devirtualize、filter/reduce/broadcast の同等特化、
および非代表 callee の自動特殊化である。

### applicable / hasmethod は実装済み (Issue #5124, DONE へ移動)

`applicable(f, args...)`(実値ベース)と `hasmethod(f, Tuple{...})`(型ベース)の
メソッド存在判定は現 main で完備。`reflection.jl` の
`applicable(f, args...) = hasmethod(f, typeof(args))`(Issue #4957)と既存 `hasmethod`
builtin で、ボディを実行せずに dispatch 可能なメソッドの有無を返す。#5124 の契約
(定義あり→true / 引数型不一致・arity 不一致→false)を両関数まとめて固定する回帰
fixture `reflection::reflection_applicable_hasmethod_5124` を追加し、upstream Julia
1.12.6 と pass 数(18/13/4)一致を確認。詳細は DONE.md / STATUS.md。残課題は
`hasmethod` の effects/exception 値の full classification(#4274 系)と world-age
filtering のみで、本 issue の存在判定とは別スコープ。

## 最新対応 (2026-05-31)

### HOF unary `-` の narrow integer callable dispatch を修正 (Issue #5462)

HOF callable としての unary `-` が narrow integer methods を見つけられない不具合は
#5462 で修正済み。この callable 解決を使った narrow integer
`map(-, A)` typed helper 特化も #5094 側の追加小片で対応済み。

### HOF unary `map` の narrow integer vector 特化を追加 (Issue #5094, Epic #5097)

#5094 Phase B の追加小片として、`abs` / `abs2` の concrete narrow integer
vector `map` も typed helper 経路へ入るようになった。残りは一般 closure の
loop devirtualize、filter/reduce/broadcast の同等特化、および非代表 callee の
自動特殊化である。

### HOF unary `map(abs2, ::Vector)` の typed helper 特化を追加 (Issue #5094, Epic #5097)

#5094 Phase B の追加小片として、`abs2` の concrete vector unary `map` も
typed helper 経路へ入るようになった。残りは一般 closure の loop devirtualize、
filter/reduce/broadcast の同等特化、および非代表 callee の自動特殊化である。

### union-split specialize_block の比較 bytecode emit を追加 (Issue #5077, Epic #5097)

#5077 の追加小片として、`specialize_block()` は既知 `Int64` / `Float64` /
`String` 比較の typed bytecode emit まで対応した。残りは full `Compiler`
context への接続、union-split 結果の branch codegen 消費、merge 後の型伝播、
および特化命令列を実パイプラインで使うための検証ベンチである。

### Struct local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_struct` 専用 map は削除し、
Struct fallback local は `Value::StructRef` を `locals_any` + `VarTypeTag::Struct`
へ集約した。これで `Frame` の名前キー fallback carrier は `locals_any` だけになり、
残りの高速化対象は typed slot 命令の利用範囲拡大と Epic #5097 側の特殊化消費へ移る。

### Tuple local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_tuple` 専用 map は削除し、
Tuple fallback local は `locals_any` + `VarTypeTag::Tuple` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### NamedTuple local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_named_tuple` 専用 map は削除し、
NamedTuple fallback local は `locals_any` + `VarTypeTag::NamedTuple` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Dict local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_dict` 専用 map は削除し、
Dict fallback local は `locals_any` + `VarTypeTag::Dict` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Array local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_array` 専用 map は削除し、
Array fallback local は `locals_any` + `VarTypeTag::Array` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Range local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_range` 専用 map は削除し、
Range fallback local は `locals_any` + `VarTypeTag::Range` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### String local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_str` 専用 map は削除し、
String fallback local は `locals_any` + `VarTypeTag::Str` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Int64 local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_i64` 専用 map は削除し、
Int64 fallback local は `locals_any` + `VarTypeTag::I64` へ集約した。
残りは fallback 不能関数での legacy container map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Float64 local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_f64` 専用 map は削除し、
Float64 fallback local は `locals_any` + `VarTypeTag::F64` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Float32 local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_f32` 専用 map は削除し、
Float32 fallback local は `locals_any` + `VarTypeTag::F32` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Float16 local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_f16` 専用 map は削除し、
Float16 fallback local は `locals_any` + `VarTypeTag::F16` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Char local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_char` 専用 map は削除し、
Char fallback local は `locals_any` + `VarTypeTag::Char` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### Bool local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_bool` 専用 map は削除し、
Bool fallback local は `locals_any` + `VarTypeTag::Bool` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### NarrowInt local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_narrow_int` 専用 map は削除し、
narrow integer fallback local は `locals_any` + `VarTypeTag::NarrowInt` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### ValSymbol local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_val_symbol` 専用 map は削除し、
`ValSymbol` fallback local は `locals_any` + `VarTypeTag::ValSymbol` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去と、最終的な単一 boxed fallback への集約である。

### RNG local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_rng` 専用 map は削除し、
`Rng` fallback local は `locals_any` + `VarTypeTag::Rng` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去、`ValSymbol` など特殊 carrier の整理、
最終的な単一 boxed fallback への集約である。

### Generator local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_generator` 専用 map は削除し、
`Generator` fallback local は `locals_any` + `VarTypeTag::Generator` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去、`ValSymbol` など特殊 carrier の整理、
最終的な単一 boxed fallback への集約である。

### Nothing local carrier を locals_any に集約 (Issue #5081, Epic #5097)

#5081 の追加小片として、legacy `locals_nothing` 専用 `HashSet` は削除し、
`Nothing` fallback local は `locals_any` + `VarTypeTag::Nothing` へ集約した。
残りは fallback 不能関数での legacy map 参照撤去、`ValSymbol` など特殊 carrier の整理、
最終的な単一 boxed fallback への集約である。

### HOF unary map の callee/element-type 特化を追加 (Issue #5094, Epic #5097)

#5094 Phase B の小さな Pure Julia slice として、`abs` / `abs2` / unary `-` の concrete vector
`map` は typed helper 経路へ入るようになった。一般 closure の loop devirtualize、
filter/reduce/broadcast の同等特化、非代表 callee の自動特殊化は引き続き残スコープ。

### branch-narrowed `isa` の codegen 定数化を追加 (Issue #5077, Epic #5097)

branch-local narrowing 済みの `isa(x, T)` / `x isa T` は #5077 の追加対応で
`PushBool(true/false)` に定数化されるようになった。`specialize_block` を実際の
branch codegen に統合する full pipeline は引き続き残スコープ。

### String kwarg override の dynamic return/equality regression を修正 (Issue #5423)

unannotated `nothing` default kwarg から戻った override 値が value position で
`nothing` に潰れたり、`Any` return 経由の `String == String` が false になる
regression は #5423 で修正済み。kwarg default 推論と FunctionInfo metadata を
dynamic に揃え、dynamic-both の String 比較は静的 String 比較と同じ内容比較へ揃えた。

### kwarg nothing default の slot metadata regression を修正 (Issue #5416, Epic #5097)

#5081 の typed slot metadata に起因して unannotated `f(; by=nothing)` が
明示指定値を `LoadSlotNothing` で拒否していた regression は #5416 で修正済み。
`nothing` default kwarg は `Any` slot metadata とし、明示指定値を受ける。

### #5097 特殊化 Epic の残スコープ (#5095 / #5096 / #5087 / #5079 / #5080 / #5091 / #5086 / #5084 / #5085 / #5094 Phase A map/reduce / #5093 / #5081 PR-A 実装後)

#5095 により boxing rate / dispatch miss rate / devirtualization rate /
typed arithmetic rate を `profiling` feature 下のベンチで取得できるようになった。
#5096 により widening threshold tuning 用の metrics と sweep bench も追加した。
#5087 により VM-level method dispatch の型キー付き positive/negative cache も追加した。
#5079 により call-site polymorphic inline cache の lookup/store helper と counter 連携も追加した。
#5080 により mixed concrete primitive arithmetic も typed opcode へ lower するようになった。
#5078 により `emit_call_or_specialize()` 経由の compile-time 解決済み call は
`CallResolved` として bytecode 上も区別され、既存 direct-call fast path で実行されるようになった。
ただし `CallDynamic*` / function-variable 経路の world-age/invalidation 安全な置換はまだ残る。
#5091 により slotized bytecode の load-op/load-op-store superinstruction 融合も追加した。
#5086 により top-level/module `const` binding 由来の numeric const folding と
dispatch-free bool branch elimination も codegen に反映されるようになった。
#5084 により call-site 実引数型から再推論した関数戻り型が代入先の concrete store に
伝播する regression coverage も追加した。
#5094 Phase A により inline anonymous function callee の LetBlock から `map` result eltype を
推論する codegen-local HOF 推論も追加した。
#5094 Phase A の追加範囲として、同じ LetBlock 抽出を `reduce` の binary lambda にも適用し、
inline reduce result type を concrete store へ伝播できるようになった。
#5094 Phase A の追加範囲として、`map` / `filter` / `reduce` の整数 range literal 入力でも
element type を `Int64` として扱い、inline lambda の戻り型推論を range 入力へ伝播できるようになった。
#5094 Phase B の小さな Pure Julia slice として、`map(abs, ::Vector{Int8/Int16/Int32/Int64/UInt8/UInt16/UInt32/UInt64/Float64/Float32/Bool})` /
`map(abs2, ::Vector{Int8/Int16/Int32/Int64/UInt8/UInt16/UInt32/UInt64/Float64/Float32/Bool})` と
`map(-, ::Vector{Int8/Int16/Int32/Int64/UInt8/UInt16/UInt32/UInt64/Float64/Float32})` は typed helper 経路へ入り、
concrete callee + concrete element type の代表的な単項 map で dynamic Generator 経路を避ける。
#5093 PR-A により inference cache key / world range / cached return の serde roundtrip
基盤も追加した。
#5093 PR-B により Base compilation の inference return cache snapshot を `CachedBase` に保持し、
Base cache hit の shared inference engine へ replay する in-memory registry wiring も追加した。
#5093 PR-C によりその snapshot を `SerializedBaseCache` version 3 へ含め、
persistent/embedded Base cache からも replay できるようになった。
#5093 PR-D により persisted inference result の `valid_worlds` をロード先 engine world へ
rebase し、cap 済み entry は復活させないようになった。
#5093 PR-E により `CompiledProgram.specializable_functions` が `SerializedBaseCache.compiled`
経由で persistent/embedded Base cache に含まれることと、skip される `compile_context` が
load 時に復元されることを regression coverage で固定した。
#5085 により concrete struct field read/write が `GetField(index)` / `SetField(index)` に
lower される bytecode coverage も追加した。
#5077 により `specialize_block()` は literal return と既知 `Int64` / `Float64` 算術 return の
最小 bytecode emit を開始した。ただし full compiler context が必要な control flow、
slot-aware local lowering、method dispatch、boxing 除去はまだ未実装。
#5077 の追加範囲として、`x isa T` の else branch が二要素 `Union{T,U}` から単一 `U` に
確定する場合の codegen-local narrowing も追加した。ただし三要素以上の Union、複合条件、
`specialize_block` との統合はまだ残る。
#5081 PR-A により `SlotInfo` / `FunctionInfo` / `CompiledProgram` に slot index 対応の
静的 `VarTypeTag` metadata を保持する基盤も追加した。`StoreAny` や conflicting store は
unknown のまま残し、実行時 frame layout はまだ変更しない。
#5081 PR-B の小さな実装単位として F32/F16/String の typed slot instruction と metadata
driven slotize も追加した。整数・Float64・Bool とあわせて primitive/string slot の
命令分岐は広がったが、Frame の typed Vec 併設、HashMap fallback 撤廃、container/struct 系
slot layout 移行はまだ残る。
#5081 PR-C 前半により primitive/string typed slot sidecar Vec を `Frame` に併設し、
typed `StoreSlot*` / `LoadSlot*` は sidecar と boxed slot を同期するようになった。
その後 `Frame::set_slot_value` helper に generic slot write を集約し、direct slot write でも
sidecar と boxed slot の同期を維持しやすくした。
さらに Array / Tuple / Dict / StructRef の typed slot sidecar と typed slot 命令を追加し、
container/struct local access も slot index 経路へ寄せ始めた。
続いて NamedTuple / Range / RNG の typed slot sidecar と typed slot 命令も追加した。
Set についても既存 sidecar を `LoadSlotSet` / `StoreSlotSet` に接続し、静的 Set local
access を typed slot 命令へ slotize するようになった。
Generator についても `LoadSlotGenerator` / `StoreSlotGenerator` と sidecar を追加し、
静的 Generator slot の load を typed slot 命令へ slotize するようになった。
Char / narrow integer / Nothing についても typed slot sidecar と load/store 命令を追加し、
静的 slot 型が確定した load を typed slot 命令へ slotize するようになった。
Symbol についても `slot_symbol` sidecar と `LoadSlotSymbol` / `StoreSlotSymbol` を追加し、
通常の `Symbol` local を slot index 経路へ寄せた。legacy `StoreAny` と引数 bind でも
通常の `Symbol` は `VarTypeTag::Symbol` で記録し、`ValSymbol` / `Any` carrier から分離している。
Set も legacy `StoreAny` / `StoreSet` / 引数 bind で `VarTypeTag::Set` を使う。
typed `StoreArray` / `StoreDict` fallback でも `Set` タグを維持する。
legacy string-keyed locals map/set は lazy wrapper 化し、空フレーム生成時には backing
`HashMap` / `HashSet` を確保しないようになった。
#5081 の追加小片として、legacy `locals_nothing` 専用 `HashSet` は削除され、
`Nothing` fallback local は `locals_any` + `VarTypeTag::Nothing` へ集約された。
#5081 の追加小片として、legacy `locals_generator` 専用 map は削除され、
`Generator` fallback local も `locals_any` + `VarTypeTag::Generator` へ集約された。
#5081 の追加小片として、legacy `locals_rng` 専用 map は削除され、
`Rng` fallback local も `locals_any` + `VarTypeTag::Rng` へ集約された。
#5081 の追加小片として、legacy `locals_val_symbol` 専用 map は削除され、
`ValSymbol` fallback local も `locals_any` + `VarTypeTag::ValSymbol` へ集約された。
#5081 の追加小片として、legacy `locals_narrow_int` 専用 map は削除され、
narrow integer fallback local も `locals_any` + `VarTypeTag::NarrowInt` へ集約された。
ただし fallback 不能関数での legacy map 参照撤去、単一 boxed fallback への集約、
残る特殊 Value carrier の slot policy 整理はまだ残る。
ただし #5097 の実行高速化 Epic は継続中で、以下の子 Issue は未完了:

- #5077 `specialize_block` から型特殊化 bytecode を実 emit（literal/simple arithmetic /
  二要素 Union の else narrowing 以外）
- #5081 整数スロット割当による frame HashMap 撤廃（slot type metadata の利用と frame layout 移行）
- #5078 単一解決呼び出しの devirtualize（dynamic/function-variable 経路の安全な置換）
- #5089 in-bounds 証明による bounds-check 除去
- #5092 MethodInstance 風 型特化多版数化
- #5094 map/reduce/filter/broadcast など HOF 特殊化（整数 range literal 入力以外の iterator
  element type 推論、実行ループ devirtualize / broadcast 特化）

## 最新対応 (2026-05-30)

### `sizeof` / `aligned_sizeof` / `allocatedinline` のモデル上の保留 (Issue #5107)

`Base.aligned_sizeof` / `Base.allocatedinline` / `Base.datatype_alignment` を実装し、
struct の `sizeof(T)`(mutable のデータレイアウトサイズ修正含む)が本家 1.12 と
pass/fail 一致(STATUS/DONE.md 2026-05-30 参照)。以下が残る:

- **isbits `Union` の inline 判定**: 本家は `Union{Int8,Float64}` のような
  bits-union を inline 格納し `allocatedinline` / `isbitsunion` が `true`、
  `aligned_sizeof` も union のバイト幅+タグを返す(`jl_islayout_inline`)。
  本実装は `Union` を concrete とみなさないため `allocatedinline(Union)==false`、
  `aligned_sizeof` はポインタ幅 8 へフォールバックする。
- **`datatype_haspadding` / `datatype_pointerfree` / `#undef` 余剰バイト**等の
  レイアウトフラグは未実装。`allocatedinline` は「concrete かつ immutable」近似で、
  本家 `jl_stored_inline` の pointerfree / `n_uninitialized` 分岐は反映しない。
- **単一英大文字名の struct(`P`/`T`/`N`)**: TypeVar 誤分類(Issue #5252, bug)で
  `isconcretetype`/`isbitstype` が誤るため `allocatedinline` も誤る。回避策は
  2 文字以上の型名。`sizeof`/`fieldoffset`/`aligned_sizeof` のレイアウト式自体は本家一致。

### `fieldoffset` / struct レイアウトのモデル上の保留 (Issue #5100)

ネスト struct のアラインメントをフィールドアラインの最大値で算出するよう修正し、
isbits/immutable struct の `fieldoffset(T, i)` / `sizeof(T)` が本家 1.12 と一致
(STATUS/DONE.md 2026-05-30 参照)。本 Issue のレイアウトモデルでは以下が残る:

- **単一英大文字名の struct(`P`/`T`/`N` など)**: TypeVar と誤分類される
  バグ(Issue #5252, bug)は**解決済み**。宣言済み型名は名前長に関係なく
  `DataType` に解決されるようになり、これらの名前でも
  `isbitstype` / `sizeof` / `fieldoffset` 等のレイアウト判定が本家 1.12 と一致
  (STATUS/DONE.md 2026-05-30 参照)。
- **ボックス型 / 非 isbits フィールドを含む struct のフィールド単位 inline 可否**:
  本家の `Base.allocatedinline` / `aligned_sizeof`(`jl_datatype_size`、
  union-of-bits の inline 格納、`#undef` フラグ用の余剰バイトなど)は未実装。
  非 isbits フィールドは一律ポインタ幅(8 バイト, align 8)として扱う近似で、
  本家が inline 格納する一部ケースのバイトレイアウトとは一致しない(Issue #5107)。
- **単一英大文字名の struct(`P`/`T`/`N` など)**: TypeVar と誤分類されるため
  (Issue #5252, bug)、`isbitstype` / `sizeof` / `fieldoffset` のレイアウト判定
  対象外。回避策は 2 文字以上の型名を使う。レイアウト計算式自体は本家一致。
- **`Base.allocatedinline` / `aligned_sizeof`**: Issue #5107 で実装済み(上記
  「`sizeof` / `aligned_sizeof` / `allocatedinline` のモデル上の保留」を参照)。
  immutable struct のボックス型フィールド(`String` 等)は本家どおり inline 格納で
  扱う。union-of-bits の inline 判定など一部は引き続き保留(#5107 参照)。
- **packed / `@align` などの非標準レイアウト**: 未対応。

### `nameof(::Type)` 修正後に残るスコープ (Issue #5106, bounded slice)

`nameof(::Type)` を正準 TypeName シンボル解決へ刷新し、`Base.typename(T)` を
導入した(`nameof(Vector{Int}) === :Array`、`typename(Foo{Int}) === typename(Foo)`)。
本 Issue の範囲外として以下が残る:

- **`Core.TypeName` オブジェクトの完全な同一性共有モデル**: SubsetJuliaVM は
  TypeName を正準基底名**シンボル**で表現する。`getfield(T, :name)` は本家の
  `Core.TypeName` を模した射影を返し、`.name`(シンボル)と `.wrapper`
  (正準ジェネリック UnionAll ラッパー、`Foo{Int}.name.wrapper === Foo`、
  Issue #10558)を公開する。`.module` など残りのフィールドや完全な同一性共有
  モデルは未対応。観測可能な等価性(`typename(Foo{Int}) === typename(Foo)`、
  `nameof` 一致)は成立する。
- **`nameof(::Module)`**: Issue #11171 で実装済み。`nameof(::Function)` /
  値・関数形は Issue #5580 の regression fix 後も成立する。
### subtypes の内部 Base 型はレジストリ範囲外で繰り延べ (Issue #5057)

`subtypes(T)` をソート・重複排除・パラメトリック基底名対応で本家
`InteractiveUtils.subtypes` に一致させた(STATUS/DONE.md 2026-05-30 参照)。
ただし VM がモデル化していない**内部 Base 型**は列挙できない。これは
`subtypes` が本質的に型レジストリのスコープに依存するためで、crash せず
妥当な(ソート済み)配列を返す。

- `subtypes(AbstractFloat)`:本家は `Core.BFloat16` を含むが sjulia は含まない
  (`[BigFloat, Float16, Float32, Float64]`)。
- `subtypes(Number)`:本家は `Base.MultiplicativeInverses.MultiplicativeInverse`
  を含むが sjulia は含まない(`[Complex, Real]`)。

フィクスチャ `reflection/supertype_subtypes_user_types_5057.jl` は、レジストリで
利用可能な安定ケース(`subtypes(Integer)`/`subtypes(Signed)`/ユーザー階層など)
のみをアサートし、上記の内部型差分は対象外としている。

### ユーザー定義パラメトリック型エイリアス実装後に残るスコープ (Issue #5055, bounded slice)

`MyVec{T} = Vector{T}` のパラメトリック型エイリアスを実装し、`MyVec{Int} ===
Vector{Int}` / 構築 / `isa` / `<:` / `::MyVec{Int}` ディスパッチを成立させた
(lowering 時に対象型文字列へ展開)。本 Issue の範囲外として以下が残る:

- **`MyVec === (Vector{T} where T)`**: `where` 式を値位置で評価する機能が未対応
  (`UnsupportedExpression("where_expression")`)。これはエイリアスに依存しない
  既存の制限で、bare エイリアスの UnionAll 値同一性比較はこの機能を前提とする。
- **ユーザー定義パラメトリック構造体の具体型引数ディスパッチ**: `f(::Box{Int})`
  への `Box{Int}` 値ディスパッチがエイリアス非依存で未成立(`NoMethodFound`)の
  ため、エイリアス経由 `f(::BoxAlias{Int})` も同様に未成立。builtin の
  `Vector{Int}` / `Dict{K,V}` 引数ディスパッチは成立する。
- **REPL の行をまたいだエイリアス永続化**: 各 REPL 入力が独立に lowering される
  ため、ある行で定義したエイリアスは次行に持ち越されない(既存の非パラメトリック
  エイリアスと同じ制限)。スクリプト/ファイル単位の一括 lowering では全エイリアス
  が事前登録されるため成立する。
### 配列 show 接頭辞修正後に残る eltype 推論 divergence (Issue #5236 / #5237)

Issue #5236 / #5237 で配列 show の eltype プレフィックス(`typeinfo_prefix` /
`typeinfo_implicit`)を実装し、配列の**実 eltype を所与として**本家と一致させた。
プレフィックスは表示側で正しく導出されるが、以下は **sjulia の型推論が本家より
広い eltype を推論する**ことに起因する表示差分で、いずれも crash せず妥当な配列
形を出力する。表示ロジックではなく推論側の課題であり、フィクスチャでは sjulia の
eltype が本家と一致するケースのみアサートしている。

- **`Complex` 整数リテラル**: `[complex(1,1), complex(2,2)]` を sjulia は
  `Complex{Float64}` に promote する(本家は `Complex{Int64}` を保持)。
  プレフィックスは eltype どおり `Complex{Float64}[...]` を出すため、本家の
  `Complex{Int64}[...]` と表記が異なる。`ComplexF64[...]` を明示構築した場合の
  eltype は逆に `Any` に widen する(下記参照)。
- **`Pair` リテラル配列**: `[1 => 2]` を sjulia は eltype `Any` に widen する
  (本家は `Pair{Int64, Int64}`)。表示は値駆動導出で同種の暗黙 `Pair` 列を
  接頭辞なし(`[1 => 2]`)に落として本家一致を保つが、これは「実 eltype が
  `Any` なら本家は `Any[...]` を出す」という原則からの**意図的な逸脱**で、
  はるかに頻出するリテラル形を優先したもの。そのため逆に `Any[1 => 2, 3 => 4]`
  (本家 eltype が真に `Any`)は sjulia では `[1 => 2, 3 => 4]` と表示され、
  本家 `Any[1 => 2, 3 => 4]` と異なる。同種・暗黙要素の真正 `Any` 配列
  (`Any[1, 2]` / `Any[1 2; 3 4]`)も同様にプレフィックスを落とす。
- **`Tuple` / `BigInt` / `Rational` リテラル配列**: `[(1,2),(3,4)]` /
  `[big(1)]` / `[1//2]` などは sjulia の eltype が `Any`(本家は
  `Tuple{Int64,Int64}` / `BigInt` / `Rational{Int64}`)。`Tuple` は値駆動で
  暗黙判定し接頭辞なし(本家一致)、`BigInt` / `Rational` は非暗黙のため真正
  `Any` 配列として `BigInt`/`Rational` ではなく要素同種型のプレフィックスを
  導出するが、これらも eltype が本家と一致しないケースとして非アサート。

これらは表示側ではなく eltype 推論の wide 化(`Vector{Any}` への collapse、
`Complex` の Float 化)に依存しており、推論修正側のクラスタ(#4646 系の
narrow eltype 保持等)で解消されるべき範囲。

### `Base.unwrap_unionall` / `Base.rewrap_unionall` で残るスコープ (Issue #5105, bounded slice)

本スライスで `unwrap_unionall` / `rewrap_unionall` の往復同一性
(`rewrap_unionall(unwrap_unionall(X), X) === X`)を、具体型 no-op と
`Vector`/`Set`/`Dict` のエイリアスについて成立させた(STATUS/DONE.md 参照)。
`Array` の `(T, N)` 二重ネスト `UnionAll` と `Vector`/`Matrix` の
`Array{T,1/2}` body は #5593 で解決済み。`Set`/`Dict` と同様に
unwrap→rewrap identity も成立する。

### 型述語(isconcretetype 等)を bare `Ref` に適用したときの残スコープ (Issue #5102 → #5223 で解決)

型述語 5 種(isconcretetype/isabstracttype/isstructtype/ismutabletype/
isprimitivetype)を本家完全一致にした(STATUS/DONE.md 参照)が、bare `Ref`
への型述語は #5102 範囲外として切り出していた。**Issue #5223 で解決済み**:
bare `Ref`/`RefValue`/`Base.RefValue` を UnionAll 型オブジェクトとして表現し、
`Ref` ファミリ(`Ref` / `Ref{T}`)を abstract に分類した(`typeof(Ref)===UnionAll`,
`isconcretetype(Ref)==false`, `isabstracttype(Ref)==true`,
`isconcretetype(Ref{Int})==false`)。詳細は DONE.md / STATUS.md の Issue #5223。

### `Ref{T}` / `Base.RefValue{T}` 実装で残るスコープ (Issue #5130, bounded slice)

本スライスで可変単一値ボックスとしての `Ref`/`Base.RefValue` を実装した
(構築・`r[]`/`r[]=v`・`getindex`/`setindex!`・`typeof`・`r.x`/`fieldnames`・
`isa`/dispatch/`<:`・`repr`/`show`。STATUS/DONE.md 参照)。以下は本スライスでは
**未対応**として切り出した:

- **連鎖インデックス左辺 `v[1][] = x`**: 配列等の要素を介して Ref を直接変更する
  代入は `UnsupportedAssignmentTarget`(lowering)で失敗する。これは Ref 固有では
  なく、`m[1][1] = x` のような **ネストした index 左辺**一般の既存制限。回避策は
  一時束縛(`e = v[1]; e[] = x`。`e` は同じセルを共有するため変更は反映される)。
  本家相当の連鎖 lvalue lowering は別 Issue 範囲。
- **ポインタ / C interop の Ref 意味論**: `Base.unsafe_convert(Ptr, ::RefValue)`,
  `pointer_from_objref`, `Ref{T}()` の本家 `#undef` 型付き挙動など。no-JIT iOS
  VM では対象外(`Ref{T}()` は近似として `#undef` を包んだ Ref を返す)。
- **`Ref(array, i)` / `Ref(x, ())` 等の多引数 Ref 構築**(refpointer.jl 由来):
  ブロードキャスト保護用の単一引数 `Ref(x)` と本 Issue の `Ref{T}(x)` のみ対応。
### `Base.Fix1` / `Base.Fix2` 実装後に残るスコープ (Issue #5127)

`Base.Fix1` / `Base.Fix2` 部分適用型と bound callable struct
`(self::Type)(args)` を実装済み (DONE.md 参照)。残る非対応:

- 一般形 `Base.Fix{N,F,T}` と整数型パラメータ dispatch
  (`Fix{1}(f, x)` のような partial inner constructor)。VM が struct の整数型
  パラメータでの partial 適用 constructor / dispatch を未対応のため、本家の
  `const Fix1 = Fix{1,F,T}` / `const Fix2 = Fix{2,F,T}` という別名関係ではなく、
  `Fix1` / `Fix2` を独立した concrete struct として定義している。
- curried comparison operator (`==(x)`, `>(x)` 等) は既存 lowering 経路
  (Issue #3119) が anonymous closure を返す挙動を維持しており、本家のように
  `Base.Fix2{typeof(==), T}` インスタンスを返さない。lowering を `Fix2` 生成へ
  切り替えるのは別課題。
- `isequal(x)` / `in(x)` を HOF の引数位置にインラインで書くと、これらが
  builtin であるため戻り値が `Bool` と誤推論される。変数束縛経由
  (`g = Base.Fix2(in, c); map(g, v)`) では正しく動作する。
- struct インスタンスの `g.f === (==)` のような関数オブジェクト identity は
  `===` では false (`==` は true)。VM の関数値 identity の課題。
- (更新, Issue #5126) bound callable struct の **フルフォーム**
  `function (self::Type)(args) ... end` とパラメトリック functor
  `function (self::Type{T})(args) where T ... end` も対応済み(従来はショートフォームのみ)。

## 最新対応 (2026-05-29)

### const 特殊化 / 推論キャッシュキーの統一で残るスコープ (Issue #4272, bounded slice)

本スライスで const の **preserve/widen 判定**を compile / AoT 両パスで単一述語
(`cache_key.rs` の `const_specialization` → `is_const_eligible`)に統一し、
AoT の `CodeInstanceArgKey::from_const_value` がこの述語経由で判定するように
した(STATUS/DONE.md 参照)。同一 `ConstValue` に対し両パスが必ず同じ const
特殊化判定を下すことをクロスパス単体テストで保証。以下は本スライスでは
**意図的に未対応**として切り出した:

- **キャッシュキー *型* そのものの統合**: AoT は依然 `CodeInstanceKey`
  (ABI レイアウト tuple + `arg_key`)を独自に持ち、compile の
  `InferenceCacheKey` とは構造的に別型。共有しているのは判定 *ポリシー* のみで、
  キー型の完全統合(`#3510` の TODO)は follow-up。
- **const 値の AoT body 推論本体への伝播**: `arg_key` は specialization の
  *識別*には使われるが、保たれた const 値を AoT の本体型推論まで運んで
  branch 除去 / `Val`-like 精度を出すところは未対応。
- **broadcast / generated 呼び出しサイト収集**: 直接ユーザ関数呼び出しと
  broadcast 経路は同一 arg-key 経路を通るが、generated 呼び出し収集など他経路
  への同ポリシー適用範囲拡張は未完。
- **large/non-profitable const widening の CI audit**: キャッシュ爆発を防ぐ
  widening 不変条件を CI スクリプト化する作業は未着手。
### `tuple_type_head/tail/cons` で残るエッジ (Issue #5119, deferred slice)

Issue #5119 で `tuple_type_head/tail/cons` の具象 Tuple 型に対する分解・構築を
実装した(STATUS/DONE.md 参照)。以下は本スライスでは**意図的に未対応**:

- **Vararg を含む Tuple**: 本家 `tuple_type_tail(Tuple{Int, Vararg{String}})` は
  `Vararg` を畳んだ Tuple 型(`Tuple{Vararg{String}}` 等、Issue #5061/#5062 の
  挙動)を返すが、本スライスは `T.parameters` をそのまま `_make_tuple_type` で
  並べ替えるだけなので Vararg の N デクリメント等は未対応。具象(非 Vararg)
  Tuple の head/tail/cons のみを対象とする。
- **空 Tuple の head 例外型**: `tuple_type_head(Tuple{})` は本家では
  `BoundsError` を投げる。sjulia でも例外を投げるが、`Core.SimpleVector` の
  範囲外 index が `String` 例外になる既存の VM 制約のため例外型は一致しない
  (head の正常系は本家と一致)。

### パラメトリック構造体インスタンス化 reflection で残るギャップ (Issue #3909 → 焦点 follow-up #5593)

**#3909 は受け入れ基準を満たし close 済み**: 受け入れコマンド
`cargo nextest run --release --test fixture_tests reflection:: types_tests:: dispatch::`
は main で green、`reflection/runtime_type_object_acceptance_3909.jl` が fresh
TypeVar / UnionAll wrap-unwrap / parametric params / layout metadata / identity
比較を本家とフィールド単位で固定。`Vector`/`Matrix` を `Array{T,N}` の
次元エイリアスとしてモデル化する focused follow-up #5593 も解決済み:

Issue #3909 でパラメトリック構造体**インスタンス化** (`Box{Int64}` 等) の
`fieldnames` / `fieldcount` / `fieldtypes` / `supertype` を本家と一致させた
(STATUS/DONE.md 参照)。以下は本スライスでは**意図的に未対応**として切り出した
（#5593 で解決済み）:

- ~~**組込みパラメトリック型の dense 階層**: `supertype(Vector{Int})` は本家では
  `DenseVector{Int64}` (= `Array{Int64,1}` の直接親) だが、sjulia は `Array` を
  返す。`DenseVector`/`DenseArray`/`DenseMatrix` の組込み型階層自体が未モデル化。~~
  **2026-06-02 / Issue #3909 slice で解決済み**:
  `DenseArray` / `DenseVector` / `DenseMatrix` は first-class type value として解決し、
  `Vector{Int}` / `Matrix{Int}` / `DenseVector{Int}` / `DenseMatrix{Int}` の direct
  `supertype` と invariant subtype surface は `reflection_dense_array_supertype_3909`
  で本家一致。
- **`Vector{Int}.parameters` の次元値パラメータ `N`** は Issue #4722 の既存
  ギャップ (上記参照) と同根で、本スライスの対象外。
- **パラメトリック親への subtype 関係**: `Sub{Int} <: AbsB{Int}` (および
  `Sub{Int} <: AbsB`) は本家では true だが sjulia では false。これは
  `supertype` の表現ではなく subtype 判定 (`CoreType` の UnionAll 環境付き
  subtype) の問題で、別スライス。
- ~~**bare UnionAll の `fieldtypes`**: `fieldtypes(Box)` は本家では型変数を上界に
  widening して `(Any,)` を返すが、sjulia は空。~~ **Issue #5099 で解決済み**:
  `field_types` が `parametric_schema` にフォールバックして型変数を上限境界
  (無境界は `Any`) に置換 (`type_objects.rs` の `bare_parametric_field_types`)。
  `fieldtypes(Box)===(Any,)` / `fieldtype(Box,1)===Any` が本家一致。

### 公開ヘルパ effect/exception 分類でクラスタ外に残るシグネチャ (#4968/#4969/#4971/#4974)

`#5141` の builtin カテゴリ層の上に、#4968/#4969/#4970/#4971/#4974 の**代表
シグネチャ**を本家 1.12.6 とフィールド単位で一致させた (STATUS/DONE.md 参照、
`reflection.jl` の `_classify_helper_effects` / `_classify_helper_exception_type`)。
分類は名前 + 引数型でガードしており、以下は本クラスタの scope 外として
**意図的に proven-total フォールバックのまま**残す:

- **同名の Vector/String オーバーロード**: `first`/`last`/`length`/`getindex`/
  `collect`/`eachindex` の Vector 版、`first`/`last`/`eachindex` の String 版、
  `repeat`/`count`/`findfirst`/`replace` の非文字列版は本家では別 record
  (例: `getindex(::Vector,::Int)` は `(!c,+e,!n,+t,+s,?m,?u,+o,+r)` / `BoundsError`、
  `first(::Vector)` も同様、`length(::Vector)` は `(?c,+e,+n,...)`)。これらは
  `?m`/`?u` の refine と引数の可変性に依存する記録で、本タスクの representative
  scope 外。引数型ガードにより誤分類せず fall-through する (regression なし)。
  完全化は #4274 の interprocedural / argmem refine 拡張に依存。
- **基盤側の残スコープ (#4274)**: ユーザーメソッドの body-based effect 推論、
  intrinsic の `nothrow`/`exct` 精緻化、CallMeta エッジ伝播は未実装で、これら
  公開ヘルパの値も本質的には #4274 の完全な推論エンジン無しに「名前 + 引数型 →
  upstream 既知値」の表で再現している (本家と同一の `EFFECTS_UNKNOWN` /
  precise record を返すが、実体は推論ではなく分類)。新規ヘルパ追加時は本家を
  probe して同方式で表に追加する。

### ~~`zero`/`one`/`oneunit` の interprocedural forwarding widening (Issue #5167 part 2)~~ → 解決済み

Issue #5167 の part 1 (zero/one/oneunit が Float32/Float16 を型保存しない) は
`base/number.jl` に `Float32`/`Float16` メソッドを追加して解決済み (PR #5195)。
**part 2 (interprocedural forwarding widening) も解決済み**:
`g(y)=zero(y); f(x::Real)=g(x); f(3)` が本家どおり `0::Int64` を返すようになった
(STATUS/DONE.md 2026-05-30 参照)。`infer_expr_type` の `Expr::Var` 分岐で
`abstract_numeric_params` を `ValueType::Any` として報告し、呼び出しサイトの投機的
戻り型再推論をスキップさせる single early-return 修正。fixture
`dispatch/interproc_forward_abstract_numeric_type_generic_5167.jl` で本家 1.12.6 と
parity 確認済み。

派生バグ(解決済み): ユーザ定義 `f(x::Real)=x`(抽象数値パラメータを**直接
返す**形)が実行時型エラー/拡大していた問題は **Issue #5242 で解決**
(STATUS/DONE.md 2026-05-30 参照)。`compile_expr` の `Expr::Var` 分岐で
`abstract_numeric_params` を `ValueType::Any` として報告し、直接 return が
`ReturnAny` で具体型を保存する(`infer_julia_type`/`infer_expr_type` の同名
ガードと対称)。fixture
`dispatch/direct_param_return_abstract_numeric_5242.jl` で本家 1.12 と parity
確認済み。

### `Bool` 配列 show 修正後に残る eltype プレフィックスギャップ (Issue #5159)

Issue #5159 で `Bool` ベクタ・行列の `repr` / `print` / `string` を本家と一致
させた (`Bool[1, 0]` 等。STATUS/DONE.md 参照)。本家の `typeinfo_implicit` は
`Float64` / `Int` / `Char` / `String` / `Symbol` / singleton 以外をすべて
「非 implicit」と分類しプレフィックスを出すが、#5159 では **`Bool` のみ** を
対象とした。以下は本家と差分が残る既知の範囲外項目で、いずれも crash せず妥当な
配列形を出力する:

- ~~`Int32` / `Float32` / `UInt8` / `Rational{Int64}` など他の非 implicit eltype
  はプレフィックス無しで出力する~~ **Issue #5236 / #5237 で大部分を解決**:
  `Int8`/`Int16`/`Int32`/`Int128`/`Float32`/`Float16`/`UInt*`/`Complex`/ユーザー
  struct/`Any` 等に `T[...]` プレフィックスを付与し、`Float32`/`Float16` 要素の
  装飾(`1.0f0` / `Float16(1.5)`)も接頭辞コンテキストで本家どおり落とす
  (`Float32[1.0, 2.0]`)。残る差分は **`UInt*` 要素の 16 進ゼロ詰め**
  (sjulia `UInt8[0x1, 0x2]` vs 本家 `UInt8[0x01, 0x02]`)で、これは
  `show(io, ::UInt8)` の pad 桁のずれであり eltype プレフィックスとは別の
  要素レンダリング差分として残る。`Rational{Int64}` などは eltype 推論側で
  `Any` に widen するため下記「推論側 eltype divergence」を参照。
- `Matrix{Bool}(undef, r, c)` パラメトリック構築子が未対応 (`Unknown
  parametric struct: Matrix`) のため、空 Bool 行列の `repr`
  (`Matrix{Bool}(undef, 0, 0)`) は sjulia では exercise できない。パラメトリック
  構築子の別ギャップ。
- `:([true, false])` の Expr-AST show は本家 `[true, false]` に対し sjulia は
  `Expr(:vect, true, false)` を出す (AST show の既存差分。#5159 の対象外で、
  本修正でも不変)。
### svec 化後も残る `.parameters` の値パラメータ欠落 (Issue #4722, deferred slice)

Issue #4722 で `<DataType>.parameters` を `Core.SimpleVector` (svec) として返す
よう実装した (display / typeof / isa / length / getindex / iteration /
構造的 `===` / `==` / splat / `Core.svec` コンストラクタ — STATUS/DONE.md 参照)。
以下は本 Issue では**意図的に未対応**として切り出した:

- ~~**次元値パラメータ `N` の欠落**: `Vector{Int64}.parameters` は本家では
  `svec(Int64, 1)` だが、sjulia は `svec(Int64)` を返す。~~ → **Issue #5162 で解決**。
  `RuntimeTypeObject.parameters_with_values()` (`vm/type_objects.rs`) が整数/値
  パラメータも射影するようになり、`Vector{Int}.parameters == svec(Int64, 1)`、
  `Matrix{Float64}.parameters == svec(Float64, 2)`、`Array{T,N}.parameters ==
  svec(T, N)`、`Val{5}/Val{:foo}/Val{true}.parameters` がすべて本家と一致する
  (STATUS/DONE.md 参照)。値要素は `DataType` ではなく具体値 (`Int64`/`Bool`/
  `Symbol`/`String`) として surface し、`===` / `typeof` / 表示 / splat も parity。
  なお `collect` を heterogeneous な svec に適用すると `Vector{Any}` に
  fallback せず numeric 要素型を誤推論する別の既存バグが残る (Issue #5196)。
- `Core.SimpleVector` は組込みの svec 値のためにのみ surface しており、
  ユーザー定義パラメトリック型での完全な svec フィールド配線や
  `Base.rewrap_unionall` 等の svec 依存 Compiler ヘルパは対象外。

### container show スイープで確認した本家との型表現ギャップ (Issue #4739)

Issue #4739 の `show(io, ::T)` 網羅スイープで、空タプル mis-dispatch は修正済み
(STATUS/DONE.md 参照)。残りの本家との差分は show の mis-dispatch ではなく
**型表現ギャップ**で、いずれも crash せず妥当な形を出力するため本 Issue の対象外
として明示的に除外した:

- `keys(::Dict)` / `values(::Dict)` が本家の view 型
  (`Base.KeySet` / `Base.ValueIterator`) でなくタプルを返す。タプルとしての
  show は正しい (例: `(1,)`) が、本家の `[1]` とは異なる。
- `range(start, stop; length=n)` が本家の `StepRangeLen` でなく `LinRange` を返す
  (例: `LinRange{Float64}(0.0, 1.0, 5)` vs `0.0:0.25:1.0`)。範囲計算の差分。
- `LinearIndices` / `CartesianIndex` が非パラメトリックなため、配列接頭辞
  (`CartesianIndex{2}[...]`) や `typeof` の完全パラメトリック表記
  (`LinearIndices{2, Tuple{Base.OneTo{Int64}, ...}}`) を出さない。
  既存の #4733 (typed empty container prefix) と同系統。
### typed-vector / typed-comprehension intercept のクラスタ外 divergence (Issue #4824 関連)

#4824 の再発防止 parity プローブ作成中に発見した、解決済みクラスタ
(#4811/#4816/#4818/#4819/#4822 — 数値型 + Any のみ) の **範囲外** で残存する
divergence。プローブ fixture
(`array/typed_vector_comprehension_cluster_parity_4824.jl`) からは scope 外として
明示的に除外し、それぞれ独立した bug Issue として追跡中:

- **#5040** (2026-05-29 解決済み): `T[expr for x in iter]` 型内包表記が
  `T ∈ {Bool, Char, Symbol, String}` で本家と乖離していた。`Bool[...]` /
  `Symbol[...]` は "Unknown function" エラー、`Char[...]` は I64 slot 拒否、
  `String[...]` は `Vector{Any}` の誤 typeof(本家は `Vector{String}`)。
  対応: lowering でこれらの型は本体を `convert(T, expr)` に書き換え、全体を
  `Vector{T}(...)` で包む。`compile_array_constructor` が comprehension 引数を
  検出し `compile_comprehension_with_elem` /
  `compile_multi_comprehension_with_elem` に強制 eltype を渡して結果
  `Vector{T}` / `Matrix{T}` の eltype を `T` に固定。fixture
  `array/typed_comprehension_nonnumeric_eltypes_5040.jl` 追加(本家 parity 一致)。
  なお `repr(::Vector{Bool})` の `show` 表記差(`[true,...]` vs `Bool[1,...]`)は
  本件とは無関係の既存 show フォーマット差であり別系統。
- **#5041**: `Vector{T}(::Tuple)` が本家では `MethodError` (該当メソッド無し) なのに
  sjulia は `compile_array_constructor` intercept で silent に `Vector{T}` を構築する。

## 最新対応 (2026-05-27)

### Value::Array enum variant retirement local fix (Issue #4568)

2026-05-27: `Value::Array(ArrayRef)` enum variant は Issue #4568 として撤去済み。
runtime/tests の literal `Value::Array` は 0 件になり、
`scripts/check_value_array_allowlist.sh` は zero-match audit へ変更済み。
旧 Phase 4 helper bodies は `native_array_value_ref`,
`native_array_value_mut_ref`, `native_array_ref_value`,
`native_array_value_from_array`, `native_array_ref_from_value` という明示的な
compatibility converter 名へ移した。残る `Value::NativeArray` converter call
sites は、今後の wrapper/Memory primitive 移行でさらに縮小する compatibility
surface である。

### Rust fallback dispatch audit closure (Issue #4276)

2026-05-27: public-name Rust fallback の棚卸し、classification、CI audit、
representative dispatch-first fixture coverage は Issue #4276 として完了済み。
`BASE_FUNCTION_ROUTES` / `PUBLIC_FALLBACKS.md`, `CollectFallback:` /
`COLLECTIONS.md`, and `BinaryBothFallback:` / `BINARY_DISPATCH.md` が
それぞれ同期監査される。残りは broad fallback audit ではなく、Issue #4568 の
final VM-native carrier cleanup / `Value::Array` variant removal である。

### number eltype dynamic dispatch local fix (Issue #4665)

2026-05-27: `eltype(x)` を generic `x` の中から scalar number に対して呼ぶと
upstream Julia では `typeof(x)` 相当の element type を返すのに、sjulia では
`Eltype` builtin fallback が `Any` を返す bug は Issue #4665 として報告済みで、
native scalar number carrier の fallback が `runtime_type()` を返すように
ローカル修正済み。残りは #4018 の broader collect materialization fast paths と
#4568 の final fallback cleanup である。

### scalar narrow-number flatten local fix (Issue #4666)

2026-05-27: `collect(Base.Iterators.flatten((Int8(1), Int16(2))))` が upstream
Julia では `Vector{Signed}` になるのに、sjulia では `iterate: unsupported
collection type I8(1)` で error になる bug は Issue #4666 として報告済みで、
VM scalar `Number` iteration が narrow signed/unsigned integer と narrow float
carrier を扱うようにローカル修正済み。残りは #4018 の broader collect
materialization fast paths と #4568 の VM-native collect boundary cleanup
である。

### scalar flatten collect local fix (Issue #4664)

2026-05-27: `collect(Base.Iterators.flatten((1, 2)))` が upstream Julia では
`Vector{Int64}` になるのに sjulia では `Vector{Any}` に widening する bug は
Issue #4664 として報告済みで、`Flatten` runtime eltype derivation が scalar
`Number` inner value から `typeof(inner)` を使うようにローカル修正済み。修正中に
見つけた broader `eltype(x::Number)` dynamic dispatch fallback bug は Issue #4665
として報告済みで、ローカル修正済み。

### flatten mixed eltype collect local fix (Issue #4663)

2026-05-27: `collect(Base.Iterators.flatten((Int8[...], Int16[...])))` が
upstream Julia では `Vector{Signed}` になるのに sjulia では `Vector{Any}` に
widening する bug は Issue #4663 として報告済みで、`Flatten` の runtime
eltype を inner iterable の `eltype` join から計算し、`IteratorEltype(f)` を
`HasEltype()` にする形でローカル修正済み。追加 probe で見つけた
`collect(Base.Iterators.flatten((1, 2)))` の scalar flatten widening は
Issue #4664 として報告済みで、未対応のまま残す。残りは #4018 の broader
collect materialization fast paths と #4568 の VM-native collect boundary
cleanup である。

### tuple collect abstract numeric eltype local fix (Issue #4662)

2026-05-27: `collect((Int8(1), Int16(2)))` が upstream Julia では
`Vector{Signed}` になるのに sjulia では `Vector{Real}` に widening する bug
は Issue #4662 として報告済みで、`_array_undef_from_dims` の abstract numeric
typed allocation route を `Number` / `Real` / `Integer` / `Signed` /
`Unsigned` / `AbstractFloat` ごとに明示してローカル修正済み。残りは #4018 の
broader collect materialization fast paths と、#4568 の最終 `Value::Array`
variant removal である。

### indexin Union result type local fix (Issue #4657)

2026-05-27: `indexin([1, 3], [1, 2])` が upstream Julia では
`Vector{Union{Nothing, Int64}}` になるのに sjulia では `Vector{Any}` に
widening する bug は Issue #4657 として報告済みで、`indexin` result
allocation を `Vector{Union{Nothing, Int64}}(undef, length(a))` に寄せる形で
ローカル修正済み。あわせて `Memory{Union{Nothing, Int64}}` /
`similar(Array{Union{Nothing, Int64}}, dims)` の Union element tag preservation、
function-value dispatch の non-exact `Type{Any}` demotion、Array `setindex!` が
必要とする `convert(::Type{Union{Nothing, Int64}}, ...)` を追加した。残りは
arbitrary `Union` への general `convert` semantics と、より広い Union element
allocation coverage である。

### typed Set construction and Set/Vector set algebra local fix (Issue #4018)

2026-05-27: `Set(Int8[...])` / `union(Set(Int8[...]), Int8[...])` が
upstream Julia では `Set{Int8}` になるのに sjulia では `Set{Any}` /
`Vector{Any}` に widening し、Pure Julia `Set` algebra result が空のままに
なる bug は Issue #4658 として報告済みで、`Set` runtime type projection と
Pure Julia set operation result update をローカル修正済み。空の
`Set{T}()` が型パラメータを保持せず `Set{Any}` になる bug は Issue #4661
として報告し、`SetValue` element type carrier / `NewSetTyped` /
`collect(::Set)` typed materialization / `empty!` carrier preservation により
ローカル修正済み。さらに direct `push!(s, x)` / `delete!(s, x)` /
`empty!(s)` も local `Set` binding を更新する。残りは最終的な native
`Value::Set` public boundary の整理である。
typed Set metadata で露見した
`Set{T} <: Set` / `empty!(s::Set)` dispatch bug は Issue #4660 として報告し、
bare `Set` type name を復元する形でローカル修正済み。fixture
`sets_parametric_set_subtype_4660` で regression を固定済み。

### empty integer division binary map local fix (Issue #4019)

2026-05-27: `map(/, Int8[], Int8[])` が upstream Julia では
`Vector{Float64}` になるのに sjulia では empty Array path が `similar(A, 0)` に
落ちて `Vector{Int8}` を返す bug は Issue #4659 として報告済みで、integer /
unsigned integer / Bool same-eltype `/` binary map allocation を
`Vector{Float64}` へ移す形でローカル修正済み。残りの arbitrary callable empty
result inference は broader HOF result type inference work として残る。

### permutedims no-perm typed allocation local fix (Issue #4018)

2026-05-27: `permutedims(Int8[...])` / `permutedims(Matrix{Int8})` and
`permutedims(Float32[...])` / `permutedims(Matrix{Float32})` が upstream Julia
では `Matrix{Int8}` / `Matrix{Float32}` になるのに sjulia では `Matrix{Any}` に
widening する bug は Issue #4656 として報告済みで、no-permutation
`permutedims` result allocation を `similar(arr, ...)` へ移す形で
ローカル修正済み。

### hcat/vcat mixed eltype promotion local fix (Issue #4018)

2026-05-27: `hcat(Int8[...], Int16[...])` /
`hcat(Int8[...], Float32[...])` が upstream Julia では `Matrix{Int16}` /
`Matrix{Float32}` になるのに sjulia では `Matrix{Any}` に落ち、
`vcat(Int8[...], Int16[...])` / `vcat(Int8[...], Float32[...])` が
`Vector{Int16}` / `Vector{Float32}` ではなく first input eltype を保持して
しまう bug は Issue #4655 として報告済みで、mixed vector concatenation
allocation を `promote_type` と `_array_undef_from_dims` へ移す形で
ローカル修正済み。

### stack mixed eltype promotion local fix (Issue #4018)

2026-05-27: `stack((Int8[1, 2], Int16[3, 4]))` /
`stack((Int8[1, 2], Float32[3, 4]))` が upstream Julia では
`Matrix{Int16}` / `Matrix{Float32}` になるのに sjulia では `Matrix{Any}` に
widening する bug は Issue #4652 として報告済みで、mixed stack result
allocation を input `eltype` の `promote_type` と `_array_undef_from_dims` へ
移す形でローカル修正済み。fixture 作成中に露出した identical typed/boxed
matrix equality gap は Issue #4653 で修正済み。

### cat mixed eltype promotion local fix (Issue #4018)

2026-05-27: `cat(Int8[1], Int16[2]; dims=1)` / `cat(Int8[1],
Float32[2]; dims=1)` が upstream Julia では `Vector{Int16}` /
`Vector{Float32}` になるのに sjulia では `Vector{Float64}` に widening し、
`cat(String["a"], Any["b"]; dims=1)` は `"a"` → `Float64` conversion error に
なる bug は Issue #4651 として報告済みで、mixed cat result allocation を
`promote_type(eltype(A), eltype(B))` と `_array_undef_from_dims` へ移す形で
ローカル修正済み。

### Partition vector chunk view eltype local fix (Issues #4648/#4649)

2026-05-27: `collect(Iterators.partition(Int8[1, 2, 3], 2))` が upstream Julia
では `Vector{SubArray{Int8,...}}` になるのに sjulia では `Vector{Any}` に
widening する bug は Issue #4648 として報告済みで、vector-backed
`Partition` が `SubArray` view chunks を返し、`SubArray{T}` collect allocation を
typed `_array_undef_from_dims` 経由にする形でローカル修正済み。修正中に露出した
`view(Int8[...], range)` が `SubArray{Float64}` になる narrow view bug は
Issue #4649 として報告し、generic `SubArray{T}` 1D view/interface methods を
追加してローカル修正済み。

### Rest explicit Array state parity local fix (Issue #4647)

2026-05-27: `collect(Iterators.rest(Int8[1, 2, 3], 2))` が upstream Julia では
`Int8[2, 3]` になるのに sjulia では legacy native Array iteration state を
0-based/current-index 扱いして `Int8[3]` だけを返す bug は Issue #4647 として
報告済みで、native Array `iterate` state を upstream の 1-based next-index
semantics に合わせる形でローカル修正済み。

### Array{Any}/Array{Real} boxed numeric IndexStore local fix (Issue #4646)

2026-05-27: `Array{Any}(undef, 1)[1] = Int8(1)` が upstream Julia では
`Int8` を保持するのに sjulia では scalar `F64` IndexStore path で `Float64`
へ widening される bug は Issue #4646 として報告済みで、direct index assignment
が Pure Julia `setindex!` に元の boxed numeric value を渡す形でローカル修正済み。
`Array{Real}` への `Float32` 代入も concrete value を保持する。

## 最新対応 (2026-05-26)

### generic similar(Array{T}, dims) tuple dispatch local fix (Issues #4569/#4643)

2026-05-26: `similar(Array{T}, dims)` が `where T` helper から
`dims::Tuple` を渡す場合に dispatch-first の Pure Julia method を通るよう修正済み。
未注釈 `dims` が runtime tuple のとき `DynamicToI64` に落ちる追加 bug は
Issue #4643 として報告し、`similar` lowering の `Any` dims を runtime dispatch
へ渡す形でローカル修正済み。runtime method search では `Tuple{Int64}` などの
concrete tuple runtime type が bare `::Tuple` に一致するよう補強した。
#4569 検証中に報告した `Array{T,length(dims)}` rank parameter bug は Issue #4644
として報告済みで、`ConstructParametricType` が integer value parameters を保持し
`Array{T,N}` を Julia type parser で正規化する形でローカル修正済み。

### mixed String equality fallback local fix (Issue #4642)

2026-05-26: `1 == "a"` / `"a" == 1` が upstream Julia では `false` だが
sjulia では static conversion error または dynamic `operator(Int64, String)`
MethodError になる bug は Issue #4642 として報告済みで、static/dynamic equality
fallback を Julia の generic equality result に合わせる形でローカル修正済み。

### prefix dotted not broadcast syntax local fix (Issue #4019)

2026-05-26: `.!xs` / `.!Bool[true, false]` が expression start の dotted
operator として扱われず parse error になる bug は Issue #4626 として報告済みで、
`Broadcasted(!, ...)` に lowering する prefix broadcast-not CST path を追加して
ローカル修正済み。検証中に露出した callable `!` function-value fallback gap は
Issue #4640 として報告し、VM の intrinsic fallback に通す形でローカル修正済み。

### Vector{Any} tuple destructuring local fix (Issue #4019)

2026-05-26: `Any[(name, values)]` が typed array constructor lowering で
tuple expression を index tuple として展開してしまい、`Vector{Any}` が
2 要素に壊れて `for (name, values) in cases` が `Expected Tuple, got String`
になる bug は Issue #4627 として報告済みで、type-like target の tuple expression
を typed array literal element として保持する形でローカル修正済み。

### runtime DataType typed array constructor local fix (Issue #4018)

2026-05-26: runtime `DataType` typed vector constructor `T[1, 2]` /
`getindex(T, 3, 4)` / abstract `Real[...]` / `Number[...]` が `DataType`
indexing として落ちる bug は Issues #4586/#4606、abstract typed vector が
`Float64` / `Float32` values を truncation する bug は Issue #4641 として
報告済みで、`IndexLoad` の `Value::DataType` target から runtime element type の
typed vector を確保し concrete values を保持する形でローカル修正済み。

### typed Vector equality bridge local fix (Issue #4639)

2026-05-26: `Int16[1, 2] == Int16[1, 2]` が
`MethodError: no method matching operator(Vector{Int16}, Vector{Int16})` で
落ちる bug は Issue #4639 として報告済みで、`CallDynamicBinaryBoth` の
no-method equality fallback に legacy native Array 同士の局所 bridge を追加して
ローカル修正済み。#3908 の残り作業では、最終的にこの bridge ごと
Pure Julia Array wrapper / Memory-first path へ寄せる。

### typed matrix literal lowering local fix (Issues #4575/#4629)

2026-05-26: `Float32[1 2; 3 4]` / `Real[1 2.0; 3 4.0]` が lowering の
`UnsupportedExpression("typed_expression: ...")` で落ちる bug は Issue #4575
および Issue #4629 として報告済みで、typed vector constructor +
`reshape` へ展開する lowering を追加してローカル修正済み。#4628 の broadcast
fixture は直接 typed matrix literal syntax を使う形に戻した。

### Dict Pair destructuring comprehension local fix (Issue #4018)

2026-05-26: `[k for (k, v) in Dict(Int8(1) => Int16(2))]` が lowering の
`UnsupportedForBinding` で落ちる bug は Issue #4632 として報告済みで、
array comprehension の tuple destructuring binding を既存の `iterate`
protocol loop へ接続してローカル修正済み。残りの generator expression 側の
tuple destructuring coverage は #4568 の final fallback cleanup と合わせて
継続する。

### map/reduction dispatch-first handoff (Issue #4019)

2026-05-26: `reduce(+, ::Vector{T})` / `foldl(+, ::Vector{T})` /
`foldr(+, ::Vector{T})` と `reduce(*, ::Vector{T})` /
`foldl(*, ::Vector{T})` / `foldr(*, ::Vector{T})`、および
`mapfoldl(identity, +, ::Vector{T})` /
`mapreduce(identity, +, ::Vector{T})` / `mapfoldr(identity, +, ::Vector{T})`
と `mapfoldl(identity, *, ::Vector{T})` /
`mapreduce(identity, *, ::Vector{T})` / `mapfoldr(identity, *, ::Vector{T})`
、および `reduce(-, ::Vector{T})` / `foldl(-, ::Vector{T})` /
`foldr(-, ::Vector{T})` と `mapfoldl(identity, -, ::Vector{T})` /
`mapreduce(identity, -, ::Vector{T})` / `mapfoldr(identity, -, ::Vector{T})`
の representative typed Vector paths は empty identity と result type を
upstream Julia に合わせた。`-` 系では left/right fold order と unsigned
wraparound result も upstream Julia に合わせた。作業中に露出した empty no-init
reduction 例外と small integer widening は Issue #4619 / #4620 / #4622 /
#4623 / #4625 として報告し、
ローカル修正済み。Broadcast materialization は representative `Float32` /
small integer / unsigned integer / `String` result allocation を upstream Julia
に合わせ、widening bug を Issue #4628 として報告してローカル修正済み。
Probe 中に露出した `.!` parser gap と `Vector{Any}` tuple destructuring gap は
Issue #4626 / #4627 として報告済みで、いずれもローカル修正済み。#4019 は
representative broadcast / HOF / reduction migration と validation を完了して
closed。残る native carrier cleanup は #4568 の scope として扱う。

### zeros/ones/fill tuple-dims similar(Array) allocation remaining scope (Issue #4018)

2026-05-26: `fill(value, dims...)` と typed tuple-dims `zeros` / `ones`
は Pure Julia `similar(Array{T}, dims)` wrapper allocation に移行した。
これで `zeros(Int64, (2, 3))` / `ones(Int64, (2, 3))` /
`zeros(Complex{Float64}, (2, 2))` が上流 Julia と同じ型・shape を返す。
4D tuple-dims `fill` / `zeros` と `similar(Array{Real}, (2, 2))` も
fixture で検証済み。
Issue #4569 の `Type{Array{T}}` runtime dispatch gap も解消し、
generic helper からの `similar(Array{T}, dims)` は constructor fallback
なしで通る。
compiler lowering の `Array{T}(undef, dims...)` / tuple-dims path も
`_array_undef_from_dims(T, dims_tuple)` call に寄せたため、この boundary は
direct `AllocUndef*` instruction ではなく Pure Julia helper dispatch を通る。
この作業中に露出した runtime `DataType` callable dispatch と Any-typed
`println` coercion regression は Issue #4580 として報告し、ローカル修正済み。
同じ helper 経路は `adjoint(::LinRange)` / `adjoint(::StepRangeLen)` の
row-vector allocation にも適用済み。
さらに shaped collect materialization の一部 concrete element type は
`iterators.jl` から `_array_undef_from_dims` 経由に移行済み。
Issue #4574 の `Type{Any}` specificity bug も解消し、`Any` shaped collect
も direct constructor fallback ではなく `_array_undef_from_dims(Any, dims)`
経由に移行済み。
generic `_array_for_inner_shape(::Type{T}, dims)` の rank 別 direct constructor
fallback も削除済みで、`Symbol` runtime type object が `Any` allocation に落ちる
Issue #4577 も解消済み。
`Symbol` / `Float32` / 3D Array source の shaped generator collect result も
fixture 化済み。`Symbol` source は parser が `[:x :y]` を `(:x):y`
range と誤読する Issue #4576 を修正して通した。3D Array source は
#4579 の Array wrapper collect fallback 修正後に Issue #4573 の再現が通る
ようになったため fixture に戻した。Float32 result は Issue #4582 の
`===` parity 修正後に strict identity まで fixture で検証済み。
`BuiltinId::Similar` の cached-bytecode compatibility fallback は全 receiver
type で `similar` / `Base.similar` method table を先に引くようになり、
user-defined method が dimension parsing / direct allocation fallback より先に
選ばれる。
VM-native range collect bridge の UnitRange / StepRange / LinRange /
StepRangeLen result allocation も `_array_undef_from_dims` 経由に移行済み。
この作業中に露出した tuple element Array wrapper の Memory `IndexStore`
bug は Issue #4578、Array wrapper collect fallback bug は Issue #4579
として報告し、どちらもローカル修正済み。
runtime `collect(x::Any)` から `collect(::Array{T})` user method が
`Vector{T}` 実引数で選ばれない dispatch gap は Issue #4581 として報告し、
`Array{T}` を次元未束縛の invariant element pattern として扱う形で
ローカル修正済み。

#4018 の残りは、range 以外の collect materialization fast paths をさらに
Pure Julia wrapper dispatch へ寄せる作業。`unique(::Vector{Int8/Int16/Int32/UInt8/UInt16/UInt32/UInt64/Float32/Symbol/Any})`
と対応する `unique(f, arr)`、および same-element `union` / `intersect` /
`setdiff` / `symdiff` の代表 concrete Vector methods は `similar(..., 0)`
経由に移行済み。
Mixed-element Vector set algebra の representative paths は
`promote_type(eltype(a), eltype(b))` に基づく `_array_undef_from_dims` allocation
へ移行済み。`Vector{Any}` に広がる bug は Issue #4630 として報告し、
ローカル修正済み。
Comprehension-backed generator expression collect の representative narrow scalar
paths は typed result allocation に移行済み。`Int8` source generator が
`Vector{Float64}` に広がる bug は Issue #4631 として報告し、
ローカル修正済み。
Pair-valued `collect(pairs(::Vector{Int8}))` / `collect(pairs(::Tuple{Int8,Int8}))`
は Pure Julia `_pairs_collect_dynamic` 経由で `Vector{Pair{Int64, Int8}}` を
保持するようになった。widening bug は Issue #4634 として報告し、
ローカル修正済み。general `Array{Pair{K,V}}(undef, dims)` /
`similar(Array{Pair{K,V}}, dims)` の public typed allocation は Issue #4635
でローカル修正済み。direct `_array_undef_from_dims(Pair{K,V}, dims)` static
dispatch / reflection mismatch は Issue #4636 として報告し、ローカル修正済み。
`Dict(Int8(...) => Int16(...))` が narrow integer key を拒否する bug は
Issue #4633 として報告し、ローカル修正済み。検証中に露出した narrow integer
`===` / tuple `in` gap は Issue #4637 として報告し、ローカル修正済み。
`Set(Float32[...])` / `Dict(1.0 => ...)` が floating-point key を拒否する
bug は Issue #4638 として報告し、ローカル修正済み。
未束縛 `Vector` signature は current dispatch で `Any` fallback と曖昧になるため
未採用。抽象 typed literal の `Real[...]` / `Number[...]` crash は Issue #4586
として切り出し、runtime DataType typed-array constructor fixture に固定済み。
heterogeneous `Vector{Any}` の mixed `isequal` error は Issue #4587 として
切り出し、ローカル修正済み。raw mixed `==` / `!=` fallback の broader parity gap
は Issue #4642 として報告し、ローカル修正済み。Array wrapper 化で露出した `in(::Any, ::Vector{Complex{Float64}})`
の `StructRef` receiver rejection は Issue #4588 として切り出し、ローカル修正済み。
`Vector{Any}` equality の `operator(Vector{Any}, Vector{Any})` gap は Issue #4589
として切り出し、ローカル確認済み。

`cumsum` / `cumprod` は `Int64` / small signed integers / unsigned integers /
`Float32` / `Float64` / `Bool` の concrete Vector methods を追加し、
上流 Julia と同じ result element type を確保するように移行済み。
この作業中に露出した `cumsum` / `cumprod` が `zeros(n)` により常に
`Vector{Float64}` を返す bug は Issue #4590 として報告し、ローカル修正済み。

`selectdim` / `dropdims` の unsupported `MatrixView` eltype fallback は
`similar(A, n)` / `similar(A, m)` allocation に移行済み。`Matrix{String}`
fallback が `Vector{Float64}` を確保して conversion error になる bug は
Issue #4591 として報告し、ローカル修正済み。

`cat(A, B; dims)` の same-eltype input result allocation は `_cat_result_like`
経由で `similar(A, ...)` に移行済み。`String` vectors/matrices が
`Vector{Float64}` / `Matrix{Float64}` を確保して conversion error になる bug は
Issue #4592 として報告し、ローカル修正済み。mixed eltype input が current
numeric widening fallback で upstream と異なる promoted eltype を失う bug は
Issue #4651 として報告し、`promote_type(eltype(A), eltype(B))` に基づく
typed allocation へ移行済み。

`mapslices` の row/column temporary slice は `similar(A, len)`、result は
最初の `f(slice)` の runtime type と matrix result shape から
`_array_undef_from_dims` で確保するように移行済み。`Matrix{String}` の temp
slice が `Vector{Float64}` を確保して conversion error になる bug は
Issue #4593 として報告し、ローカル修正済み。

`sortslices(A; dims)` の result matrix は `similar(A, m, n)` allocation に
移行済み。`Matrix{String}` input が `Matrix{Float64}` result を確保して
conversion error になる bug は Issue #4594 として報告し、ローカル修正済み。

`minimum` / `maximum` の dims result は `similar(A, ...)` allocation に移行済み。
`sum(::Matrix{Bool}; dims)` は upstream に合わせ `Matrix{Int64}` result に移行済み。
`extrema(A; dims)` は upstream と同じ `Matrix{Tuple{T,T}}` shape に移行済み。
String reduction dims conversion error は Issue #4595、Bool sum dims result-type
mismatch は Issue #4596 として報告し、ローカル修正済み。

`adjoint(::Array)` の materialized result allocation は `Float32` typed branch を
追加済み。`Float32` arrays が `Matrix{Float64}` result に広がる bug は
Issue #4597 として報告し、ローカル修正済み。

`permutedims(A, perm)` の explicit permutation result allocation は
`similar(A, ...)` に移行済み。`Float32` arrays が `Float64` result に広がる
bug は Issue #4598 として報告し、ローカル修正済み。
no-permutation `permutedims(A)` も Issue #4656 follow-up で `similar(A, ...)`
allocation に移行済み。`Int8` / `Float32` inputs が `Matrix{Any}` result に
広がる bug は Issue #4656 として報告し、ローカル修正済み。

`empty(::Array{T})` / `empty(arr, ::Type{S})` は typed 0-length allocation に
移行済み。source/requested element type に関係なく `Vector{Float64}` を返す
bug は Issue #4599 として報告し、ローカル修正済み。

`diff(arr)` は `similar(arr, n - 1)` allocation に移行済み。non-Float64 vector
input が `Vector{Float64}` に広がる bug は Issue #4600 として報告し、
ローカル修正済み。

`adjoint(::Array)` の materialized result allocation は `Bool` typed branch を
追加済み。`Bool` arrays が `Matrix{Float64}` result に広がる bug は
Issue #4601 として報告し、ローカル修正済み。

`adjoint(::Array)` の materialized result allocation は small signed/unsigned
integer typed branches を追加済み。`Int8` / `UInt8` arrays が `Matrix{Float64}`
result に広がる bug は Issue #4602 として報告し、ローカル修正済み。

`stack(arrays)` の homogeneous input path は `similar(first_arr, m, n)` allocation
に移行済み。typed vectors が `Matrix{Any}` result に広がる bug は Issue #4603
として報告し、ローカル修正済み。mixed-eltype input が `Matrix{Any}` fallback
で upstream と異なる promoted eltype を失う bug は Issue #4652 として報告し、
`promote_type` に基づく typed allocation へ移行済み。

`adjoint(::Array)` の materialized result allocation は `Complex{Float64}`
typed branch を追加済み。Complex arrays が `StructRef` を dimension として
coercion して `expected I64` runtime error になる bug は Issue #4604 として
報告し、ローカル修正済み。`ComplexF64[...]` / `Complex{T}[...]` typed
literal が同じく element value を dimension として coercion する bug は
Issue #4605 として報告し、runtime DataType typed-array constructor が raw values
を保持する形でローカル修正済み。

`hcat` の homogeneous vector result allocation は small signed/unsigned integer
branches を追加済み。`Int16` / `UInt8` などが `Matrix{Any}` result に広がる
bug は Issue #4607 として報告し、ローカル修正済み。probe 中に見つけた
runtime `T[...]` constructor bug は Issue #4606 として切り出した。
mixed vector input が `Matrix{Any}` または first input eltype に落ちて
upstream と異なる promoted eltype を失う bug は Issue #4655 として報告し、
`promote_type` に基づく typed allocation へ移行済み。

`accumulate(+, A)` / `accumulate(*, A)` と generic `accumulate(f, A::Array)` の
representative typed Vector paths は typed result allocation に移行済み。`[]`
fallback により `Vector{Any}` result に広がる bug は Issue #4608 として報告し、
ローカル修正済み。generic callable で accumulator が `Float32` などに promote
される covered Array path も Issue #4645 として報告後にローカル修正済み。
`=>` body を持つ arrow function の precedence も修正し、changing accumulator
shape は `Vector{Any}` result に到達する。single-element / empty `Int8` vector
cases も sample-value based `_accumulate_promote_op` approximation で `/` =>
`Vector{Float64}`、`Float32`-promoting lambda => `Vector{Float32}`、`=>` lambda =>
`Vector{Any}` に合わせた。full `_accumulate_promote_op` generalization beyond the
covered representative types remains Issue #4645 の残スコープ。

## 最新対応 (2026-05-25)

### compile/inference_trace audit cleanup (Issue #3908)

2026-05-25: `compile/inference_trace.rs` の `serialize_env` 内で
`use serde_json::{json, Value};` 経由のローカル別名により
`Value::Array(entries)` が出現していたが、これは `serde_json::Value::Array`
(ランタイムの `crate::vm::value::Value::Array` とは別物) の false positive
であった。ローカル `Value` 別名を撤去し、`serde_json::Value::Null` は
完全修飾、配列構築は `Vec<serde_json::Value>::into()` 経由に書き換えて
literal `Value::Array` テキストを 1 → 0 に削減。`scripts/check_value_array_allowlist.sh`
から該当エントリを削除。出力 JSON は完全に同一であり、本タスクは
audit ノイズ削減のみ。Array/Memory 本体移行は引き続き別タスクで継続する。

### vm/exec/hof helper delegation (Issue #3908)

2026-05-25: `vm/exec/hof.rs` の `array_value(ArrayValue) -> Value`
本体が `Value::Array(new_array_ref(arr))` を直接構築していたため、同じ
ファイルの `array_ref_value(ArrayRef) -> Value` と合わせて literal
`Value::Array` が 2 か所カウントされていた。`array_value` の本体を
`array_ref_value(new_array_ref(arr))` に書き換え、構築は
`array_ref_value` 1 か所のみに集約。`scripts/check_value_array_allowlist.sh`
の `vm/exec/hof.rs` ceiling を 2 → 1 に引き下げ。残る Pure Julia
`Array{T,N}` ラッパへの本格移行は引き続き別タスクで継続する。

### vm/hof_exec/dispatch helper delegation (Issue #3908)

2026-05-25: `vm/hof_exec/dispatch.rs` の `array_value(ArrayValue) -> Value`
本体が `Value::Array(new_array_ref(arr))` を直接構築していたため、同じ
ファイルの `array_ref_value(ArrayRef) -> Value` と合わせて literal
`Value::Array` が 2 か所カウントされていた。`array_value` の本体を
`array_ref_value(new_array_ref(arr))` に書き換え、構築は
`array_ref_value` 1 か所のみに集約。`scripts/check_value_array_allowlist.sh`
の `hof_exec/dispatch.rs` ceiling を 2 → 1 に引き下げ、コメントで
delegation を明示。残る Pure Julia `Array{T,N}` ラッパへの本格移行は
引き続き別タスクで継続する。

### compile/expr/coercion comment cleanup (Issue #3908)

2026-05-25: `compile/expr/coercion.rs` の `Struct -> Array` coercion 直前に
あった通常コメントから literal `Value::Array` 1 件を「legacy native-array
container」と言い換えで除去。`scripts/check_value_array_allowlist.sh` の
`coercion.rs` allowlist エントリ自体を削除した (ファイルは rg 検索で
0 ヒットとなり audit 対象から外れる)。Pure Julia `Array{T,N}` ラッパへの
本格移行は引き続き別タスクで継続する。

### vm/exec/binary_both doc cleanup (Issue #3908)

2026-05-25: `vm/exec/binary_both.rs` の `legacy_array_ref_from_value` /
`array_value` ヘルパ doc comment に含まれていた literal `Value::Array` 2 件を
「legacy native-array operand」「legacy native-array construction」と
言い換えで除去。`scripts/check_value_array_allowlist.sh` の
`binary_both.rs` ceiling を 7 → 5 に引き下げた。残る 5 件は 3 つの
ヘルパ本体 (`is_legacy_array_value` / `legacy_array_ref_from_value` /
`array_value`) と Memory<->Array equality bridge の 2 件 tuple-pattern
arm のみ。Pure Julia `Array{T,N}` ラッパへ完全移行するまでヘルパ経由の
境界化を続ける。

### vm/type_ops/iteration doc cleanup (Issue #3908)

2026-05-25: `vm/type_ops/iteration.rs` の `legacy_array_value_ref` ヘルパ
doc comment に含まれていた literal `Value::Array` 2 件を「legacy native
Array carrier」「raw native-array destructures」と言い換えで除去。
`scripts/check_value_array_allowlist.sh` の iteration.rs ceiling を
4 → 2 に引き下げた。残る 2 件は `legacy_array_value_ref` 本体と
`array_value` 本体のみ。Pure Julia `Array{T,N}` ラッパへ完全移行するまで
ヘルパ経由の境界化を続ける。

### vm/builtins_types doc comment cleanup (Issue #3908)

2026-05-25: `vm/builtins_types.rs` の `legacy_array_value_ref` ヘルパ
doc comment に含まれていた literal `Value::Array(_)` 2 件を「legacy
native-array carrier」「raw native-array destructure pattern」と
言い換えで除去。`scripts/check_value_array_allowlist.sh` の
builtins_types.rs ceiling を 4 → 2 に引き下げた。残る 2 件は
`legacy_array_value_ref` 本体と `any_vector_array_value` 本体のみ。
Pure Julia `Array{T,N}` ラッパへ完全移行するまでヘルパ経由の境界化を
続ける。

### vm/formatting doc comment cleanup (Issue #3908)

2026-05-25: `vm/formatting.rs` の `legacy_array_value_ref` ヘルパ doc
comment の literal `Value::Array(_)` を「legacy native-array carrier
variant」と言い換えで除去。`scripts/check_value_array_allowlist.sh` の
formatting.rs ceiling を 5 → 4 に引き下げた。残る 4 件は helper 本体
+ `format_value_slow` / `value_to_string` の網羅 arm 2 件 + テスト
コンストラクタ本体。Pure Julia `Array{T,N}` ラッパへ完全移行するまで
ヘルパ経由の境界化を続ける。

### vm/exec/call_dynamic Array helpers (Issue #3908)

2026-05-25: `vm/exec/call_dynamic.rs` の `can_score_iterate_dynamic_candidates`
の `matches!(value, Value::Struct(_) | Value::StructRef(_) | Value::Array(_))`
と `native_array_rank_count` の `Value::Array(arr) => { ... }` arm の 2 か所
の直接 destructure を、新規 file-local helper
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` 経由に集約。predicate
側は `matches!(value, Value::Struct(_) | Value::StructRef(_)) ||
legacy_array_value_ref(value).is_some()`、rank/count 側は
`legacy_array_value_ref(iter)?.borrow()` の rank / element_count /
empty-shape タプル返却に書き換え、`IterateDynamic` の native Array scoring と
generator-iter サイズ判定の意味論はそのまま。allowlist ceiling を 2 → 1 に
縮小し、残る 1 件は helper 本体のみ。Issue #3908 の Pure Julia
`Array{T,N}` ラッパ移行が完了し `Value::Array` が runtime から消える段階で、
helper 本体も撤去できる見込み。

### vm/builtins_io Array construction helper (Issue #3908)

2026-05-25: `vm/builtins_io.rs` の `Readlines` / `Readdir` 分岐が直接
書いていた 2 か所の `self.stack.push(Value::Array(new_array_ref(arr)))`
構築を、新規 file-local helper `array_value(ArrayValue) -> Value` 経由に
集約 (PR #4476 / #4482 / #4488 と同形)。`readlines` (`Vector{String}`
風) / `readdir` (sort 済み名前 `Vector{String}` 風) いずれも
`ArrayValue::any_vector(...)` 経由で生成しており、I/O 結果の Array
wrapping 経路を 1 か所に揃えた。allowlist ceiling を 2 → 1 に縮小し、
残る 1 件は helper 本体のみ。Issue #3908 の Pure Julia `Array{T,N}`
ラッパ移行が完了し `Value::Array` が runtime から消える段階で、helper
本体も撤去できる見込み。

### vm/exec/range Array construction helper (Issue #3908)

2026-05-25: `vm/exec/range.rs` の `MakeRange` / `MakeRangeF64` が直接
書いていた 2 か所の `self.stack.push(Value::Array(new_array_ref(arr)))`
構築を、新規 file-local helper `array_value(ArrayValue) -> Value` 経由に
集約 (PR #4476 / #4482 と同形)。`MakeRangeLazy` は `Value::Range` を
push する分岐のため対象外。allowlist ceiling を 2 → 1 に縮小し、残る
1 件は helper 本体のみ。Issue #3908 の Pure Julia `Array{T,N}` ラッパ
移行が完了し `Value::Array` が runtime から消える段階で、helper 本体も
撤去できる見込み。

### builtins_strings try_chars_to_string_from_array_like routing (Issue #3908)

2026-05-25: `vm/builtins_strings.rs` の `try_chars_to_string_from_array_like`
を既存の file-local helper `legacy_array_ref_from_value(&Value) ->
Option<&ArrayRef>` 経由に書き換え、直接の `Value::Array(arr) => { ... }`
arm を `if let Some(arr) = legacy_array_ref_from_value(value) { ... }`
に置換。`Value::Memory(mem)` 分岐は `if let` ガードで保持、末尾の
非配列・非メモリ値に対する `Ok(None)` フォールバックも維持。
allowlist ceiling を 3 → 2 に縮小。残る 2 件は `array_value` コンストラクタと
`legacy_array_ref_from_value` helper 本体のみで、Array-wrapper `_mem`
reader / `_substring_retag` builtin はすでに同 helper 経由に集約済み。
Issue #3908 の Pure Julia `Array{T,N}` ラッパ移行が完了し `Value::Array`
が runtime から消える段階で、両 helper 本体も撤去できる見込み。

### vm/exec/array_index destructure helpers (round 5) remaining scope (Issue #3908)

2026-05-25: `vm/exec/array_index.rs` の追加 3 か所の native Array 直接
destructure を既存の file-local helper
`array_ref_from_value(Value) -> Result<ArrayRef, Value>` 経由に集約
(round 4 に続く round 5)。対象は `IndexLoadTyped` の `match index_val`
内の `Value::Array(idx_arr_ref) => ...` arm (logical boolean/integer
index 経路)、`IndexStoreTyped` の `match self.stack.pop()`
(struct-array 経由の `setindex!` / multi-dispatch / INTERNAL fallback)、
`IndexLoad` の `match val` 内の `Value::Array(idx_arr_ref) => ...` arm
(logical index と Dict/StructRef-Dict 多重ディスパッチ) の 3 か所。
allowlist ceiling を 7 から 4 に縮小。残る 4 件は `array_value` /
`array_ref_from_value` / `sub_array_parent_array_ref` の各ヘルパ本体と、
Generator underlying iter の borrowed `&Value::Array` arm のみ。最後の
Generator borrow site は `match g.iter.as_ref()` で借用参照を使うため、
helper の所有 `Value -> Result<ArrayRef, Value>` シグネチャと合致せず
別形 helper か Pure Julia Array wrapper 経由への置換が必要。Issue #3908
が Pure Julia `Array{T,N}` ラッパ移行を完了し `Value::Array` 自体が
runtime から消えた段階で全ヘルパ本体と Generator arm も撤去できる
見込み。

### vm/exec/array_index destructure helpers (round 4) remaining scope (Issue #3908)

2026-05-25: `vm/exec/array_index.rs` の追加 3 か所の native Array
直接 destructure を既存の file-local helper
`array_ref_from_value(Value) -> Result<ArrayRef, Value>` 経由に集約
(round 3 に続く round 4)。対象は
`selected_indices_from_array_wrapper` の `mem` ディスパッチ (Pure Julia
Array wrapper の `_mem::Array` 経由読み出しと `Value::Memory` 経由の
linear 読み出しを `Ok(array_ref)` / `Err(Value::Memory(_))` / `Err(_)
=> Ok(None)` の三分岐で保持)、`load_selected_array_elements` の
`target` ディスパッチ (logical indexing 用の `create_sliced_array`
ラッパは不変)、`IndexLoadTyped` の `match self.stack.pop()` (Array /
Struct/StructRef 経由の `getindex` 多重ディスパッチ / INTERNAL fallback
を `Err(target @ ...)` / `Err(_)` で残す) の 3 か所。allowlist ceiling
を 10 から 7 に縮小。残る 7 件は `array_value` /
`array_ref_from_value` / `sub_array_parent_array_ref` の各ヘルパ本体、
論理 boolean/integer インデックス読み込みの 3 か所 (`IndexLoadTyped` /
`IndexLoad` の `match index_val` 2 か所と `IndexStoreTyped` の
`match self.stack.pop()`)、および Generator underlying iter の
`match g.iter.as_ref()` の Array arm。これら残存 site は次ラウンド以降で
Pure Julia Array wrapper / Memory-first 経路への置換と統合可能で、
最終的に `Value::Array` 自体が runtime から消えた段階で全ヘルパ本体も
撤去できる見込み。

### vm/frame.rs Array helper remaining scope (Issue #3908)

2026-05-25: `vm/frame.rs` の `Frame::get_by_tag` (`VarTypeTag::Array` arm) と
`Frame::get_by_cascade` (`self.locals_array.get(name)` arm) における
2 か所の native Array carrier 構築 `Value::Array(v.clone())` を、新規
file-local `array_value(arr: ArrayRef) -> Value` 経由に集約。
allowlist ceiling を 2 から 1 に縮小。残る 1 件はヘルパ本体の構築のみ。
`var_types` ベースの O(1) tag ディスパッチと `get_by_cascade` の
fallback chain は据え置き、ローカル変数 lookup の意味論は不変。
Issue #3908 が Pure Julia `Array{T,N}` ラッパ移行を完了し、Frame の
`locals_array: HashMap<String, ArrayRef>` ストレージ自体が
Memory 保有型へ置き換わった段階で `array_value` も撤去できる見込み。

### vm/builtins_linalg Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/builtins_linalg.rs` に file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` ヘルパを追加し、
`with_linalg_array` の native Array 直接 destructure と
`linalg_array_wrapper_value` の Pure Julia Array wrapper `_mem::Array`
リーダ arm をヘルパ経由に書き換え。前者は `if let Some(arr_ref) =
legacy_array_value_ref(&val) { ... return ... }` の早期 return、後者は
`_ if legacy_array_value_ref(mem).is_some() => ...` のガード付き arm。
各々の outer match は既存の `_ =>` / `other =>` フォールバックを保持。
`linalg_array_value(ArrayValue) -> Value` 構築ヘルパは前回ラウンドのまま。
allowlist ceiling を 3 から 2 に縮小。残る 2 件は `linalg_array_value`
本体と `legacy_array_value_ref` 本体のみ。`det` / `inv` / `lu` /
`eigvals` / `eigen` / `qr` / `svd` などの LinearAlgebra カーネルは
Pure Julia `Array{T,N}` ラッパへの移行 (Issue #3908) が完了した段階で、
これらヘルパ自体も撤去できる見込み。

### vm/exec/array_mutate destructure helpers remaining scope (Issue #3908)

2026-05-25: `vm/exec/array_mutate.rs` の consumed `Value::Array(arr)`
直接 destructure 4 か所 (`array_mutation_target` ヘルパ、`Zero` ハンドラ、
`ArrayPush` / `ArrayPushTypejoin` ハンドラ、`ArrayPop` ハンドラ) を
file-local `try_consume_array_value(value: Value) -> Result<ArrayRef,
Value>` 経由に書き換え、`match try_consume_array_value(value) { Ok(arr) =>
..., Err(other) => match other { ... } }` の二段マッチ構造に統一。
Memory / Set / fallback の各分岐、`try_or_handle` の `Continue` 経路、
`raise` 後の `Continue`、push! のメソッド検索による start_function_call
経路はいずれも従来動作を保持。`array_mutation_target` を経由する
`ArrayPushFirst` / `ArrayPopFirst` / `ArrayInsert` / `ArrayDeleteAt` も
据え置き。allowlist ceiling を 5 から 2 に縮小。残る 2 件は
`try_consume_array_value` ヘルパ本体と `push_array_ref` ヘルパ本体のみ。
Issue #3908 が Pure Julia `Array{T,N}` ラッパへの移行を完了し、
`Value::Array` 自体が退役した後にこれらヘルパも撤去できる見込み。

### vm/exec/struct_ops Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/exec/struct_ops.rs` の `NewStructSplat` `match val` 内の
`Value::Array(arr) =>` 直接 destructure arm を file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` 経由のガード付き
`_ if legacy_array_value_ref(&val).is_some() =>` arm に書き換え、surrounding
match の `_ =>` フォールバックを保持。`GetFieldByName` 内の
Pure Julia Array wrapper bridge (`._mem` / `._size`) を
`if let Some(arr) = legacy_array_value_ref(&val) { ... }` に集約し、
`"_mem" => Value::Array(arr.clone())` 構築を file-local
`array_value(arr: ArrayRef) -> Value` ヘルパに集約。allowlist ceiling を 3 から
2 に縮小。残る 2 件はヘルパ本体のみ。Issue #3908 が Pure Julia `Array{T,N}`
ラッパへの移行を完了し、`Value::Array` 自体が退役した後にこれらヘルパも
撤去できる見込み。

### dynamic_ops/dispatch Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/dynamic_ops/dispatch.rs` の
`should_use_inline_dynamic_op` 内で並んでいた `Value::Array` 直接 destructure
3 件 (Array×Array タプル `if let`、片側 `if let Value::Array(arr) = a`、片側
`if let Value::Array(arr) = b`) を file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` ヘルパ経由の `Some(arr)`
取得に集約。allowlist ceiling を 3 から 1 に縮小。残る 1 件はヘルパ本体の
`Value::Array(arr) => Some(arr)` arm のみ。Issue #3908 が Pure Julia
`Array{T,N}` ラッパへの移行を完了し、`Value::Array` 自体が退役した後に
ヘルパも撤去できる見込み。

### builtins_macro/mod Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/builtins_macro/mod.rs` の `Expr` splat arm にあった
legacy native Array 直接 destructure を file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` ヘルパ経由のガード付き
arm に集約し、併せて `RegexSplit` / `RegexEachmatch` の構築 2 か所を
file-local `array_value(arr: ArrayValue) -> Value` ヘルパに集約。allowlist
ceiling を 3 から 2 に縮小。残る 2 件はヘルパ本体の
`Value::Array(new_array_ref(arr))` と `Value::Array(a) => Some(a)` arm のみ。
Issue #3908 が Pure Julia `Array{T,N}` ラッパへの移行を完了し、
`Value::Array` 自体が退役した後にこれらヘルパも撤去できる見込み。

### vm/exec/rng Array push helper remaining scope (Issue #3908)

2026-05-25: `vm/exec/rng.rs` の `RandArray` / `RandIntArray` / `RandnArray`
3 ハンドラの `Value::Array(new_array_ref(arr))` 直接構築を file-local
`array_value(arr: ArrayValue) -> Value` ヘルパ経由に集約し、allowlist
ceiling を 3 から 1 に縮小。残る 1 件はヘルパ本体の
`Value::Array(new_array_ref(arr))` arm のみ。Issue #3908 が Pure Julia
`Array{T,N}` ラッパへの移行を完了し、`Value::Array` 自体が退役した後に
このヘルパも撤去できる見込み。

### vm/util.rs Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/util.rs` の `pop_array_or_values` から legacy native Array
直接破壊代入を file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>`
ヘルパ経由の early-return に集約。`value_type_name` と `bind_value_to_frame`
の `Value::Array(_)` arm は `Value` 全変種を網羅する match に `_ =>`
フォールバックが無いため、`vm/formatting.rs` の `format_value_slow` /
`value_to_string` と同じ慣例で直接 arm のまま残す。allowlist ceiling は
3 を据え置き (ヘルパ本体 + 上記 2 件の exhaustive arm)。Issue #3908 が
Pure Julia `Array{T,N}` ラッパへの移行を完了し、`Value::Array` 自体が
退役した後にこのヘルパも残り 2 件の exhaustive arm もまとめて撤去できる
見込み。

### builtins_equality Array destructure remaining scope (round 2) (Issue #3908)

2026-05-25: `vm/builtins_equality.rs` の `Isequal` / `Hash` / `Egal` から
残っていた legacy native Array 直接マッチ 3 箇所 (`array_like_logical_view`
内側 arm、`try_hash_array_like` 内側 arm、`Egal` の
`(Value::Array(a), Value::Array(b))` タプルパターン) を file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` ヘルパ経由の
early-return またはガード付き arm に集約し、allowlist ceiling を 3 から
1 に縮小。残る 1 件はヘルパ本体の `Value::Array(a) => Some(a)` arm のみ。
`===` の reference identity、`isequal` の shape + 論理 element-wise
比較、`hash` の linear element ハッシュは従来通り `ArrayValue` 経由で
処理される。Issue #3908 が進めば、native ArrayValue 自体が Pure Julia
`Array{T,N}` ラッパへ移ったあとにこのヘルパ本体も撤去できる見込み。

### builtins_collections Array destructure remaining scope (Issue #3908)

2026-05-25: `vm/builtins_collections.rs` の `Length` / `Eltype` ハンドラに
あったネイティブ Array の直接マッチ 3 箇所を `legacy_array_value_ref`
ヘルパ経由のガード付きアームに集約。allowlist ceiling は 3 から 1 に
下がった。残るのはヘルパ本体だけで、`Length` の Generator 内側マッチや
`Eltype` 直下のフォールバックも従来通り動作する。Issue #3908 / #4018 が
進めば、ArrayValue を Memory-first ストレージへ寄せたあとにヘルパ本体も
撤去できる見込み。

### Package include cache remaining scope (Issue #4452)

2026-05-25: package module cache hashes now include recursively loaded literal
`include("...")` file content, so bundled packages such as Plots invalidate
stale cached modules when included sources change. Remaining scope is to replace
the lightweight literal-include scanner with a CST-level include dependency
collector if/when package loading supports computed include paths beyond the
current literal include subset.

### iteration.rs destructure helpers remaining scope (round 2) (Issue #3908)

2026-05-25: `vm/type_ops/iteration.rs` の destructure 側 legacy
`Value::Array` 8 箇所 (matrix shape プローブ 3 箇所、`_mem::Array` 線形
読み出し、`iterate_first` / `iterate_next` / `collect_iterator` の Array
arm、`collect_iterator_values` の materialize 後 unwrap) を file-local
`legacy_array_value_ref(&Value) -> Option<&ArrayRef>` 経由のガードアーム
あるいは `let-else` に置き換え、allowlist 上限を 9 から 4 に縮小 (削減 5)。
残る 4 件はヘルパ本体 (`array_value` / `legacy_array_value_ref`) と
そのドキュメントコメントのみで、これ以上のヘルパ集約は不要。次の縮小は
Issue #4018 (`wrap(Array, Memory, dims)` / Pure Julia Array wrapper) に
合わせて `legacy_array_value_ref` 自体を撤去する形で達成する予定。

### builtins_types Array destructure helpers remaining scope (Issue #3908)

2026-05-25: `vm/builtins_types.rs` の legacy native-array destructure 5 件
(`Typeof` / `Isa` / `Sizeof` / `Objectid` / `In`) と `Ismutable` の OR
パターン 1 件は file-local `legacy_array_value_ref(&Value) -> Option<&ArrayRef>`
経由のガードアームに集約済み。各 match の `_ =>` フォールバックを保つので
exhaustiveness は維持される。allowlist 上限は 7 から 4 に下がった。残る
4 件は `any_vector_array_value` 構築ヘルパ本体、`legacy_array_value_ref`
ヘルパ本体、およびヘルパ doc コメント内の `Value::Array(_)` 言及 2 件。
次の縮小は `any_vector` 構築側を Memory プリミティブ + Pure Julia Array
wrapper の境界に完全移行することで達成する予定。

### binary_both Array constructions helper remaining scope (round 3) (Issue #3908)

2026-05-25: `vm/exec/binary_both.rs` の matmul / スカラー × 配列 / 配列 ×
配列の各フォールバックで legacy native-array を書き戻していた 4 箇所を
file-local `array_value(ArrayValue) -> Value` ヘルパ経由に集約し、
allowlist 上限を 9 から 7 に縮小。残りは三つのヘルパ本体 + そのドキュメント
コメント計 5 件と、Memory<->Array 等価ブリッジのタプルパターン 2 件のみ。
次の縮小は等価ブリッジ自体を `memory_array_values_equal` 側に閉じ込め、
binary 二項演算子で legacy native `Value::Array` を完全に経由しない形に
する Phase 3/4 (`wrap(Array, Memory, dims)` / Pure Julia Array wrapper)
と合わせて達成する予定。

### iteration.rs construction helpers remaining scope (Issue #3908)

2026-05-25: `vm/type_ops/iteration.rs` の生 `Value::Array` 構築 13 箇所
(`collect_zip_fields` / `collect_enumerate_fields` /
`collect_rest_fields` / `collect_logrange_fields` / および
`collect_generator_dispatch` 内 5 callable の empty fallback) を
file-local helper `array_value(ArrayValue) -> Value` 経由に集約し、
allowlist 上限は 21 から 9 に下がった。残る 9 件はヘルパ本体 1 件と
destructure 8 箇所 (matrix shape probe 62/86/109、`get_linear` の
linear getter 185、`iterate_first` / `iterate_next` の 602/1164/1761、
`collect_iterator_values` の collected unwrap 2055)。次の縮小は後続の
Issue #3908 destructure 回で `legacy_array_value_ref` 経由のガードアームに
集約することで達成する予定。

### builtins_dicts Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/builtins_dicts.rs` の `DictKeys` / `DictValues` /
`DictPairs` Array 分岐は新しい file-local
`any_vector_array_value` / `legacy_array_value_ref` 経由に集約済み。
残る 2 件 (`any_vector_array_value` / `legacy_array_value_ref` 各本体) は
allowlist 上限内で意図的に保持。次の対象候補は
`vm/builtins_equality.rs` (上限 3) や `vm/builtins_collections.rs` (上限 3)
など Public Base fallback 系の残ファイル。Memory プリミティブ + Pure
Julia Array wrapper への完全移行は Issue #3908 のスコープ。

### array_index_slice Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/exec/array_index_slice.rs` の legacy native-array
destructure 4 箇所と slice result re-push 3 箇所を file-local helper
(`legacy_array_value_ref` / `array_value`) 経由に集約し、allowlist 上限
は 7 から 2 に下がった。残りは両ヘルパ本体のみ。次の縮小は legacy
`Value::Array` carrier そのものを slicing 表面から完全に除去し、
`array_wrapper_logical_values` / `value_to_slice_index` / `execute_index_slice`
が Pure Julia `Array{T,N}` wrapper のみを受け入れる形へ移行することで、
Phase 3/4 (`wrap(Array, Memory, dims)` / public reshape の Pure Julia 化)
と合わせて達成する予定。

## 最新対応 (2026-05-24)

### `methods(f)` の `MethodList` 型名 / `Method` show 表示 (Issue #5125)

2026-05-30: ユーザー定義 generic function の `methods(f)` はメソッド数カウントと
イテレーション(`for` / `collect` / indexing / `isempty`)を本家 Julia 1.12 と
pass/fail 一致でサポート済み(fixture
`reflection/methods_iteration_5125.jl`、`base/reflection.jl` の
`methods` → `_methods_by_ftype` 経由で VM メソッド表から `Method` 風 struct の
`Vector{Any}` を構築)。以下は範囲外(deferred):

- `typeof(methods(f))` は本家では `Base.MethodList` だが sjulia は `Vector{Any}`
  を返す。専用 `MethodList` ラッパ型と本家の集計 `show`
  (`# N methods for generic function "f"` ヘッダ付き)は未実装。
- 個々の `Method` の `show` 表示は簡易形式(`f(::Int64, ...)`)で、本家の
  file/line 付き `f(x::Int64) @ Main path/to/file.jl:N` 形式は未実装
  (VM はソース span を `Method` に保持しないため)。非可搬な show 文字列は
  fixture でアサートしない方針。
- Base / builtin 関数(`+`, `length` など)の `methods` 列挙は
  `create_builtin_method_structs` の代表メソッドに限定され、本家の全
  オーバーロード集合とは一致しない。

### builtins_reflection primitives Array helpers remaining scope (Issue #3908)

2026-05-25: `vm/builtins_reflection/primitives.rs` の `extract_types_from_value`
Vector-of-types 分岐を新しい file-local `legacy_array_value_ref` 経由のガード
アームに集約し、テスト側 3 箇所の `Value::Array(new_array_ref(arr))` を
`array_value` ヘルパに統合した。allowlist ceiling は 4 から 2 に下がり、残る
`Value::Array` 言及は両ヘルパ本体のみ。次の縮小は legacy native Array carrier
そのものを `extract_types_from_value` のディスパッチ表面から完全に除去し、
Pure Julia `Array{T,N}` wrapper 経由でのみ `methods(f, [T1, T2])` 経路を
受け入れるよう変えることである (Issue #4019 系の broadcast/HOF ヘルパー
群と歩調を合わせる)。

### Subtypes any_vector helper remaining scope (Issue #3908)

2026-05-24: `vm/builtins_types.rs` の `Subtypes` builtin は empty-result と
populated-result の両分岐を共有の Memory-first `any_vector_array_value`
helper 経由で構築し、`Value::Array` 直接構築を 1 箇所に集約した。
`subset_julia_vm_vm/src/vm/builtins_types.rs` の allowlist 上限は 8 から 7 に
下げた。残りは `TypeOf`/`Isa`/`Sizeof`/`Ismutable`/`Objectid`/`In` などの
Array 判別 arm を Pure Julia wrapper dispatch や共通 trait helper へさらに
寄せ、native `Value::Array` discriminant branch を順次縮小することである。

### array_index IndexStore dispatch helper remaining scope (Issue #3908)

2026-05-24: `vm/exec/array_index.rs` の IndexStore Tuple-value /
Array-element / boxed scalar の 3 ブランチを `array_ref_from_value` ヘルパに
集約し、SubArray parent unwrap 2 箇所を `sub_array_parent_array_ref` に
集約した。allowlist ceiling は 16 から 13 に下がった。
2026-05-25 (round 3): 同じ `array_ref_from_value` ヘルパを IndexLoad の
target dispatch、IndexStore の `is_complex_val` ブランチ、scalar IndexStore
の f64/i64 ブランチへ拡張し、3 つの追加 destructure を集約した。
allowlist ceiling は 13 から 10 に下がった。残る 10 件は wrapper
`_mem::Array` 経由の論理読み出し (`array_wrapper_logical_values`)、
論理 boolean/integer インデックス読み込み (IndexLoadTyped / IndexLoad の
`match index_val` / `match self.stack.pop()` 計 4 か所)、Generator
underlying iter の `match g.iter.as_ref()` Array arm、そして
`array_value` / `array_ref_from_value` / `sub_array_parent_array_ref` の
ヘルパ本体である。これらは Phase 3/4 (`wrap(Array, Memory, dims)` /
public reshape/view の Pure Julia 化) で完全に解消する予定。

### builtins_equality Array isequal/hash remaining scope (Issue #3908)

2026-05-24: `vm/builtins_equality.rs` の `Isequal` / `Hash` / `_Hash` から
`Value::Array` 直接マッチを除去し、`try_isequal_array_like` /
`try_hash_array_like` を経由する形に統一した。allowlist ceiling は 6 から 3
に下がった。残りは `Egal` の参照同一性比較 (`std::ptr::eq`) と、ヘルパ内に
残る 2 つの `Value::Array(_)` 分岐 (Array vs Memory 論理ビューと
Hash の Array 分岐) を Pure Julia Array wrapper 経由のディスパッチに
さらに寄せ、native `Value::Array` の destructure 自体をなくすことである。

### formatting display boundary remaining scope (Issue #3908)

2026-05-24: `vm/formatting.rs` の Julia-source 表現サーフェス
(`value_to_julia_code`) は新規の `legacy_array_value_ref` ヘルパ経由でガード化
され、フォーマッティングのユニットテストは `array_value(ArrayValue) -> Value`
ヘルパに集約された。allowlist ceiling は 6 から 5 に下がった。残るのは
`format_value_slow` (`print`) と `value_to_string` (`string(x)`) の
exhaustive match における Array アーム本体で、Rust の網羅性チェックが
ガード付きヘルパ呼び出しを `Value::Array(_)` のカバーと認めないため、当面は
直接マッチを維持する。最終的にはこれらの表示サーフェス自体を Pure Julia の
`show` / `print` メソッド経由へ寄せ、native Array アームを縮小することである。

### dynamic_ops mod array-like guards remaining scope (Issue #3908)

2026-05-24: `vm/dynamic_ops/mod.rs` の `dynamic_add` / `dynamic_sub` /
`dynamic_mul` / `dynamic_div` array-arm は、ファイル内 predicate
`is_array_like_value(&Value) -> bool` を経由した match-guard に集約され、
allowlist ceiling は 6 から 2 に下がった。残る 2 件は `dynamic_array_value`
の `Value::Array(new_array_ref(arr))` コンストラクタと、predicate 内の
`matches!(value, Value::Array(_) | Value::Memory(_))` であり、いずれも
Phase 3 (`wrap(Array, Memory, dims)` の Pure Julia 化) と
Phase 4 (`Value::Array` の境界 → Pure Julia wrapper 化) で完全に解消する
予定。当面はディスパッチ高速パスの順序を保つために残置する。

### array_mutate native-Array re-push remaining scope (Issue #3908)

2026-05-24: `vm/exec/array_mutate.rs` の Zero / ArrayPush / ArrayPop /
ArrayPushFirst / ArrayPopFirst / ArrayInsert / ArrayDeleteAt は、`Value::Array`
の構築をファイル内ヘルパ `push_array_ref` / `push_array_value` に集約し、
allowlist ceiling は 12 から 5 に下がった。残りは ArrayPush の Set 経路や
任意 push! の動的メソッドディスパッチを含む 3 つの Array-arm パターンを Pure
Julia wrapper / dispatch へ寄せ、native `Value::Array` の destructure 自体を
さらに縮小することである。`array_mutation_target` の classification arm と
re-push ヘルパ本体は Memory バリアントとの統一的な MethodError 報告のため
当面そのまま残る。

### binary_both Memory<->Array boundary remaining scope (Issue #3908)

2026-05-24: `vm/exec/binary_both.rs` の Memory<->Array 等価判定は
`MemoryValue::get` (1-indexed public boundary) 経由になり、matmul / scalar-array
fallback の guard-only `Value::Array` パターンは `is_legacy_array_value`
predicate に集約された。round 2 では更に Complex スカラー × 配列 / Real
スカラー × 配列 / Array × Array matmul の `Value::Array(a)` 直接マッチが
`legacy_array_ref_from_value(&Value) -> Option<&ArrayRef>` ヘルパに集約され、
`Value::Array` allowlist ceiling は 19 → 13 → 9 まで下がった。新規の
`scripts/check_array_public_data_access.sh` 規則が `memory.data.get_value`
への退行を防いでいる。残りは scalar-array / Array-Array matmul の結果書き戻し
(`Value::Array(new_array_ref(result))`) と Memory<->Array 等価ブリッジアームを
Pure Julia wrapper / Memory-first dispatch へさらに寄せ、native `Value::Array`
の構築・分解箇所を縮小することである。

### Dynamic op Array storage remaining scope (Issue #3908)

2026-05-24: retained dynamic arithmetic dispatch no longer matches
`ArrayData::StructRefs` / `ArrayData::Any` directly in `dynamic_ops/dispatch.rs`;
the storage classification is owned by `ArrayValue::supports_inline_dynamic_storage()`.
残りは Array arithmetic の public behavior を Pure Julia broadcast / arithmetic
dispatch へさらに寄せ、inline native Array fast path 自体を縮小することである。

### Plots SVG axis remaining scope (Issue #4437)

2026-05-24: generated SVG plot artifacts now render default x/y axis lines.
Remaining upstream Plots scope includes ticks, tick labels, grids, axis labels,
framestyle variants, and user-configurable axis visibility.

### Expr splat remaining scope (Issue #3908)

2026-05-24: `Expr` constructor の Array splat expansion は
`ArrayData::get_value` ではなく `ArrayValue::get_linear()` で logical element
を読むようになり、`Expr(:call, args...)` の splat lowering bug も Issue #4435
として修正した。残りは metaprogramming constructor splat の public behavior
を Pure Julia wrapper / iterator dispatch へさらに寄せ、native `Value::Array`
compatibility branch を削減することである。

### Plots plot! remaining scope (Issue #4410)

2026-05-24: embedded Plots now supports `plot!(f::Function)`,
`plot!(y::Vector)`, and `plot!(x, y)` by mutating the current series set and
returning the updated plot. Remaining upstream Plots scope includes explicit
target-plot forms, keyword attributes, and the wider recipe pipeline.

### NewStructSplat remaining scope (Issue #3908)

2026-05-24: `NewStructSplat` の Array argument expansion は
`ArrayData::get_value` ではなく `ArrayValue::get_linear()` で logical element
を読むようになった。残りは struct construction splat の public behavior を
Pure Julia wrapper / iterator dispatch へさらに寄せ、native `Value::Array`
compatibility branch を削減することである。

### IndexStoreTyped remaining scope (Issue #3908)

2026-05-24: typed `setindex!` struct-array classification は
`ArrayData::StructRefs` を直接 match せず、
`ArrayValue::is_struct_ref_array()` を使うようになった。残りは typed
indexing mutation fallback 自体を Pure Julia wrapper / Memory-first dispatch
へさらに寄せ、native `Value::Array` branch を縮小することである。

### Array push remaining scope (Issue #3908)

2026-05-24: native `push!` Array builtin は `ArrayData::StructRefs` を直接
mutate せず、`ArrayValue::push()` に委譲するようになった。残りは public
Array mutation behavior を Pure Julia wrapper dispatch へさらに寄せ、native
`Value::Array` mutation fallback 自体を縮小することである。

### Generator Array indexing remaining scope (Issue #3908)

2026-05-24: retained `Generator` indexing fallback over Array inputs は
`ArrayData::get_value` ではなく `ArrayValue::get_linear()` で logical element
を読むようになった。残りは Generator / collect / indexing public behavior を
Pure Julia iterator dispatch へさらに寄せ、native `Value::Array` compatibility
branch を削減することである。

### typed array literal remaining scope (Issue #3908)

2026-05-24: `PushElemTyped` は `ArrayData::StructRefs` を直接 match せず、
`ArrayValue::is_struct_ref_array()` と `ArrayValue::push()` に委譲するように
なった。残りは typed/untyped array literal builder がまだ transitional
`Value::Array` を返している点を Pure Julia Array wrapper / Memory primitive
boundary へさらに寄せることである。

### Array/Memory migration remaining scope inventory (Issue #3908)

2026-05-24: `docs/vm/ARRAY_MEMORY_MIGRATION.md` の current inventory を
現在の audit count に更新した。`memory_to_array_ref` は 0 件で audit により
retired 状態を維持している。array_index logical-load helper routing と
array_mutate re-push helper cleanup の後で `Value::Array` references は
219 件まで縮小したが、残りと 93 files touching Array representation surfaces
のうち、public behavior を Pure Julia wrapper / Memory primitive dispatch へ
移せる箇所をさらに削ることである。

### array_index Value::Array allowlist remaining scope (Issue #3908)

2026-05-24: the `vm/exec/array_index.rs` `Value::Array` allowlist ceiling was
lowered from 22 to the current 16 references by routing
`load_selected_array_elements` through `ArrayValue::get_linear` and
centralizing the IndexStore re-push sites behind a single `array_value`
constructor helper. 残りは generic indexing, multi-dimensional indexing,
SubArray parent extraction, and wrapper index-vector paths を Pure Julia /
Memory-first dispatch へさらに寄せ、native `Value::Array` compatibility
branches を削減することである。

### builtins_exec Value::Array allowlist remaining scope (Issue #3908)

2026-05-24: `builtins_exec.rs` was removed from
`scripts/check_value_array_allowlist.sh` after reaching zero `Value::Array` /
`ArrayData` / `ArrayValue` references. 残りは other classified files の native
Array compatibility branches を同じ方式で縮小し、ゼロになった file を allowlist
から外していくことである。

### Array first/last fallback remaining scope (Issue #3908)

2026-05-24: `Value::Array` branches were removed from the retained internal
`BuiltinId::TupleFirst` / `BuiltinId::TupleLast` fallback in `builtins_exec.rs`;
public `first(::Array)` / `last(::Array)` stay on Pure Julia indexing. 残りは
other public Array access helpers を Pure Julia wrapper dispatch へさらに寄せ、
native `Value::Array` compatibility branches を縮小することである。

### Array reshape/mutation fallback remaining scope (Issue #3908)

2026-05-24: unreachable legacy `BuiltinId::Reshape` / `BuiltinId::Push` /
`BuiltinId::Pop` fallback arms were removed from `builtins_exec.rs`; active
ownership remains in `builtins_arrays.rs`. 残りは these public Array reshape and
mutation behaviors を Pure Julia wrapper dispatch へさらに寄せ、native
`Value::Array` compatibility branches を縮小することである。

### Deepcopy fallback remaining scope (Issue #3908)

2026-05-24: unreachable legacy `BuiltinId::Deepcopy` fallback arm was removed
from `builtins_exec.rs`; active ownership remains in `builtins_reflection`,
including the Memory-first logical Array copy helper. 残りは `deepcopy` public
behavior を Pure Julia wrapper dispatch へさらに寄せ、native `Value::Array`
return boundary 自体を縮小することである。

### String fallback remaining scope (Issue #3908)

2026-05-24: unreachable legacy string builtin fallback arms were removed from
`builtins_exec.rs`; active ownership remains in `builtins_strings.rs`, including
the Memory-first `codeunits(::String)` handler. 残りは string/array conversion
and string indexing public behavior を Pure Julia wrapper dispatch へさらに寄せ、
native `Value::Array` compatibility branches を縮小することである。

### Type fallback remaining scope (Issue #3908)

2026-05-24: unreachable legacy `BuiltinId::TypeOf` / `BuiltinId::Isa` /
`BuiltinId::Subtype` fallback arms were removed from `builtins_exec.rs`;
active ownership remains in `builtins_types.rs`, including the centralized
Array element-type projection path. 残りは public Array reflection and type
identity behavior を Pure Julia wrapper dispatch / runtime type-object identity
へさらに寄せ、native `Value::Array` compatibility projection branch 自体を
縮小することである。

### Zeros/ones fallback remaining scope (Issue #3908)

2026-05-24: unreachable legacy `BuiltinId::Zeros` / `BuiltinId::Ones` fallback
arms were removed from `builtins_exec.rs`; active constructor ownership remains
in `builtins_arrays.rs` on Memory-first helpers. 残りは #4018 とあわせて public
Array allocation behavior を Pure Julia wrapper dispatch へさらに寄せ、native
constructor fallback surface 全体を縮小することである。

### zero Array fallback remaining scope (Issue #4419)

2026-05-24: retained native `zero(::Array)` fallback now derives result storage
from logical `ArrayValue::element_type()` and Memory-first typed allocation,
preserving `Vector{Int64}` and `Vector{Bool}` instead of returning
`Vector{Float64}` for all arrays. 残りは #3908 とあわせて public
`zero(::Array)` behavior を Pure Julia wrapper dispatch へ寄せ、native
`Value::Array` fallback branch 自体を compatibility path から削ることである。

### Array literal PushElem remaining scope (Issue #3908)

2026-05-24: retained untyped array literal `PushElem` execution now grows the
builder through `ArrayValue::push_f64()` instead of direct raw `ArrayData::F64`
mutation, and the literal Memory-first audit covers this boundary. 残りは
array literal public construction を Pure Julia Array wrapper / MemoryRef
semantics へさらに寄せ、legacy `NewArray` / `PushElem` / `FinalizeArray`
builder instructions 自体を compatibility path から削ることである。

### Any array trait projection remaining scope (Issue #4416)

2026-05-24: public `eltype(::Array)`, `valtype(::Array)`, and native
`typeof(::Array)` now preserve declared `ArrayElementType::Any` metadata instead
of peeking at runtime user-struct elements. 残りは #3908 とあわせて these native
trait/reflection compatibility branches を Pure Julia wrapper dispatch /
type-object identity へ寄せることである。

### Array type trait projection remaining scope (Issue #3908)

2026-05-24: `eltype(::Array)`, `valtype(::Array)`, dispatch-facing
`Vm::get_value_julia_type`, and native `typeof(::Array)` now use centralized
ArrayValue element-type projection helpers, preserving user-struct array element
metadata without over-projecting `Vector{Any}`. 残りは these public trait and
reflection paths を Pure Julia wrapper dispatch / type-object identity へさらに寄せ、
native `Value::Array` compatibility projection branch 自体を削ることである。

### Slice index array remaining scope (Issue #3908)

2026-05-24: Pure Julia `Array` wrapper は Int64 index vector / Bool mask
`getindex` を扱い、retained native slice fallback も `ArrayValue::get_linear()`
経由で logical Array elements を読む。残りは generic indexing public behavior
を `to_indices` dispatch へさらに寄せ、native slice fallback branch 自体を
compatibility path から削ることである。

### String from Array remaining scope (Issue #3908)

2026-05-24: `String(::Array)` character conversion now reads logical Array
elements through `ArrayValue::get_linear()` instead of raw storage variants,
including the retained Pure Julia Array wrapper `_mem::Array` compatibility
boundary. 残りは string/array conversion public behavior を Pure Julia wrapper
dispatch へ移し、native `Value::Array` fallback branch 自体を compatibility
path から削ることである。

### Matmul result remaining scope (Issue #3908)

2026-05-24: retained real-valued matmul and scalar-vector compatibility kernels
now materialize result arrays through Memory-first `ArrayValue` helpers.
public LinearAlgebra behavior は #4020 で Pure Julia / stdlib dispatch-first
path へ移行済み。残りは #4568 とあわせて native `ArrayValue` kernel
boundary 自体を compatibility path から削ることである。

### HOF input normalization remaining scope (Issue #3908)

2026-05-24: `pop_array_or_values` now rebuilds logical numeric HOF fallback
inputs through Memory-first `ArrayValue` helpers, and the audit covers that
input normalization boundary. 残りは HOF input normalization itself を Pure
Julia dispatch / iterator traits へさらに寄せ、native HOF fallback entry を
compatibility path から削ることである。

### HOF dispatch result array remaining scope (Issue #3908)

2026-05-24: F64-mode HOF dispatch result builders now allocate Broadcast and
FindAll outputs through Memory-first `ArrayValue` helpers, and the audit covers
that retained compatibility file. 残りは HOF/broadcast public behavior を Pure
Julia dispatch / iterator traits へさらに寄せ、native HOF result
materialization boundary を compatibility path から削ることである。

### RNG/range VM array remaining scope (Issue #3908)

2026-05-24: retained RNG array instructions and legacy eager range
instructions now allocate result storage through Memory-first `ArrayValue`
helpers before returning transitional `Value::Array` wrappers. 残りは random
array and range public behavior を Pure Julia / stdlib dispatch surfaces へ
さらに寄せ、old VM materialization instructions を compatibility path から削る
ことである。

### String index vector remaining scope (Issue #3908)

2026-05-24: String `getindex` compilation now treats Array-like index
arguments as slice inputs instead of scalar `DynamicToI64`, enabling
`s[[i, j]]` through `IndexSlice`; runtime String slicing now selects each
integer vector index independently rather than a continuous range. 残りは
string/array indexing behavior を Pure Julia wrapper / generic `getindex`
dispatch へさらに寄せ、native slice fallback を compatibility path から削ることである。

### Array iteration/in remaining scope (Issue #3908)

2026-05-24: Array iteration next-state handling and native `in(x, array)`
fallback now read elements through logical `ArrayValue::get_linear()` instead
of raw storage, preserving Complex array membership semantics. 残りは Array
iteration and `in` public behavior を Pure Julia Array wrapper / MemoryRef
iteration へさらに寄せ、native `Value::Array` fallback branch 自体を
compatibility path から削ることである。

2026-05-24 (follow-up): `vm/type_ops/iteration.rs` の EachCol / EachRow /
EachSlice ハンドラは `matrix_array_dims_2d` / `extract_matrix_row_1based` /
`extract_matrix_column_1based` という file-local ヘルパに raw Array マッチを
集約し、ceiling は 32 → 21 に縮小した。残りは collect / generator の
Memory-first 結果構築、`iterate_first` / `iterate_next` の Array 本体スキャン、
`collect_iterator` のシャロー型保存コピー、Array wrapper `_mem::Array` 移行
ブリッジを Pure Julia / MemoryRef iteration に寄せて、native `Value::Array`
fallback branch 自体を compatibility path から削ることである。

2026-05-24 (follow-up): `vm/exec/array_basic.rs` の `NewArray` /
`PushArrayValue` / `NewArrayTyped` / `LoadArray` ハンドラは
`push_array_ref` / `push_array_value` / `push_typed_array_value` の 3 つの
file-local ヘルパに native-Array push 構築を集約し、Memory-first リテラル
ビルダと per-frame `LoadArray` 再 push 経路がすべて 1 行の境界を通るように
した。ceiling は 19 → 10 に縮小。残りの 10 件は `Some(Value::Array(arr)) =>`
/ `if let Some(Value::Array(arr)) = ...` のパターンマッチとヘルパ定義自身で
あり、Pure Julia Array wrapper dispatch が `LoadArray` / `StoreArray` を
完全に置き換えるまでの compatibility boundary として残る。

2026-05-25 (follow-up, round 2): `vm/exec/array_basic.rs` の destructure
側 9 か所 (`PushElem` / `FinalizeArray` / `PushElemTyped` /
`FinalizeArrayTyped` の `match self.stack.last_mut()` 4 か所、`LoadArray`
の current-frame と global-frame の `if let Some(Value::Array(arr)) = ...` 4
か所、`StoreArray` の `match val { Value::Array(arr) => ... }`) も 3 つの
file-local helper (`legacy_array_value_mut_ref`、`legacy_array_value_into`、
`try_consume_array_value`) に集約。ceiling は 10 → 4 に縮小。残りの 4 件は
`push_array_ref` re-push helper 本体と 3 つの destructure helper 本体のみで、
すべての公開ハンドラは file-local helper 1 行で Pure Julia Array carrier の
境界を通過する。残作業は Pure Julia Array wrapper dispatch が
`LoadArray` / `StoreArray` を完全に置き換えてから、ヘルパ自身を
撤去することである。

### Array sizeof remaining scope (Issue #3908)

2026-05-24: native `sizeof(::Array)` fallback now projects element bytes from
logical `ArrayValue::element_type()` instead of raw storage tags, preserving
Complex array byte-size semantics. 残りは `sizeof` public behavior を Pure
Julia wrapper dispatch / type-layout metadata へさらに寄せ、native
`Value::Array` fallback branch 自体を compatibility path から削ることである。

### IndexStore scalar conversion remaining scope (Issue #3908)

2026-05-24: numeric `IndexStore` scalar conversion now reads logical
`ArrayValue::element_type()` instead of raw storage tags for direct and
SubArray parent stores. 残りは public `setindex!` behavior を Pure Julia
wrapper / MemoryRef mutation path へさらに寄せ、native `Value::Array`
mutation fallback を compatibility path から削ることである。

### Array isa remaining scope (Issue #3908)

2026-05-24: native `isa(::Array, ::Type)` fallback now uses the shared
dispatch-facing logical Array type projection instead of raw `ArrayData` tags.
残りは `isa` / dispatch behavior を Pure Julia wrapper and type-object identity
paths へさらに寄せ、native `Value::Array` fallback branch 自体を compatibility
path から削ることである。

### Array runtime_type remaining scope (Issue #3908)

2026-05-24: `Value::runtime_type(::Array)` now uses the shared logical
`ArrayElementType` projection helper, including nested tuple field types.
残りは `runtime_type` callers that still depend on transitional `Value::Array`
objects を Pure Julia wrapper / Memory primitive boundaries へ寄せることである。

### Array dispatch type projection remaining scope (Issue #3908)

2026-05-24: runtime dispatch-facing `Value::Array` type projection now uses
`ArrayValue::element_type()` for logical element metadata such as
`Complex{Float64}` instead of raw primitive storage tags. 残りは public Array
method dispatch を Pure Julia wrapper / MemoryRef semantics へさらに寄せ、
native `Value::Array` projection branch 自体を compatibility path から削ることである。

### Array valtype remaining scope (Issue #3908)

2026-05-24: `valtype(::Array)` now projects through
`ArrayValue::element_type()` instead of raw `ArrayData` tags, preserving logical
types such as `Complex{Float64}` at the collection trait boundary. 残りは
`valtype` / `eltype` public behavior を Pure Julia wrapper dispatch へさらに寄せ、
native `Value::Array` trait fallback を compatibility path から削ることである。

### LinearAlgebra fallback input remaining scope (Issue #3908)

2026-05-24: LinearAlgebra native compatibility kernels now read numeric Array
inputs through `ArrayValue::to_logical_f64_vec()`. public LinearAlgebra
behavior は #4020 で Pure Julia / stdlib dispatch-first path へ移行済み。残りは
#4568 とあわせて native ArrayValue kernel boundary を compatibility path
から削ることである。

### HOF F64 fallback start remaining scope (Issue #3908)

2026-05-24: retained VM HOF F64 fallback entry points now read source arrays
through `ArrayValue::to_logical_f64_vec()` instead of raw-storage
`try_as_f64_vec()`. 残りは #4019 とあわせて these HOF fallbacks を Pure Julia
dispatch に置き換え、native F64 HOF boundary を compatibility path から削ることである。

### HOF array popping remaining scope (Issue #3908)

2026-05-24: `pop_array_or_values` now reads non-F64 / shared storage through
logical ArrayValue accessors instead of matching raw `ArrayData` variants.
残りは #4019 とあわせて predicate HOF public behavior を Pure Julia dispatch
へさらに移し、legacy F64 HOF fallback を compatibility path から削ることである。

### filter! HOF fallback remaining scope (Issue #3908)

2026-05-24: the retained VM `FilterInPlace` compatibility fallback now rebuilds
filtered storage through `ArrayValue::memory_first_from_f64()` instead of
assigning raw `ArrayData::F64`. Public `filter!(f, a::Array)` is still intended
to stay on the Pure Julia method path; 残りは #4019 とあわせて native HOF
fallback boundary 自体を compatibility path から削ることである。

### Range count remaining scope (Issue #3908)

2026-05-24: `count(f, r::AbstractRange)` now runs through Pure Julia iteration,
and the retained VM `CountFunc` Range compatibility fallback delegates
materialization to `RangeValue::collect()`. 残りは #4568 とあわせて
predicate HOF / Range collect public behavior をさらに Pure Julia dispatch へ移し、
native HOF fallback boundary を compatibility path から削ることである。

### Range pop_array materialization remaining scope (Issue #3908)

2026-05-24: `StackOps::pop_array` now delegates `Value::Range`
auto-collection to `RangeValue::collect()`, preserving Memory-first storage and
integer range element types. VM-native Range collect boundary は #4266 で
trait-shaped public `collect` dispatch へ縮小済み。残りは #4568 とあわせて
final native fallback を削ることである。

### Matmul helper remaining scope (Issue #3908)

2026-05-24: matmul helper extraction now reads logical real and complex elements
via `ArrayValue::to_logical_f64_vec` / `get_linear`, preserving reshaped/shared
parent projection. LinearAlgebra public behavior は #4020 で Pure Julia /
stdlib dispatch-first path へ移行済み。残りは #4568 とあわせて native
`ArrayValue` kernel boundary を compatibility path から削ることである。

### Array index-array extraction remaining scope (Issue #3908)

2026-05-24: runtime array-valued index extraction now reads logical elements via
`ArrayValue::get_linear`, preserving reshaped/shared parent projection for Bool,
F64 boolean-like, and I64 index-array modes. 残りは array indexing public
behavior を Pure Julia wrapper dispatch / `MemoryRef` semantics へ移し、native
`Value::Array` indexing branches を compatibility boundary から削ることである。

### Array deep copy remaining scope (Issue #3908)

2026-05-24: `deep_copy_value(::Array)` now copies logical array elements through
`ArrayValue::memory_first_copy_from_array` and returns independent
Memory-backed transitional storage. 残りは `deepcopy` public behavior を Pure
Julia wrapper dispatch へ移し、native `Value::Array` branch を compatibility
boundary から削ることである。

### Dynamic complex broadcast bridge remaining scope (Issue #3908)

2026-05-24: dynamic broadcast's Complex struct-ref array bridge now uses the
Memory-first `ArrayValue::complex_f64` helper for interleaved storage
materialization. 残りは broadcast public behavior 自体を #4019 とあわせて Pure
Julia wrapper dispatch へ移し、この bridge を native `ArrayValue` compatibility
boundary から削ることである。

### ArrayValue legacy constructor remaining scope (Issue #3908)

2026-05-24: `ArrayValue::new` and `with_struct_type` now construct storage
through Memory-first helpers while preserving the legacy public constructor API
and struct-reference logical type metadata. 残りは direct `ArrayValue` public
fallback builders and call sites that still rely on native Array semantics を
Pure Julia `Array` wrapper / `MemoryRef` projectionへ縮小していくことである。

### Tuple and isbits ArrayValue helper remaining scope (Issue #3908)

2026-05-24: tuple and isbits struct `ArrayValue` helper constructors now build
through Memory-first materialization / capacity helpers while preserving logical
element tags. 残りは these helpers' callers を native `Value::Array` public
behavior から Pure Julia wrapper dispatch へ移し、inline tuple/struct array
semantics を Array wrapper / MemoryRef boundary 上で表現することである。

### Primitive ArrayValue helper remaining scope (Issue #3908)

2026-05-24: primitive `ArrayValue` helper constructors now build storage through
`MemoryValue` before returning the transitional wrapper, including `zeros`,
`ones`, `fill`, and primitive / boxed `undef_typed` branches. 残りは helper の
返却先である native `Value::Array` public boundary を Pure Julia `Array`
wrapper / `MemoryRef` projectionに縮小し、旧 bytecode / host boundary 以外での
semantic owner 利用を消していくことである。

### Complex Array materialization remaining scope (Issue #3908)

2026-05-24: complex F64/F32 helper constructors and complex zeros/undef paths now
construct interleaved real storage through `MemoryValue` before returning the
transitional `ArrayValue` wrapper. 残りは complex array public behavior を
Pure Julia `Array` wrapper dispatch へ移し、LinearAlgebra / broadcast 側に残る
native `ArrayValue` kernel boundary を #4568 とあわせて縮小することである。

### String/Any materialization remaining scope (Issue #3908)

2026-05-24: `codeunits`, regex `split`, regex `eachmatch`, and `subtypes`
Vector construction now allocate transitional `ArrayValue` storage through
Memory-first helpers. `ArrayValue::any_vector` also constructs `MemoryValue`
storage first. 残りは public return boundary の `Value::Array` を Pure Julia
`Array` wrapper / `MemoryRef` projectionへ縮小し、同種の materialization path を
順次 Rust fallback から dispatch-first Base 側へ移すことである。

### Array builtin Value::Array boundary remaining scope (Issue #3908)

2026-05-24: `vm/builtins_arrays.rs` の `Value::Array` audit ceiling を 9 から
3 まで縮小し、public Array builtin boundary の再拡大を検出できるようにした。
Similar / Reshape / Size / Ndims / Keytype / Valtype のクエリ・構築ハンドラを
ファイルローカルな `value_as_array_ref` ヘルパへ集約済みで、残るは集約済み
ヘルパ定義 (`value_as_array_ref` / `push_array_ref` /
`pop_array_ref_for_builtin`) のみ。これらは同ファイル内 classified native
Array boundary であり、Memory primitive または Pure Julia Array wrapper
dispatch への次フェーズ移行で削減していく。

### Scalar backslash fallback remaining scope (Issue #4353)

2026-05-24: unrelated user `Base.:\` methods no longer make unmatched scalar
`a \ b` calls fall into the retained LinearAlgebra `Ldiv` builtin. Direct scalar
and `Any` wrapper scalar cases now preserve Julia's `b / a` fallback. 追加の
未実装スコープは残していない。広い残課題は #4568 の final
`Value::Array` removal で継続する。

## 最新対応 (2026-05-23)

### Union vararg string multiplication remaining scope (Issue #4350)

2026-05-23: `Base.:*` for `String` / `Char` concatenation now uses the
upstream-shaped `Union{AbstractChar, AbstractString}` vararg method signature.
Representative binary and n-ary dispatch, including
`hasmethod(*, Tuple{String, Char, String})`, matches upstream Julia. 追加の
未実装スコープは残していない。広い残課題は AnnotatedString support を追加した時点で、
upstream と同じ annotated branch を Pure Julia 側に戻すことである。

## 最新対応 (2026-05-22)

### Union vararg string multiplication remaining scope (Issue #4350)

2026-05-22: binary `String` / `Char` multiplication became available through
explicit Pure Julia methods and slice-indexed `*` reached string dispatch
instead of `MatMul`. This interim remaining scope was closed by the
2026-05-23 upstream-shaped Union vararg method implementation.

### isvalid dispatch-first remaining scope (Issue #4352)

2026-05-22: `isvalid` は dispatch-first public fallback に戻り、代表的な direct
route bypass は解消済み。追加の未実装スコープは残していない。

### Peephole compare-jump fusion remaining scope (Issue #4351)

2026-05-22: compare+jump fusion は branch target boundary を跨がなくなり、短絡 OR
代表 failure は解消済み。残りは basic-block aware な peephole pass として制御フロー情報を
明示的に持つ設計へ整理することである。

### Narrow integer fused slot remaining scope (Issue #4349)

2026-05-22: `LoadAddI64Slot` / `LoadSubI64Slot` / `LoadMulI64Slot` /
`LoadModI64Slot` は narrow signed/unsigned integer slot を保持できるようになった。
代表 failure は解消済み。残りは fused slot opcode 名と型表現を `I64` 固定から
より一般的な integer slot operation へ整理することである。

### Membership alias remaining scope (Issue #3911)

2026-05-22: `∉` / `∋` / `∌` は、alias 自体の specific user method がない場合に
call-site で `in` へ直接委譲するようになり、後から定義された `Base.in` 拡張を
見落とさなくなった。残りは Base alias wrapper の事前コンパイルでも world-age /
method invalidation 相当の再解決を表現できるようにすることである。

### Module struct DataType identity remaining scope (Issue #4348)

2026-05-22: imported module struct aliases now compare correctly for `DataType`
identity, so `typeof(s) === MyStruct{Int64}` matches upstream Julia in the
representative `module_struct_isa` fixture. 追加の未実装スコープは残していない。
広い残課題は DataType object identity を文字列正規化ではなく registry identity に寄せることである。

### IOBuffer/empty Tuple show remaining scope (Issue #4347)

2026-05-22: `show(buf, ())` の代表 failure は解消済み。通常候補でも
runtime subtype fallback を使うようになり、`IOBuffer <: IO` / `Tuple{} <: Tuple`
で `show(::IO, ::Tuple)` に到達する。広い残課題は statement boundary で
`IOBuffer()` が `Any` に落ちる箇所を減らし、runtime dispatch 依存を縮小することである。

### Pair expression remaining scope (Issue #4346)

2026-05-22: `:a => 1` の代表表示 failure は解消済み。残りは Pair を本家 Julia と同じ
parametric `Pair{A,B}` として保持し、tuple-like fallback を縮小することである。

### gcdx remaining scope (Issue #4345)

2026-05-22: `gcdx` の integer recurrence Float64 leakage は解消済み。残りは
整数除算・剰余周辺のより広い本家互換 fixture 拡充で扱う。

### Runtime varargs specialization remaining scope (Issue #4344)

2026-05-22: varargs collector slot の代表 failure は解消済み。残りは varargs の要素型・長さを
runtime specialization により精密に持ち込むことである。

### @kwdef constructor remaining scope (Issue #4343)

2026-05-22: `@kwdef` default constructor fallback の代表 failure は解消済み。残りは
keyword constructor lowering を本家 Julia の lowering/constructor model にさらに近づけることである。

### iOS Random Simulation remaining scope (Issue #4342)

2026-05-22: iOS Random/Dice Simulation の Base `floor` shadowing 代表 failure は解消済み。
sample helper は `integer_floor` に改名済み。追加の未実装スコープは残していない。

### Complex function-value trig remaining scope (Issue #4341)

2026-05-22: `r = tan(z::Complex{Float64})` の代表 failure は解消済み。
Complex 引数の Pure Julia math call は top-level/global inference と function-value
intrinsic fallback の両方で Float64 固定にならない。追加の未実装スコープは残していない。
広い残課題は math function metadata を Pure Julia method inference に寄せ、個別の
Complex-return override を削減することである。

### Complex reshape remaining scope (Issue #4340)

2026-05-22: `reshape(Vector{Complex{Float64}}(undef, 4), 2, 2)` は
Pure Julia Array wrapper projection 経由でも `Matrix{Complex{Float64}}` /
`Complex{Float64}` を保つようになった。iOS Mandelbrot 2D broadcast sample の
代表 failure も解消済み。追加の未実装スコープは残していない。広い残課題は
Array wrapper / Memory primitive migration を進め、legacy native `ArrayValue` を
`_mem` として扱う bridge 自体を縮小することである。

### BigInt/BigFloat abstract numeric remaining scope (Issue #4337)

2026-05-22: `BigInt` / `BigFloat` の代表的な constructor inference、
`x::Number` / `x::Real` / `x::Integer` 抽象数値引数経由の VM dispatch、
caller result storage、および `sign(::BigInt)` の型保存は本家 Julia と揃った。
追加の未実装スコープは残していない。広い残課題は abstract numeric inference を
Julia 本家の lattice/union semantics にさらに近づけ、型付き slot と runtime dispatch の
境界を縮小することである。

### Broadcast logical indexing remaining scope (Issue #4338)

2026-05-22: `.>` などの materialized broadcast comparison は Boolean array として
推論され、logical indexing は代表 fixture で scalar Int64 index path に落ちなくなった。
追加の未実装スコープは残していない。残りは broadcast 全体の shape/element lattice、
lazy Broadcasted representation、および Array wrapper-only indexing path の拡張で扱う。

### Matrix rotation remaining scope (Issue #4336)

2026-05-22: `rotl90` / `rotr90` / `rot180` の VM-supported Matrix overload coverage は
追加済み。追加の未実装スコープは残していない。残りは Array wrapper/Memory primitive
boundary をさらに縮小し、本家 Julia の generic `AbstractMatrix` 実装へ寄せることである。

### REPL Array wrapper persistence remaining scope (Issue #4335)

2026-05-22: Pure Julia `Array{T}` wrapper backed by Memory / native Array storage is
REPL literal reinjection 対象になった。追加の未実装スコープは残していない。広い残課題は
REPL persistence の wrapper identity / shared backing semantics をより完全に保つことである。

### Reshape fallback audit remaining scope (Issue #4334)

2026-05-22: `reshape` は dispatch-first route かつ retained VM fallback として
reachability audit に再登録済み。追加の未実装スコープは残していない。広い残課題は
Array wrapper / Memory primitive boundary への移行で扱う。

### Array mutation dispatch remaining scope (Issue #4276)

2026-05-22: `push!`, `pop!`, `pushfirst!`, `popfirst!`, `insert!`, and
`deleteat!` now check runtime user-method candidates before falling back to VM
array mutation, and no longer let `Vector{Int64}` mutation extensions intercept
`Vector{Any}`. The representative `Vector{Any}` array mutation mismatch is
closed. Remaining work is broader: retire the retained native Array mutation
fallbacks themselves in favor of Pure Julia wrapper dispatch and Memory
primitive operations.

### CallDynamic Array boundary remaining scope (Issues #3908/#4276)

2026-05-22: `CallDynamic` の native Array rank/count inspection は 1 つの
helper に集約し、`vm/exec/call_dynamic.rs` の `Value::Array` allowlist ceiling
1 を再び満たすようになった。残りは `Value::Array` container 自体の退役、
generator/collect/IteratorSize の wrapper-only path への移行、および他 VM runtime
files の classified `Value::Array` boundary 削減である。
同日 follow-up で `vm/dynamic_ops/mod.rs` の dynamic array arithmetic result builder も
`dynamic_array_value` に集約し、同ファイルの classified `Value::Array` ceiling は 12 から
6 へ縮小した。残りは #4276 で dynamic array arithmetic fallback 自体を
Pure Julia dispatch-first / Memory primitive path へさらに寄せることである。
同日 follow-up で、既に縮小済みだった `vm/builtins_linalg.rs` の ceiling を 31 から 2 へ、
`vm/type_ops/iteration.rs` の ceiling を 33 から 31 へ下げ、LinearAlgebra / Generator /
Range の残境界が再拡大しないよう監査を締めた。残りは各ファイルの classified
`Value::Array` match 自体をさらに Pure Julia wrapper / Memory primitive boundary へ移すこと。
同日 follow-up で `vm/exec/call_function_variable.rs` の ceiling も 2 から 1 へ下げ、
callable-value dispatch の Array argument element-type projection だけを残した。残りはこの
type projection も wrapper/type metadata 経由へ移し、runtime callable path から native
`Value::Array` 参照をなくすことである。
同日 follow-up で `vm/builtins_arrays.rs` の native Array result push を helper に集約し、
ceiling を 25 から 10 へ下げた。残りは zeros/ones/undef/similar/reshape/mutation fallback
などの public Array allocation behavior 自体を、さらに Pure Julia wrapper dispatch と
Memory primitive boundary へ分離することである。

### Numeric constructor callable remaining scope (Issue #4316)

2026-05-22: 直接呼び出し、runtime `DataType` callable、tuple-splat
`Base.Generator` の代表ケースは本家 Julia と同じ `MethodError` / empty
`Vector{Union{}}` になった。追加の未実装スコープは残していない。広い残課題は
primitive type constructor 全体の Pure Julia 化と Rust fallback boundary 削減で扱う。

### LinearAlgebra fallback handoff (Issue #4020)

2026-05-22: rank-specific な `Matrix{T} <: AbstractMatrix` /
`Vector{T} <: AbstractVector` と、`Matrix{T} <: AbstractVector` を許さない代表的な
runtime subtype lattice は本家 Julia と揃った。`using LinearAlgebra` 後の user
`Base.:*` override order、`C * nullspace(C)` の matrix-matrix dispatch、および
`Vector{Complex{Float64}}` の specialized method selection も代表 fixture で確認済み。
`inv` / `svd` / `qr` / `eigen` / `eigvals` / `cholesky` / `cond` の retained direct
kernel route も、`import LinearAlgebra: ...` 後の user method を代表 fixture で先に選ぶ。
`A \ b` も user `Base.:\(A::Matrix{Float64}, b::Vector{Float64})` method を direct
call と `Any` wrapper の両方で VM `Ldiv` kernel より先に選ぶ。
matrix literal local は lowering shape metadata から `Matrix{T}` と推論されるようになり、
`mul!` / `ldiv!` / `rdiv!` / `tr` / `opnorm` / `diag` の代表 direct call で
`Matrix{Float64}` user method が VM/native fallback より先に選ばれる。
`reshape` が返す Pure Julia `Array` wrapper も LinearAlgebra kernel 入り口で代表的に
扱えるため、`rank(reshape(v, 1, n))` の `svd` 経路は `StructRef` error なしで通る。
`Matrix{<:Integer}` / `Matrix{<:AbstractFloat}` の bounded parametric Matrix alias も、
unqualified call と `LinearAlgebra.det` / `LinearAlgebra.lu` の両方で本家 Julia と同じ
代表 dispatch 結果になった。2026-05-27 に current checkout で再検証し、
public LinearAlgebra dispatch-first scope は #4020 として closed。残る VM primitive
representation bridge / final native Array carrier cleanup は #4568 へ渡す。

## 最新対応 (2026-05-21)

### LinearAlgebra fallback handoff (Issue #4020)

2026-05-21: `det` / `lu` は public route registry で `DispatchFirst` になり、
user `det(::Array)` / `lu(::Array)` method は unqualified call と
`LinearAlgebra.det` / `LinearAlgebra.lu` の両方で Rust fallback より先に選ばれる。
後続の 2026-05-22 / 2026-05-27 slices で parametric alias signature、
LinearAlgebra `Value::Array` special-case 削減、Pure Julia wrapper dispatch-first
coverage を閉じた。残る VM primitive representation bridge / final native Array
carrier cleanup は #4568 へ渡す。

### Diagonal size residual scope (Issue #4314)

2026-05-21: `size(::Diagonal, dim)` の代表 VM runtime error は修正済み。
残りは根本原因である `if a || b` 形式の短絡条件コンパイルを CoreCompiler/VM 側で
直すこと、および `isa(::LinearAlgebra.Diagonal, Diagonal)` の unqualified exported type
alias parity を改善することである。

### Bugfix residual scope notes (Issues #4311, #4312, #4313)

2026-05-21: `resize!(Vector{Bool})` の grow path、tuple equality の数値同値比較、
および local callable callee dispatch の代表不具合は修正済み。これらの Issue には
追加の未実装スコープは残していない。関連する広い課題は、branch join のさらなる精度向上、
equality fallback 全体の dispatch-first 化、および callable object/closure semantics の
未完了領域として既存の型推論・dispatch 追跡 Issue で扱う。

### Keyword default preservation remaining scope (Issue #4297)

2026-05-21: quoted Symbol keyword defaults like `debuginfo=:default` are now
pre-evaluated as `Value::Symbol` and typed as `ValueType::Symbol`, so omitted
and explicit Symbol keyword values match upstream representative behavior.
Bool keyword defaults are also stored as `Value::Bool` / `ValueType::Bool`.
Same-day follow-up stores optional kw default IR and evaluates supported omitted
defaults left-to-right, so arithmetic defaults like `x=1+2` and dependent
defaults like `b=a+1` see earlier omitted or explicit keyword values. Global
binding defaults such as `debuginfo=x4297` are also read from the call-time
global frame. Simple zero-argument calls in defaults are also supported for
representative cases such as `x=default_num()+1` and
`debuginfo=default_sym()`. Same-day follow-up supports simple positional-argument
calls whose arguments are themselves supported default expressions, such as
`x=default_arg(41)` and `debuginfo=identity_sym(:default)`, and abstract
inference now seeds keyword locals from those supported defaults so
`Base.infer_return_type(f, Tuple{})` preserves representative numeric and Symbol
return types. Same-day follow-up supports simple keyword calls such as
`x=default_kw(x=41)`, dependent keyword-call defaults such as
`x=default_kw(x=seed)`, and Symbol-preserving keyword call defaults. Remaining
scope is broader kwarg default parity for splatted/default-allocation call
shapes, complex keyword wrapper call forms, type annotations on all keyword
defaults, and full upstream keyword wrapper lowering semantics.

### MethodInstance/world-age remaining scope (Issue #4271)

2026-05-21: inference-only method table mutation は conservative single-world counter を進め、
return type cache / PartialStruct return cache / tentative recursive result / limited-accuracy
metadata を全て破棄するようになった。これにより method 追加・置換後に古い推論 cache を
そのまま再利用する代表的な stale-cache path は避ける。
2026-05-29: 上記 table-wide `.clear()` を **WorldRange-aware targeted invalidation** に置き換えた。
本家 `julia/Compiler/src/cicache.jl` の `WorldRange{min_world,max_world}` と
`julia/src/gf.c` の `jl_rettype_inferred` world ガードを写し、`return_type_cache` の各エントリに
valid world range と callee 依存(backedge 近似)を付与(`engine/world.rs`、`CachedReturn`)。
method mutation 時は mutate された関数自身と、その関数への依存 edge を持つエントリのみ
`max_world` を capped して retire し、無関係エントリの推論結果は valid なまま温存する。
lookup は world-gated。健全性優先(over-invalidate)で edge/world 未追跡の recursion-side cache
(tentative / limited)は依然 full drop。
2026-06-05 (#5603): `partial_struct_return_cache` も entry ごとの `valid_worlds`、
callee `edges`、`global_reads` を持つ `CachedConstructorPartial` に移行した。method mutation
では affected PartialStruct entry のみ retire し、無関係 entry は温存する。PartialStruct return
inference 中の user-function call も dependency edge として記録するため、callee mutation は caller の
PartialStruct fact を失効させる。binding change も `global_reads` に基づいて targeted に retire する。
2026-06-07 (#5939): PartialStruct return inference 中の precise function-table call は
bare callee edge ではなく `DispatchedMethodEdge { callee, arg_types }` を stamp する。
これにより `inner(::Float64)` の mutation で、`inner(::Int64)` だけに依存した caller の
PartialStruct side-cache entry を retire しない。
2026-06-07 (#6176/#5939): precise method-edge caller は typed callee method identity に
記録された `global_reads` も fold するため、callee method が読んだ global binding の変更で
caller cache も targeted に retire される。
2026-06-07 (#5939): binding change で retire した cache entry について、
`method_dependencies` も `global_binding_dependencies` / `function_dependencies` と同時に
clear する。precise method-edge 経由の global-read cache は、binding invalidation 後に古い
method-edge record を持ち越さず、再推論で current world の dependency を作り直す。
2026-06-07 (#6179/#5939): precise method-edge caller は typed callee method identity に
記録された transitive dependency edges も fold するため、`caller -> mid -> leaf` のような
chain で leaf method mutation が caller cache まで targeted に届く。
2026-06-07 (#6181/#5939): PartialStruct return inference でも cold callee inference 後に
precise dependency recording を再実行し、`outer -> mid -> inner` のような side-cache chain で
inner method mutation が outer PartialStruct cache まで targeted に届く。
2026-06-07 (#5939): `limited_results` / `tentative_results` でも `DispatchedMethodEdge`
による signature-aware invalidation を fixture 化し、`callee(::Float64)` mutation では
`callee(::Int64)` dependency を持つ side-cache entry を retire しないことを固定した。
2026-07-02 (#8739): `partial_struct_return_cache` / `CachedConstructorPartial` サイドキャッシュ
自体を撤去。PartialStruct fact は通常の `CachedReturn` に乗るため、上記 #5603/#5939 の
world/backedge invalidation セマンティクスは main return cache がそのまま提供する
(cache_invalidation.rs の PartialStruct テスト群を regular-cache 上へ移行して固定)。
2026-06-05 (#5603): method-table return inference は `DispatchError::AmbiguousMethod` を
unknown fallback ではなく `LatticeType::Bottom` として扱うようになった。`Base.infer_return_type`
は ambiguous dispatch を本家 Julia と同じ `Union{}` として報告し、非曖昧 signature の selected-method
return precision は維持する。`Base.return_types` の ambiguous no-result surface も fixture で固定した。
2026-06-05 (#5603): method mutation 時の同名 cache invalidation は、cache key の widened argtypes が
変更された `MethodSig` に dispatch し得る entry だけを retire するようになった。これにより
`f(::Int64)` の追加・置換で `f(::Float64)` specialization を一律に落とす over-invalidation を避ける。
imprecise/legacy callee dependency edge は引き続き bare-name conservative に retire する。
2026-06-05 (#5603): `limited_results` も entry ごとの `valid_worlds`、callee `edges`、
`global_reads` を持つ `CachedLimitedAccuracy` に移行した。method mutation / binding change では
affected limited marker だけ retire し、無関係 marker は温存する。
2026-06-05 (#5603): `tentative_results` も entry ごとの return type、`valid_worlds`、
callee `edges`、`global_reads` を持つ `CachedTentativeResult` に移行した。recursive fixpoint
iteration 間の clear は維持しつつ、method mutation / binding change では affected tentative
result だけ retire し、無関係 entry は温存する。
残りは本家 Julia と同じ厳密な `MethodInstance` / `CodeInstance` identity、callee backedge の
per-method precision、
method ambiguity diagnostics の full parity、compile ordering で method registration と
body inference を分離すること、および top-level redefinition の world-staged 可視化
(現状は static method table が両定義を同時に見るため `g(x::Int64)` 再定義後の return-type 反映は
#4985 の future-method leakage に支配される)/`invokelatest` / historical world filtering の
完全互換である。global binding 無効化は #4285。

### CallMeta/effects remaining scope (Issue #4274)

2026-05-21: `div` / `rem` / `mod` / `%` と binary integer division/modulo は
argument lattice type を見て、整数-only では effect-free かつ `DivideError`、
Float-containing numeric 経路では `nothrow` / `ExceptionType::Bottom` として扱うようにした。
2026-05-29: pure Julia の最小サーフェスとして `Base.infer_effects` /
`Base.infer_exception_type` を実装した。upstream `Compiler.Effects` と同一の
フィールド構成・UInt8/Bool エンコード・custom `show`
(`(+c,+e,+n,+t,+s,+m,+u,+o,+r)`)を持つ `Effects` struct を追加し、単純な
proven-total メソッドでは全 true Effects / `Union{}` を返す(fixture
`reflection_infer_effects_basic_4274`、julia 1.12 と parity 一致)。
残りは method/builtin semantics に基づく full CallMeta propagation、呼び出しごとの
厳密な効果分類(throwing / mutating / world-sensitive / type-callable 構築子の
`InexactError`・`BoundsError`・`DomainError` 等)、conversion / bounds / domain /
method errors の union exception type、interprocedural effects、override metadata、
world/backedge 情報を typed IR と reflection snapshot に接続することである。
2026-05-29 (#4970): throwing math helper (`sin`/`log1p`/`divrem`/`gcd`/`lcm`) は
per-signature classifier で upstream の inferred exception type
(`DomainError` / `Union{DomainError,InexactError}` / `DivideError` /
`OverflowError` / `Union{DivideError,OverflowError}`)と `nothrow=false` effects を
返すようにした(fixture `reflection_infer_effects_math_4970`、julia 1.12 parity)。
残りの string / parse / tuple / range / search helper の per-signature effect/exception
table は未整備(#4968 / #4969 / #4971 / #4974)で、現状 proven-total fallback のまま。
2026-05-29 (builtin category slice): reflect 可能な Core builtin
(`tuple`/`typeof`/`nfields`/`isa`/`typeassert`/`sizeof`/`ifelse`/`fieldtype`)を、
アドホックな per-name レコードでなく本家 `tfuncs.jl` `builtin_effects`/`builtin_exct`
と同じ意味カテゴリ集合 (`_PURE_BUILTINS` / `_CONSISTENT_BUILTINS` /
`_EFFECT_FREE_BUILTINS` / `_INACCESSIBLEMEM_BUILTINS`) のメンバーシップ +
呼び出しごとの `nothrow` 判定から合成するようにした(`reflection.jl`、fixture
`reflection_infer_effects_builtin_categories_4274`、julia 1.12 parity)。VM 側
Rust `infer_builtin_effects` も同カテゴリに整合(`getfield`/`fieldtype` を
`effect_free_may_throw` に修正)。残課題: bare identifier が関数値として reflect
できない builtin (`getfield`/`apply_type`/`throw`/`<:` は symbol 化 / MethodError)、
intrinsic 単位の `nothrow`/`exct` 精緻化(除算 typemin/-1・境界チェック)、
interprocedural CallMeta エッジ伝播、ユーザーメソッド body の effect 推論。
関連 follow-up:
- #4986: `@assume_effects` 由来の `Method.purity` は ca49737c で `Method` struct に
  public `purity::UInt16` として露出済み。回帰 fixture を追加して close。
- #4985: 後続 specific method 追加前の generic-only reflection 推論が保守的すぎる
  (body-based 精緻化が未実装)。
- #4986: `@assume_effects` 由来の `Method.purity` は PR #5007 で実装済み。
  `Method` への public purity field 露出が残課題。
- #4985: generic-only state での body-based reflection 精度
  (`reflection_late_specific_4271(x)=1` の `infer_return_type` / `return_types` が
  リテラル `1` から `Int64` を返す)は現 main で upstream と一致しており、回帰防止
  fixture `type_inference_generic_only_reflection_precision_4985` で固定済み。
  残課題は world-age 由来の future-method leakage: 同一ファイル/`-e` 列で後続の
  more-specific top-level method (`reflection_late_specific_4271(x::Int64)=...`)
  が top-level 実行到達前に静的 method table へ可視となり、初期の generic-only
  reflection が `DataType()` 等の誤結果を返す。この portion は #4271/#4285 の
  world-age epic に残す。
- #4987 / #4991: `infer_effects` / `_function_name` の type-callable (`Int64` 等)
  対応。現 main では `_function_name` 自体が未実装で、issue が前提とする
  実験ローカル状態とは別。type-callable の効果分類は未着手。
- #4957: `applicable` は実装済み(PR #4957 系)。`hasmethod` / `which` / `methods`
  等に対する厳密な effects/exception 値は full classification 待ち。
- 2026-05-31 (#4274): ビット演算/シフト演算子(`xor` / `&` / `|` / `~` / `<<` /
  `>>` / `>>>`)を整数引数に対し `EFFECTS_TOTAL` / `Union{}` として
  `_classify_helper_effects` に分類追加(fixture
  `reflection_infer_effects_bitwise_4274`、julia 1.12.6 parity)。残課題: 名前付き
  ビットカウントヘルパ(`count_ones` / `count_zeros` / `leading_zeros` /
  `trailing_zeros` / `leading_ones` / `trailing_ones`)と `bitrotate` は Rust
  builtin として存在するが第一級の関数値として未解決(`UndefVarError`)のため
  `infer_effects` で reflect できない(#5333 で起票)。分類アームには将来の関数値
  露出に備えて含めてあるが、関数値サーフェス拡張は #5333。

### Rust fallback dispatch audit closure (Issue #4276)

2026-05-21: array/memory `!=` は direct `BuiltinId::Isequal` fallback を使わず、
`==` を compile してから `NotBool` することで、upstream の `!=(x, y) = !(x == y)` と同じく
user-defined `==` method を尊重するようになった。残りは `collect` / `similar` /
`broadcast` / `map` / array arithmetic / LinearAlgebra などの public-name Rust fallback を
dispatch-first boundary、primitive representation bridge、removable shortcut に分類し、
新規 fallback route の CI audit と代表 fixture を追加することである。
同日 follow-up で `BASE_FUNCTION_ROUTES` の route 名は
`docs/vm/PUBLIC_FALLBACKS.md` と `scripts/check_base_routing_registry.sh` の同期監査に
接続した。残りは分類済み route を個別に削減・置換し、dispatch-first behavior を
代表 fixture で増やすことである。
2026-05-22 follow-up: `eltype(::Vector{Int64})` user method は direct call と
`x::Any` runtime path の両方で array eltype fallback より先に選ばれるようになった。
同日 follow-up で `size(::Vector{Int64})` user method も direct call と `x::Any`
runtime path の両方で array shape fallback より先に選ばれるようになった。
同日 follow-up で `length(::Vector{Int64})` / `ndims(::Vector{Int64})` user method と
1 引数 `similar(::Vector{Int64})` user method も `x::Any` runtime path で VM fallback
より先に選ばれるようになった。同日 follow-up で direct `getindex` / `setindex!` と
bracket `a[i]` / `a[i] = v` も matching user method が存在する場合は VM
`IndexLoad` / `IndexStore` fallback より先に method dispatch へ入る。同日 follow-up で
direct `reshape(::Vector{Int64}, dims...)` と `x::Any` 経由の `reshape(x, dims...)`
も user method を VM reshape fallback より先に選ぶようになった。同日 follow-up で
明示 `a::Any, b::Any` 経路の `Vector{Int64} + Vector{Int64}` user method も VM array
arithmetic fallback より先に runtime binary dispatch で選ばれるようになった。同日
follow-up で lazy-specialized unannotated `f(a, b) = a + b` / full-form
`function f(a,b)` の非 primitive binary operand は generic VM body に戻し、同じ
`Vector{Int64} + Vector{Int64}` user method が Rust array arithmetic fallback より先に
選ばれるようになった。同日
follow-up で `map(::typeof(identity), ::Vector{Int64})` と
`broadcast(::typeof(+), ::Vector{Int64}, ::Vector{Int64})` の callable singleton
user method も direct call と `Any` runtime path で generic HOF fallback より先に
選ばれるようになった。同日 follow-up で generic unary
`broadcast(identity, array)` / `broadcast(f, array)` materialization も typed vector を
返すようになり、binary `broadcast(f, A, B)`, `broadcast!`, `preprocess`,
`similar(::Broadcasted, ::Type)`, 2D fast path, reshaped shared-storage broadcast,
および struct-backed `LinRange` / `StepRangeLen` collect も同じ VM lowering 境界を避ける。
同日 follow-up で `push!` / `pop!` / `pushfirst!` / `popfirst!` / `insert!` /
`deleteat!` は direct Array call と `Any` receiver path の両方で same-arity user method
を VM mutation fallback より先に選べるようになった。通常の non-overridden Array mutation
は既存 VM fallback を維持する。
同日 follow-up で `in(::Int64, ::Vector{Int64})` user method も direct call と
`Any` runtime path で membership fallback より先に選ばれ、`Vector{Any}` では
本家 Julia と同じく mismatched `Vector{Int64}` method を避けて membership fallback に戻る。
同日 follow-up で binary `∈` も public callable へ lowering し、`∈` user method が
`in` rewrite より先に選ばれる。同日 follow-up で `∉` / `∋` / `∌` の否定・引数反転
alias も public callable へ lowering し、それぞれの user method が rewrite より先に
選ばれるようになった。
同日 follow-up で `reshape` 結果の rank を Julia dispatch inference に残し、
`map(::typeof(identity), ::Matrix{Int64})` は direct/static call でも Matrix user method を
Vector/generic fallback より先に選べるようになった。
同日 follow-up で `broadcast_range_size_2d` の fused `xs' .+ im .* ys` も含めて
`broadcast::` fixture category は green になった。残りは LinearAlgebra などの他 public
fallback route で同じ dispatch-first 代表 fixture と boundary 削減を進めること。
同日 follow-up で multi-arg `similar(a, dims...)` / `similar(a, T, dims...)` は
user-backed same-arity method が存在する場合に `CallTypedDispatchOrBuiltin` で VM fallback
より先に試すようになり、direct call と `x::Any` runtime path の代表 user method は本家
Julia と同じ結果になった。通常 Array allocation は iOS-safe VM fallback を維持している。
同日 follow-up で `keys` / `values` / `pairs` の `Dict{K,V}` user method も
direct call と `x::Any` runtime path で VM Dict view fallback より先に選ばれるようになった。
このために homogeneous `Dict(k => v, ...)` constructor は runtime `Value::Dict` の
key/value type parameter を保存し、代表ケースでは `Dict{Any, Any}` ではなく
`Dict{String, Float64}` として dispatch に見える。
同日 follow-up で `keytype(::Dict{K,V})` / `valtype(::Dict{K,V})` user method も
direct call と `x::Any` runtime path で VM Dict type-parameter fallback より先に
選ばれるようになった。unmatched fallback も保存済み key/value type parameter を返す。
同日 follow-up で `get!(::Dict{K,V}, key, default)` user method も direct call と
`x::Any` runtime path で VM Dict mutation fallback より先に選ばれるようになった。
同日 follow-up で `empty!(::Dict{K,V})` と `merge!(::Dict{K,V}, ::Dict{K,V})`
user method も direct call と `x::Any` runtime path で VM Dict mutation fallback より先に
選ばれるようになった。unmatched fallback は既存と同じく mutated Dict を元 local に
store-back する。
同日 follow-up で `delete!(::Dict{K,V}, key)` user method も direct call と
`x::Any` runtime path で VM Dict delete fallback より先に選ばれるようになった。
unmatched fallback は mutated Dict / Set を元 local に store-back する。
同日 follow-up で `pop!(::Dict{K,V}, key[, default])` user method も direct call と
`x::Any` runtime path で VM Dict pop fallback より先に選ばれるようになった。
unmatched fallback は mutated Dict を元 local に store-back しつつ popped/default value を返す。
Issue #4276 closure: public-name Rust fallback の route inventory、retained
fallback classification、CI audits、代表 dispatch-first fixtures は完了済み。
残りは `CallTypedDispatchOrBuiltin` を他の public fallback route へ広げる broad
audit ではなく、#4568 の final VM-native carrier cleanup / `Value::Array`
variant removal として扱う。

### String/Symbol/Bool/Char/Type{T} equality fallback remaining scope (Issue #4298)

2026-05-21: `Base.:(==)(::String, ::String)` と
`Base.:(==)(::Symbol, ::Symbol)` / `Base.:(==)(::Bool, ::Bool)` /
`Base.:(==)(::Char, ::Char)` / exact `Base.:(==)(::Type{T}, ::Type{T})`
は direct call と `Any` runtime dispatch の両方で VM fallback より先に選ばれ、`!=` も
`==` + `NotBool` 経由で user override を反映する。
残りは String ordering / generic `DataType`・`Type` object equality / UnionAll など他の
singleton・primitive comparison fallback を同じ dispatch-first inventory で監査し、`!=` の
generic lowering をより体系的に本家 `julia/base/operators.jl` へ寄せることである。

### Base.Generator collect boundary handoff (Issue #4265)

2026-05-21: user-defined `collect(::Base.Generator)` は direct
`collect(Base.Generator(...))` と `collect(x::Any)` runtime path の両方で
`collect_generator` fallback より先に dispatch scoring へ入るようになった。
通常の VM-native generator materialization fallback は維持している。同日 follow-up
で `IteratorSize(typeof(g))` / `IteratorEltype(typeof(g))` の representative
type-object trait と、明示 `Tuple{Base.Generator{I, typeof(f)}}` collect reflection
return inference は本家に寄せた。さらに `Tuple{typeof(g)}` の context-aware 型式 lowering
も dynamic type construction に通し、代表ケースの `Base.infer_return_type(collect,
Tuple{typeof(g)})` は `Vector{Int64}` / `Vector{Float64}` を返す。同じく
`T = typeof(g); Tuple{T}` のような local DataType alias も runtime DataType value として
評価する。残りは本家 `julia/base/array.jl` の `collect(itr::Generator)` / `_collect` /
`collect_to_with_first!` を Pure Julia method dispatch へさらに寄せ、special-case sentinel
と VM-native representation boundary を縮小することである。
2026-05-21 follow-up: user generator method が存在しない direct
`collect(Base.Generator(...))` は static `RangeCollect` 直行ではなく、collect method table
の `CallDynamic` sentinel を通るようになった。さらに VM-native `Value::Generator` は
representative non-splat callable で `f` / `iter` field projection を持ち、
明示 `Base._collect(1:1, g, IteratorEltype(g), IteratorSize(g))` は本家と同じ値を返す。
ただし public `collect(g)` を本家 `Base.collect(itr::Generator)` method body へ常時入れると、
既存の `collect_similar(::Memory, ::Generator)`・tuple-splat・filtered generator の
iOS/no-JIT safe behavior を壊すため、実行時 materialization はまだ
`collect_generator` boundary に戻す。残りはこの sentinel 自体を本家
`Base.collect(itr::Generator)` / `_collect` method body へ吸収できる object model と
callable semantics を整えること。
2026-05-22 follow-up: nonempty かつ Vector/Memory/Range-backed、lowercase named
function callable の VM-native `Value::Generator` は public `collect(x::Any)` runtime sentinel
から generic `collect(::Any)` / `_collect` method body へ入れるようにした。empty generator は
VM が保持する `result_element_type` に依存するため native boundary に残し、type constructor、
tuple-backed、tuple-splat、filtered、eager、unknown iterator shape も iOS/no-JIT safe behavior
を優先して `collect_generator` に残す。numeric type constructor vararg arity の別バグは #4316 に分離した。
同日 follow-up で nonempty Matrix-backed generator も同じ generic `_collect` path に通し、
shape-preserving `HasShape` collect の direct/runtime public path を固定した。
2026-05-27 に current checkout で再検証し、#4265 の goal である native
`collect_generator` surface の縮小と documented compatibility bridge 化は完了として
closed。empty/type-constructor/tuple-splat/filtered/eager/runtime-callable cases の
完全撤去は #4568 の final fallback / native carrier cleanup へ渡す。

### AoT const specialization remaining scope (Issue #4272)

2026-05-21: AoT `CodeInstanceKey` は ABI/codegen 用 `arg_types` と別に const-aware
`arg_key` を持つようになり、direct user-function call site の Bool / Nothing / Symbol /
small Int literal は compile-side `InferenceCacheKey` と同じ controlled const policy で
specialization identity に残る。final inference attach も同じ function / ABI tuple の
const-aware instances へ反映される。残りは AoT body inference の env 自体へ const value を流すこと、
ternary / `Val`-like dispatch / Symbol field selector の return precision を compile-side と比較する
fixture を追加すること、broadcast / generated call-site collection へ同じ key policy を広げること、
および large/non-profitable const widening の監査を CI に入れることである。

### PartialStruct lattice/cache remaining scope (Issue #4269)

2026-05-21: immutable default constructor 由来の field facts は one-call
interprocedural return boundary を越えて保持されるようになり、
`getfield(make_box(flag), :b)` と `box = make_box(flag); getfield(box, :b)` の代表ケースは
本家同様 `String` と推論される。
2026-05-29 follow-up: パラメトリック struct コンストラクタの reflection 推論を解消
(Issues #4849 / #4850 / #4851)。immutable パラメトリック既定コンストラクタ (#4849)、
明示パラメトリック内部コンストラクタ `Foo{Int64}(...)` の `new{T}` body (#4850)、
ネスト型フィールド `Tuple{T,T}` からの `T` 束縛 (#4851) が、いずれも具体インスタンス
(`Foo{Int64}`) とフィールドファクトを保持し本家 Julia と一致する。
残りは `Core.PartialStruct` 相当を first-class lattice value
として表現すること、branch/loop join で field facts を完全に merge すること、
recursive immutable structs / custom outer constructor partial facts へ広げること、
および cache invalidation / LimitedAccuracy metadata と統合することである。

### comparison-aware widening remaining scope (Issue #4273)

2026-05-21: `join_limited` は public bounded `join` を経由せず raw join から
`limit_type_size(compare_to=...)` を適用するようになり、env merge も previous type を
comparison source として使う。代表 fixture では既知の 9-member wide union を
`flag ? x : x` で `Any` に潰さず保持する。残りは all inference join callsites
(function return joins、ternary / array literal / union-split method return joins、union_split
merge path) を comparison context 付き API へ移すこと、source/compare の provenance を
diagnostic に残すこと、および本家 `tmerge` / `limit_type_size` の tuple-depth / tuple-len
budget と完全に揃えることである。

2026-05-30: `limit_type_size` に比較対応の再帰成長有界化を追加(`widening.rs` の
`limit_concrete_against` / `concrete_more_complex` / `widen_concrete_to_wrapper`)。
単一 `Concrete` および `Union` メンバーが `compare_to` と同ラッパーの構造成分より
深くネストする場合、本家 `type_more_complex` / `_limit_type_size` の wrapper-widening に
倣って当該成分を `Any` に潰し、`x = (x,)` / `a = [a]` のような再帰累算器を絶対上限の
手前で有界化する(fixture `recursive_type_growth_4273.jl`)。残りは (1) 本家の要素単位
Tuple 限定(`__limit_type_size` の `allowed_tuplelen` / element-wise 適用)に揃え、
ラッパー全潰しではなく深い slot のみを限定すること、(2) `tupledepth` / `allowed_tupledepth`
パラメータの導入、(3) 残りの非 env join callsites(function return / ternary / array literal /
union-split method return / union_split merge)への comparison context 付き API の適用、
(4) source provenance の diagnostic 化、である。

### Zip iterator type preservation remaining scope (Issue #4281)

2026-05-21: `Zip5` / `Zip6` / `Zip7` も `Zip` / `Zip3` / `Zip4` と同じ native collect /
iterator recognition path に入り、`ArrayElementType::TupleOf` の `eltype` projection は
canonical `Tuple{...}` 型として比較できるようになった。代表 fixture では zip6 / zip7 の
`typeof(result)` / `typeof(result[1])` / `eltype(result)` が本家 Julia と一致する。
残りは unbounded arbitrary arity の `zip`、runtime function-valued zip の全 arity、Base の
trait-shaped `_collect` path へのさらなる移行、および VM-native iterator boundary の縮小である。

### Union-split reflection remaining scope (Issue #4287)

2026-05-21: `Union{Int64,String}` と `Union{String,Int64}` の `DataType`
比較が本家同様 order-insensitive になり、`Base.return_types` と
`Base.infer_return_type` が union member construction order の違いで不一致にならない
ことを代表 fixture で固定した。残りは本家の `max_methods` /
per-module・per-function override、`_apply_iterate` 専用
`max_apply_union_enum == 8`、method ambiguity を含む完全な match lattice、
および diagnostic を source span / trace UI に接続することである。

### Type-stability report parity remaining scope (Issue #4291)

2026-05-21: #4291 の representative parity fixture と API regression で、arrays,
tuples, generators, local closure generator bodies, dispatch caller は
`Base.infer_return_type` / runtime reflection と type-stability report の stable return
shape が一致することを固定した。同日 follow-up で public API / CLI report path は
production shared inference snapshot から return-type stability を分類し、
`uses_production_inference: true` を返すようになった。残りは statement-level typed IR、
effect/cache/invalidation facts、world-age metadata、および diagnostic span を typed IR /
`code_typed` 相当の user-facing location へ接続することである。
同日 follow-up で CLI `--type-stability --json` は Base/prelude 全関数を eager 推論せず、
`plain_4291() = 41` のような function-only file を timeout なしで stable report にする。
同日 follow-up で user struct table と default-constructor return recovery を production
facts に接続し、`make_box() = TSBox(41); field_from_box() = make_box().x` の代表ケースも
stable report になる。同日 follow-up で non-parametric inner constructor method return
snapshots を production report inference engine に seed し、
`TSBoxInner(x) = new(x + 1); field_inner() = make_inner().x` の代表ケースも stable report にする。
残りは custom outer constructor partial facts、parametric inner constructor report parity、
broader closure/report parity、statement-level typed IR、effects、cache invalidation、
world-age metadata である。

### MustAlias-style field identity remaining scope (Issue #4270)

2026-05-21: `getfield(obj, :field)` の static Symbol が parser-lowered
`QuoteLiteral(SymbolNew(...))` の場合でも declared union field type を保持するようになった。
同日 follow-up で mutable array の repeated indexed read は本家同様 `MustAlias` field
identity として扱わず、`Vector{Union{Int64,Nothing}}` の `a[1] !== nothing ? a[1] : 0` は
`Base.infer_return_type` / `Core.Compiler.return_type` とも `Union{Nothing,Int64}` を返す。
残りは本家 `MustAlias` / `InterMustAlias` 相当の SSA-versioned field identity、alias graph、
field mutation invalidation、nested immutable field path の完全な統一である。indexed access は
mutable container write / alias analysis を導入するまで field path とは分けて扱う。
2026-06-04: field/index refinement は ordinary `TypeEnv.bindings` の string path-key 混在から
`RefinementPath { root, segment }` keyed side table へ分離済み。残りはこの structured root を
SSA definition version と結び、alias graph / InterMustAlias identity として root rebind・field write・
nested path provenance を精密に扱う scope として #5601 に継続する。
同日 follow-up で branch join は両 incoming env に存在する同一 structured refinement だけを join して
残すようにし、片側 branch の root rebind で消えた refinement が stale に復活する #5858 を修正した。
さらに `current_narrowable_type` が ordinary binding の前に structured refinement side table を参照し、
`x.f !== nothing` 後の `x.f isa T` のような chained guard で既存 refinement を再利用する #5860 を修正した。
`o.inner.val` のような nested field guard も structured path として記録・参照し、`o.inner` 書き換え時に
descendant refinement を落とす #5862 を修正した。nested assignment lowering の
`tmp = o.inner; tmp.val = ...` は `tmp -> o.inner` field-path alias で元の nested refinement を落とし、
stale `Int64` が残る #5864 を修正した。これに必要な inline `Stmt::Block` 推論実行 #5865 も対応済み。
SSA definition version / InterMustAlias identity そのものは引き続き #5601 の残 scope。

### Global binding invalidation remaining scope (Issue #4285)

2026-05-21: non-const global reader が単一代入・関数定義後の初回代入・incompatible
reassignment の代表ケースで stale concrete type を返さず、本家同様
`Base.infer_return_type` / `Base.return_types` で `Any` を返すことを fixture に追加した。
残りは binding dependency graph を inference cache key / invalidation record として
明示表現し、world-age / module partition / method cache invalidation と統合することである。

2026-05-30: `InferenceEngine` に per-global-binding 依存追跡を追加し、
`set_global_types` を全置換から差分検出ベースの**ターゲット無効化**へ変更した。
`global_binding_dependencies`(関数→読んだ束縛集合)と `CachedReturn.global_reads`
スナップショット、`binding_world` を導入し、変更された束縛(追加/削除/型変更)を読む
キャッシュ結果のみ `valid_worlds` を `cap_before` する(本家
`bindinginvalidations.jl` の per-`GlobalRef` エッジ無効化に対応)。callee 経由の
推移的 global 読みも畳み込む。const 再定義で依存関数の戻り型が更新される観測 fixture
(`const_redefinition_invalidation_4285.jl`)を本家 parity 付きで追加。残りは
binding partition / module 修飾の依存グラフ materialization、historical world-age、
副キャッシュ(partial_struct / tentative / limited)の束縛エッジ単位無効化、
method-cache invalidation との完全統合である。

### CFG/worklist inference remaining scope (Issue #4267)

2026-05-21: while-loop fixpoint output と condition false-branch env を交差させ、
`while x isa Int64; x = "s"; end` の通常終了で loop-carried assignment が本家同様
`String` と推論される代表ケースを fixture に追加した。残りは production inference path
全体を lowered CFG block / block input-output state / explicit worklist propagation へ接続し、
statement side table、loop backedge、Phi/SSA value types、branch state を legacy walker から
切り替えることである。

2026-06-02: `infer_block_with_fixpoint` に lowered CFG / `run_to_fixpoint`
observation pass を接続し、production entry `TypeEnv` から block input/output state を
記録するようになった。`statement_type` も lowered CFG payload id ベースで refresh される。
残りは edge-predicated branch narrowing、while/for transfer の worklist authoritative 化、
CFG-authoritative returns、Phi/SSA value typing、および `break` / `continue` / `return` /
`for` / `try` / short-circuit edge lowering の完全化である。

### Reflection CodeInfo remaining scope (Issue #4288)

2026-05-21: `Base.code_lowered` と `Base.code_typed(...)[i][1]` は `nothing` placeholder
ではなく `CodeInfo` record を返すようになった。残りは本家 `Core.CodeInfo` と同じ
lowered/typed SSA statements、slot metadata、world bounds、effect/inlining metadata を
materialize することである。同日 follow-up で representative `CodeInfo.code` field は
本家同様 access 可能になり、空 placeholder ではなく代表 call/return `Expr` vector を
返す。ただし、まだ full lowered/typed SSA IR ではない。
同日 follow-up で `code_typed(...; optimize=false)` /
`debuginfo=:source` は accepted boundary として通し、unsupported `generated=false` は
reject するようにした。さらに `debuginfo=:default` / `:source` / `:none` と invalid
symbol の `ArgumentError` boundary も本家に合わせた。残りは `code_typed` の interp /
world / full optimize semantics を本家 compiler semantics に寄せることである。

2026-05-29 (Issues #4979/#4982/#4983/#4984): `CodeInfo` に代表 `nargs`(UInt64)/
`isva` / `has_fcall` / `inlining_cost` / `purity` / `has_image_globalref` /
`propagate_inbounds` / `nospecializeinfer`、`Method` に `nospecialize`(Int32)/
`isva` / `propagate_inbounds` / `nospecializeinfer` / `purity` を追加。`has_fcall`
は常に false、`has_image_globalref` は no-global 関数の false のみモデル化。残りは
全 SSA statement / slot / world / backedge / context-sensitive flag の精密
materialization(full `Core.CodeInfo` parity)であり、`@nospecialize(x)` 仮引数位置
metadata の retention(現状 statement-position のみ)も含む。

### Array/Vector eltype reflection remaining scope (Issue #4299)

2026-05-21: `eltype(Array)` / `eltype(Vector)` は exact `Type` methods で `Any`
を返すようになり、`Vector{Int64}` / `Matrix{Bool}` の bound element type projection は
維持される。残りは broader `UnionAll` method matching で未束縛 TypeVar を method body に
漏らさない resolver 側の一般化と、Array alias 以外の unparameterized parametric type object
全体の監査である。

### World-age architecture remaining scope (Issues #4271/#4285)

2026-05-21: `Base.code_typed(...; world=w)` は single-world typed snapshot として通し、
`Base.code_lowered(...; world=w)` は本家同様 unsupported keyword として拒否する boundary を
fixture に追加した。#4290 の public invoke / invokelatest / world-specified reflection
surface は single-world compatibility と明示 rejection boundary として閉じる。残りは
#4271/#4285 側で扱う architectural scope であり、historical world filtering、method
invalidation semantics、world-specific method-table snapshots、`code_typed` の
interp/optimize/generated keyword semantics を本家 compiler path に寄せることである。
2026-06-07 (#5939): top-level method staging は no-JIT/static prepass と前方参照サポートの
衝突として明示的に未対応に残す。例えば同一入力内の
`w(x)=100; println(w(1)); w(x)=200; println(w(1))` は upstream Julia では
`100` / `200` と各実行到達時点の method world を見るが、sjulia は実行前に全 method table を
構築するため最終 table に束縛され、両方が `200` になる。この差は cache invalidation の漏れではなく、
historical world filtering / world-specific method-table snapshots / runtime method birth-death
visibility を導入するまで残る architectural limitation である。

## AoT Type Semantics Follow-up Notes

2026-05-19: #3912 の first slice として AoT inbound `JuliaType` / type-name
conversion は shared `CoreType` へ投影してから `StaticType` に落とすようにした。
wide primitive、`Missing`、tuple、union、`Vector{T}` / `Matrix{T}` /
`Array{T,N}` の supported backend shape は保持する。残りは AoT widening / meet /
constant typing / call specialization のさらに内側を `CoreType` / shared dispatch
resolver に寄せること、`Type{T}` / `UnionAll` / TypeVar / arbitrary value parameter
を誤って `StaticType` の意味論として所有しない explicit diagnostic にすることである。

## Test Infrastructure Follow-up Notes

2026-05-18: #3972 の full release nextest timeout mitigation として generated fixture
chunk size は 32 に増やし、`scripts/check_fixture_chunk_size.sh` で manifest entry 数と
generated chunk 数を監査するようにした。今後 fixture 数が大きく増えた場合は、テストを
skip せずに chunk size / persistent cache / slow category split の追加調整で対応する。

## Runtime Representation Follow-up Notes

2026-05-20: #4270 で `x isa UserStruct` の user-defined struct 名を conditional
narrowing が struct table 経由で解決し、local alias 後の field access 代表ケースは
`Any` へ落ちずに済むようになった。残りは本家 `MustAlias` / `InterMustAlias` 相当の
SSA versioning、mutable field write / index write を含む alias invalidation、nested path、
call-form `getfield` と surface field access の完全統一である。

2026-05-20: #4275 で production tfunc 登録は legacy metadata-free shim を使わず、
明示 arity/cost rule に統一した。`abs(::Float64)` が経由する `abs_float` 代表ケースも
reflection inference で `Float64` を保持する。残りは upstream `Compiler/src/tfuncs.jl`
全体の intrinsic/builtin transfer semantics、effect/cost model、`Type`/`Const` lattice
情報を使う tfunc の完全移植である。

2026-05-20: #4287 で ordinary method dispatch の union-split expansion budget を
本家 `InferenceParams.max_union_splitting == 4` 相当に分離し、small union method return
join は `Base.return_types` に反映されるようになった。残りは本家の
`max_methods` / per-module・per-function override、`_apply_iterate` 専用
`max_apply_union_enum == 8`、method ambiguity を含む完全な match lattice、
および diagnostic を source span / trace UI に接続することである。

2026-05-20: #4277 で homogeneous `String` / `Char` array literals は本家同様
`Vector{String}` / `Vector{Char}` を保持し、upstream lowering と同じ
`getindex(::Type{T}, vals...)` 代表経路で `String["a"]` / `Char['x']` /
`Int8[1]` / `Any["a",1]` も typed vector constructor として扱うようになった。
残りは typed non-empty array syntax の任意 element type、matrix/vcat/hcat typed literal、
厳密な conversion semantics、および array literal promotion をより広い Julia
conversion/promotion semantics に寄せることである。

2026-05-20: #4278 で `String` literal arrays は `similar` / `repeat` / `permutedims` の代表ケースでも
本家同様 `Vector{String}` / `Matrix{String}` を保持するようになった。残りは arbitrary
`AbstractString` subtype、Any-typed string array の包括的な promotion/retagging、typed
non-empty array syntax と連動した全 array transformation の統一である。

2026-05-20: #4279 で heterogeneous numeric generator collect の代表ケースは
`Vector{Real}` へ typejoin し、`(1, "a")` のような non-numeric heterogeneous tuple
generator/comprehension は `Vector{Any}` へ widening するようになった。残りは full
`typejoin` lattice を使った任意 branch/body 形状、Union element type、runtime-only
generator values の精密 widening である。

2026-05-20: #4290 で `Base.invokelatest(f, args...; kwargs...)` の positional /
vararg / keyword 代表ケースは world-age-free ordinary call wrapper として通るようになった。
また `invoke(f, Tuple{...}, args...)` / `Base.invoke(...)` の statically named function +
statically known Tuple signature 代表ケースは、指定 signature の method を選ぶようになった。
simple `@invoke f(x::T, ...)` macro form も同じ static `invoke` path へ lower する。
`g = f` のような静的 function alias も `invoke(g, Tuple{...}, args...)` で同じ method table
lookup を使う。`sig = Tuple{...}` のような静的 DataType value alias も `invoke(f, sig, args...)`
で使用できる。`@invokelatest f(args...; kwargs...)` と keyword 付き
`invoke(f, Tuple{...}, args...; kwargs...)` の代表ケースも本家互換になった。
同日 follow-up で `Core.invoke` / `Core.invokelatest` の module-qualified primitive lookup と、
`Tuple{Vararg{T}}` invoke signature の arity 展開も代表ケースで本家互換になった。
同日 follow-up で `@invokelatest x.f`, `@invokelatest x.f = v`,
`@invokelatest xs[i]`, `@invokelatest xs[i] = v` の property / index /
assignment 代表ケースも `invokelatest(getproperty/setproperty!/getindex/setindex!, ...)`
へ lower するようになった。
同日 follow-up で `f` が引数として渡された Function value でも、static Tuple signature の
`invoke(f, Tuple{Number}, x)` 代表ケースは指定 signature で method selection できるようになった。
さらに `sig` が引数として渡される runtime Tuple signature でも、non-keyword
`invoke(f, sig, x)` / `invoke(runtime_f, sig, x)` の代表ケースは指定 signature dispatch に入る。
同日 follow-up で function value 経由の keyword `invoke` も、static Tuple signature と runtime
Tuple signature の代表ケースで指定 signature dispatch 後に keyword binding できるようになった。
さらに keyword splat `invoke(...; kw...)` も、static named / runtime Function value /
runtime Tuple signature の代表ケースで本家互換の指定 signature dispatch に入る。
同日 follow-up で `@invoke f(x)` の unannotated positional argument は runtime
`typeof(x)` を Tuple signature に入れ、`@invoke f(x::T; kw...)` も keyword /
keyword-splat を保持するようになった。
同日 follow-up で property / index / assignment 形の `@invoke` 代表ケースは
`getproperty` / `setproperty!` / `getindex` / `setindex!` の declared-signature
`invoke` へ lower するようになった。さらに `Base.get_world_counter()` /
`Base.tls_world_age()` / `Base.invoke_in_world(world, f, args...; kwargs...)` の
single-world compatibility surface は代表ケースを通すようになり、`UInt64(0)` の
old-world boundary は本家同様 `MethodError` で拒否するようになった。true historical
world dispatch と method birth/death world filtering は #4271/#4285 の architecture
scope に切り出し、#4290 の public surface は single-world compatibility と明示
rejection boundary として扱う。
同日 follow-up で `hasmethod(...; world=...)`、`Base.return_types(...; world=...)`、
`Core.Compiler.return_type(..., world)` の代表ケースは single-world method table に
接続した。さらに `hasmethod(f, Tuple{...}; world=typemax(UInt64))` は本家代表ケース同様
`false` にし、`hasmethod(f, t, kwnames; world=typemax(UInt64))` は本家と同じ
generated-function boundary error にする。real historical world filtering と method
invalidation は #4271/#4285 に残し、`methods` / `code_lowered` など本家が world keyword を
受けない API との代表 error surface は #4290 の fixture で固定している。
同日 follow-up で upstream doctest `@invoke 420::Integer % Unsigned` の代表 operator
form は `%` を `rem` alias として declared-signature `invoke` に lower し、integer
type-conversion remainder method へ接続するようになった。
同日 follow-up で `@world length Base.get_world_counter()` と
`@world Base.length Base.get_world_counter()` の代表ケースは、world expression を評価したうえで
single-world の Function value に解決するようになった。true historical world filtering は
#4271/#4285 に残し、`GlobalRef` materialization、`∞` world handling、non-function global
bindings、module import/visibility edge cases は broader runtime reflection follow-up として扱う。

2026-06-01: #5123（`invoke(f, Tuple{...}, args...)` 明示シグネチャ method 呼び出し）は
上記 #4290 の `invoke` compile path で既に代表ケースが実装済みであることを確認し、専用
回帰 fixture `dispatch::invoke_explicit_signature_5123` を追加して resolved とした。
single/multi-arg、`Tuple{Vararg{T}}`、parametric `where`、`Base.invoke` / `Core.invoke`、
function alias / signature alias、keyword 付き invoke の代表ケースを upstream Julia 1.12.6 と
parity 固定。残りは statically-known な no-method `invoke` を compile error ではなく
catchable runtime `MethodError` にする件で、これは通常 dispatch と共通の static-pipeline
limitation（invoke 固有ではない）として別途扱う。

2026-05-20: #4292 で `hasmethod(f, types, kwnames)` の代表ケースは既存
`FunctionInfo.kwparams` metadata から named keyword / unknown keyword / `kwargs...`
acceptance を判定するようになり、known-function keyword splat は runtime `Pairs` を
展開して `kwargs...` forwarding できるようになった。known-function / module-qualified call の
`NamedTuple` keyword splat と positional splat、function value / nested closure keyword
splat、closure positional splat、callable struct variable keyword splat の代表ケースも
metadata を保持する。残りは arbitrary callable wrapper 経由の keyword metadata
preservation、method ambiguity と keyword sorter の完全互換、`methods` / `which` など
他 reflection API への keyword signature filter propagation である。

2026-05-20: #4294 で `c = (x; y=1) -> x + y`、keyword-only arrow、`kwargs...`
forwarding、`args...; kwargs...` forwarding、IIFE arrow の代表ケースは本家互換になった。
残りは arrow parameter head 以外の semicolon-containing parenthesized block 表現、
typed/default positional arrow parameter のより広い構文、macro expansion 経由で生成される
arrow signatures の完全互換である。

2026-05-21: #4268 で `Val{N}` / `Val(N)`、`NTuple{N,T}`、および
`ntuple(i -> i, N)` の代表 value-parameter binding は本家互換の代表ケースを通すように
なった。同日 follow-up で ordinary Float64 value parameter `Val{1.5}` も runtime
value として束縛する。同日 follow-up で simple Char value parameter `Val{'x'}` も
runtime `Char` value として束縛する。escaped Char value parameter も ordinary char
literal と同じ decoding で扱い、`Val{'\n'}` / `Val{'\''}` / `Val{'\\'}` の代表ケースを
本家互換にした。hex / Unicode escape 代表ケース `Val{'\x41'}` / `Val{'\u03B1'}` も
同じ path で扱う。同日 follow-up で `ntuple(i -> i + a, 3)` と
`f = i -> i + a; ntuple(f, 3)` の代表 capturing arrow callable も runtime callable path で
closure captures を保持し、`Base.infer_return_type(..., Tuple{Int64})` も本家同様
`Tuple{Int64, Int64, Int64}` を返すようになった。non-finite Float value parameters
`Val{Inf}` / `Val{-Inf}` / `Val{NaN}` も同じ Float64 path で本家互換に fixture 化した。
tuple value parameter `Val{(1, 2)}`、`Val{()}`、`Val{(1,)}`、
`Val{(true, :x)}`、および nested tuple 代表ケースも本家互換にした。
2026-05-22 follow-up で `Val{UInt8(1)}` / `Val{Int32(2)}` /
`Val{Float32(1.5)}` と `Val{(UInt8(1), Int32(2))}` の typed constructor value
parameter も本家互換にした。
2026-05-31 follow-up で `g(::Type{T}) where T = T` のように `::Type{T}` から束縛した
型変数 `T` を直接返すメソッドの `Base.infer_return_type` を本家互換の `Type{T}`
(`Type{Int64}` / `Type{Float64}` / `Type{String}`)に精密化した(実行時値・
`typeof === DataType` は元から本家互換)。残りはこれらの代表 numeric/Char/Float/
Symbol/tuple/`Type{T}` 以外の broader arbitrary isbits value parameter、heterogeneous
tuple に対する `T` の厳密な typejoin、nested `NTuple` / `Vararg` pattern、および method
ambiguity resolver まで含めた完全な Julia `UnionAll` binding である。

2026-05-20: #4285 で同じ top-level global が異なる storage type に再束縛される代表ケースは、
`global_types` を `Any` に widen して stale typed global load を避けるようになった。
`g = 1; f() = g + 1; g = 1.5; f()` は本家互換に `2.5` を返し、単一代入 `const`
global の代表ケースは実行と反射 API の両方で `Int64` 精度を保つ。さらに `const x = 1; x = 1.5` は
invalid assignment として元の値を保持する。残りは本家 `bindinginvalidations.jl` 相当の
binding dependency tracking、`const x = ...` による明示的な const 再定義警告/invalidations、
world-age aware method cache、module-qualified binding partition である。

2026-05-20: #4284 で full-form `@generated function ... end` syntax は lowering で受理し、
one-line `@generated f(...) = ...` syntax も受理するようになった。`where` で束縛済みの
type/value parameter を直接返す代表ケースと、`:(x + 1)` / `return :(x * x)` /
`@generated f(x) = :(x + 2)` の単純 quoted expression body は本家互換になった。
直接 `T` を返す generated body の reflection は、具体 signature では `Type{Int64}`、
抽象 signature では `Any` に戻る本家の代表挙動に合わせた。
残りは本家 `generated_body_to_codeinfo` 相当の generated-body purity check、quote returned
expression の包括的な実行コード化、argument names を型として読む generated-body 環境、
method cache / world-age aware invalidation である。
2026-06-05: #5927 で quote returned expression materialization の土台として
runtime `eval` が `Expr(:tuple, ...)` を再帰評価して `Tuple` を返すようになった。
同日 #5928 で `Expr(:vect, ...)` も `Vector{Any}` 相当の runtime array として
再帰評価できるようになった。さらに #5929 で `Expr(:if, cond, then[, else])` も
条件分岐として評価できるようになり、#5930 で `Expr(:curly, T, params...)` も
型構築として `DataType` に materialize できるようになった。#5931 では
`Expr(:string, ...)` も interpolation part の再帰評価 + `string` 委譲で評価できるようになった。
同日 follow-up で `Expr(:ref, ...)` も Julia-compatible な indexing expression として評価できるようになった。
2026-06-07 (#6183/#5936): generated returned Expr eval は `Expr(:return, value_expr)` を
staged result marker として扱い、payload を runtime argument frame 上で評価する。
同日 follow-up (#6185/#5936) で `Expr(:let, binding..., body)` も一時 eval frame 上で
binding を評価し、body にだけ見える local scope として扱えるようになった。
同日 follow-up (#6187/#5936) で `Expr(:call, GlobalRef(Base, :+), args...)` のような
GlobalRef callee も qualified function dispatch へ渡せるようになった。
同日 follow-up (#6190/#5936) で `Expr(:copyast, QuoteNode(ex))` も quoted AST payload を
data として返す eval head として扱えるようになった。
同日 follow-up (#6192/#5936) で `Expr(:comparison, value, op, value, ...)` の chained
comparison tail も左から評価し、Julia と同じ boolean result を返すようになった。
同日 follow-up (#6194/#5936) で `Expr(:elseif, cond, then[, else])` も `:if` と同じ
conditional eval head として扱えるようになった。
同日 follow-up (#6196/#5936) で `Expr(:call, callee, Expr(:parameters, Expr(:kw, ...)), args...)`
も keyword entries を runtime kwargs dispatch に渡す returned-Expr eval path として扱えるようになった。
同日 follow-up (#5936) で `Expr(:block, ...)` / `Expr(:(=), ...)` /
`Expr(:&&, ...)` / `Expr(:||, ...)` / `Expr(:quote, ...)` も generated returned-Expr
eval fixture として固定した。
同日 follow-up (#5936) で issue 本文の `sumn(::Val{N})` 代表再現
（generated body が loop で `Expr` を組み立てて返すケース）も fixture 化し、returned-Expr
compatibility path で `Val(3)==6` / `Val(5)==15` を返すことを固定した。
残りの eval Expr head は #5074 の generated staging 前提 issue 群に継続する。

2026-05-20: #4286 で `@noinline function ... end`、
`Base.@nospecializeinfer function ... end`、`Base.@propagate_inbounds f(x)=...`、
`@inline function ... end`、`Base.@inline f(x)=...`、
`Base.@propagate_inbounds @inline f(x)=...`、`Base.@boundscheck expr`、
仮引数位置の `@nospecialize x` / `@specialize(x)` は
compatibility wrapper として parse/lower/実行できるようになった。
同日 follow-up で statement/expression `@inbounds` が call context を marked call として
callee frame へ渡し、`@boundscheck` block を抑止する direct-call 代表ケースも本家互換になった。
2026-05-21 follow-up で callsite `@inline f(x)` / `@noinline (expr)` と
statement-position `@nospecialize x y` / `@specialize` metadata marker の代表ケースも
compatibility wrapper として fixture 化した。
同日 follow-up で function body 内の bare statement-position `@inline` / `@noinline` marker も
no-op metadata statement として受理するようにした。
同日 follow-up で `Base.@constprop :aggressive function ... end`、
`Base.@constprop :none f(x)=...`、`Base.@assume_effects :foldable function ... end`、
`Base.@assume_effects :terminates_locally f(x)=...`、metadata-only statement marker、
callsite `Base.@assume_effects :foldable f(x)` の代表ケースも本家互換の no-op wrapper として
受理するようにした。
2026-05-22 follow-up で Lazy AoT `CallSpecialize` 経路にも `@inbounds` frame context を
伝播する `CallSpecializeInbounds` を追加し、特殊化対象でも `@boundscheck` block を抑止する
代表ケースを fixture 化した。
同日 follow-up で statement-position と wrapped target の `@inline` / `@noinline` /
`@nospecialize` / `@specialize` / `Base.@constprop` / `Base.@assume_effects` marker は
`nothing` 式ではなく `Stmt::Meta` として明示保持するようにした。現時点では VM/AoT 実行は
no-op として扱う。
同日 follow-up で function body 内の `@inline` / `@noinline` marker は AoT inliner の
`AotFunction.inline_policy` へ消費され、`@noinline` 抑止と非再帰 `@inline` の size-limit
override は本家 optimizer metadata に近い代表 slice になった。metadata marker 自体は
AoT statement へ変換せず、実行時 no-op のまま保持/消費する。
追加 follow-up で wrapped top-level definition の `@inline function ... end` /
`@noinline function ... end` も function body の `Stmt::Meta` へ移され、
`Program.functions` 経由で AoT `inline_policy` まで届くようになった。
追加 follow-up で expression-position callsite `@inline f(x)` / `@noinline f(x)` は
retained wrapper を経由して AoT direct `CallStatic.inline_policy` まで届き、
callsite policy が definition policy より優先される代表 slice になった。nested
callsite annotations は innermost precedence を固定している。
さらに wrapped expression 内の direct static calls へ callsite policy を再帰適用し、
`@inline (f(x) + g(x))` と `@inline (@noinline f(x) + g(x))` の代表 AoT slice を固定した。
outer policy は lambda body へは伝播しない。
残りは本家同様の
`Expr(:meta, :noinline)` / `Expr(:meta, :nospecializeinfer)` /
`Expr(:meta, :aggressive_constprop)` / `Expr(:meta, :no_constprop)` /
`Expr(:meta, :assume_effects)` / `Expr(:boundscheck, ...)` metadata retention、
metadata の function / callsite semantic consumption、specialization/cache-key への反映、
dynamic / non-static callsite annotations と full statement-level metadata を含む inline/noinline/constprop optimizer decisions、
assumed-effects purity/effect checks、callsite annotation precedence、non-call / nested call の
inbounds 伝播、nested annotation combinations の完全互換である。
2026-05-30 follow-up (Issue #5122): 短縮形(代入形)関数定義の引数位置
`f(@nospecialize(x)) = ...` / `g(@nospecialize(x::Number)) = ...` /
`k(@specialize(x)) = ...` も受理できるようになった。完全形
`function f(@nospecialize x) ... end` はパーサが平のパラメータへ展開して既に
動作していたが、短縮形では注釈が `ArgumentList` 内の `MacrocallExpression` として
残り `parse_parameter` がパラメータを黙って落としていた。`parse_parameter` に
`MacroCall` 分岐(`@nospecialize`/`@specialize` を内側パラメータへ展開、型注釈も保持)を
追加して解消。注釈は引き続き実行時 no-op で、推論特殊化(specialization)の
抑制セマンティクス自体は未モデル化のまま。

2026-05-20: #4288 で `Base.return_types` / `Base.infer_return_type` の単一メソッド代表ケース、
`Core.Compiler.return_type` alias、`Base.code_lowered` / `Base.code_typed` の
method-match / return-type snapshot 代表 surface は VM method snapshot から返せるようになった。
`Vector{Int64}` direct parameter return、concrete signature の most-specific method filtering、
`Type{T}` return binding、`Core.Compiler.return_type(Tuple{typeof(f), Args...})` の
signature-tuple form、`+ :: Tuple{Int64,Int64}` builtin tfunc reflection の代表ケースも本家互換にした。
同日 follow-up で production abstract inference の `map` / generator `collect` 代表経路から
`Vector{Int64}` reflection snapshot を保持し、`return_types` / `infer_return_type` /
`code_typed` が `Array` / `Any` に落ちないようにした。
同日 follow-up で anonymous-arrow tuple `map` の lowered `LetBlock` callable も解決し、
`map(x -> x + 1, t::Tuple{Int64,Float64})` の representative reflection snapshot が
本家同様 `Tuple{Int64,Float64}` になるようにした。
同日 follow-up (#4296) で untyped method の representative reflection-time specialization を追加し、
`foo(a)=a+1; Base.return_types(foo, Tuple{Int64})` は本家同様 `Int64` を返すようになった。
同日 follow-up で小さい `Union` 引数の reflection method lookup を branch-specific に split し、
`Base.return_types(f, Tuple{Union{Int64,String}})` / `Core.Compiler.return_type` は
本家代表ケースと同じ `Union{Int64,String}` を返すようになった。さらに
Union-annotated parameter を abstract inference 初期環境で保持し、Union 引数 caller の
representative return snapshot も `Union{...}` になった。
同日 follow-up で `Base.code_typed(...; interp=nothing)` は accepted no-op、
`interp=1` は本家同様 `Expected AbstractInterpreter` error として扱うようになった。
同日 follow-up で ternary expression も conditional narrowing を使うようにし、
`x::Union{Int64,String}` の `x isa Int64 ? ... : ...` 代表ケースは本家同様 `Int64` を返す。
同日 follow-up で builtin reflection slice に `string(::Int64) -> String`、
`length(::Vector{Int64}) -> Int64`、`getindex(::Vector{Int64}, ::Int64) -> Int64`
の代表ケースを追加した。
残りは本家 compiler inference と同じ `CodeInfo` materialization、world / interp /
generated context error、複数 method match
の精密な per-method inference、一般 builtin tfunc table、任意 guard/body の branch-specific
inferred result、
invalidated method cache 追従、および untyped method specialization の完全一般化である。

2026-05-20: #4291 で type-stability report は `inference_provenance` を持ち、
現時点では standalone analyzer 由来で production codegen inference snapshots をまだ使わないことを
JSON/text に明示するようになった。また user-facing line は byte offset ではなく source line を
指すようになり、production global type collection を共有して const global reader の代表ケースは
`Int64` stable と報告する。残りは production compiler path の inferred facts を report へ直接接続すること、
同日 follow-up で overloaded method dispatch の代表ケースも standalone analyzer の
inference-only method table に接続し、`caller(x::Int64)=f(x)` が `f(::Integer)` method を選んで
`Concrete Int64` stable と報告するようになった。残りは production compiler path の inferred facts を
report へ直接接続すること、array / tuple / generator / closure / broader dispatch の return-type
behavior と diagnostic output を同一 fixture で突き合わせることである。
同日 follow-up で qualified nested functions と caller environment を standalone analyzer /
production reflection inference の両方へ接続し、captured local closure を呼ぶ generator body の
代表ケースは `Vector{Int64}` と報告するようになった。
2026-05-21 follow-up: returned arrow / named nested closures は captured value types を
abstract inference に保持し、caller 側の変数名が異なる local call と returned composed
closure を含む代表ケースで、runtime と `Base.return_types` / `Base.infer_return_type` の両方が
本家同様 `Int64` を返すようになった。opaque closure entry point は
`Base.Experimental.@opaque` / `Core.OpaqueClosure` とも Issue #4289 の explicit unsupported
diagnostic に揃えた。2026-05-21 follow-up で Issue #4309 の static top-level
`T = typeof(f)` alias から `(::T)(...)` method を追加する代表 closure object case は
`f` の method table に接続した。残りは runtime world-age を伴う任意 method 追加、
method table identity の完全 introspection、および no-JIT/iOS VM では表現しない
full opaque-closure semantics である。

2026-05-20: #4282 で runtime `kwargs...` の `Value::Pairs` は
`eltype(kwargs) == Pair{Symbol, T}` と `collect(kwargs)::Vector{Pair{Symbol,T}}` を
homogeneous value types で保持するようになった。さらに representative heterogeneous
kwargs として `Int64`/`Float64` は `Pair{Symbol, Real}`、non-numeric mixed values は
`Pair{Symbol, Any}`、empty kwargs は `Pair{Symbol, Union{}}` を返すようになった。
`typeof(kwargs)` も runtime names / values から
`Base.Pairs{Symbol,V,Nothing,@NamedTuple{...}}` 形式へ投影する。残りは
string-backed projection ではない structural `Base.Pairs{K,V,I,A}` identity、
full lattice coverage for all heterogeneous keyword value joins、Pure Julia
`Pairs{K,V,I,A}` wrapper と VM-native `Value::Pairs` の表現統合である。

2026-05-20: #4281 で `zip(a, b, c, d, e)` / `zip(a, b, c, d, e, f)` は
`Zip5` / `Zip6` Pure Julia wrapper として
`iterate` / `length` / trait / `eltype` / `size` / `axes` に対応し、
`collect(zip(...))` は precise 5/6-tuple element type を保持するようになった。残りは
本家 `zip` と同じ arbitrary arity varargs generalization、および `Zip`, `Zip3`,
`Zip4`, `Zip5`, `Zip6` の重複を shared n-ary representation に畳むことである。

2026-05-20: #4280 で `hcat(args...)` の supported homogeneous typed vectors は
4+ 引数でも typed seed vector から `Matrix{T}` を構築し、`Int8` / `Float32` と
代表 `Int64`/`Float64` promotion は本家に寄った。残りは Julia 本家の
`hcat(V::Vector{T}...) where T` と同じ任意 `T` / full promotion path、および unsupported
element type を Rust fallback ではなく Pure Julia dispatch で扱う一般化である。

2026-05-20: #4283 で tuple `map(f, t::Tuple)` と 2 tuple / 3 tuple /
representative 4 tuple
`map(f, t1::Tuple, t2::Tuple, ...)` は small arity tuple literal return により shape と
per-slot result type を保持するようになった。残りは upstream `tuple.jl` の 5+ tuple
varargs map と larger tuple fallback の完全な type-preserving generalization である。

2026-05-20: #4289 で captured nested function を `map(f, [1,2,3])` と
`collect(Base.Generator(f, 1:3))` に渡す代表ケースは、local callable を bare function
index に置き換えず runtime closure value として扱うようになり、unfiltered
generator expression `collect(f(x) for x in 1:3)` の non-empty 代表ケースも生成値から
`Vector{Int64}` を保持し、filtered generator / comprehension の local callable body も
non-empty `Vector{Int64}` と all-filtered-out `Vector{Union{}}` を保持するようになった。
さらに full-form function body の `map(x -> x + a, [1,2,3])` は anonymous function argument
を nested `FunctionDef` として扱い captured `a` を保持する。assigned anonymous function
`f = x -> x + a; map(f, xs)` も同じ nested `FunctionDef` path で本家代表ケースに合わせた。
`Base.Generator(x -> x + a, 1:3)` のような module-qualified call argument 内の nested
`FunctionDef` も収集し、直接匿名 generator closure の代表ケースも本家に合わせた。
同日 follow-up で reflection / abstract inference でも unfiltered generator expression 内の
captured local callable を解決し、`Base.return_types` / `Base.infer_return_type` が
`Vector{Any}` ではなく `Vector{Int64}` を返す代表ケースを固定した。
2026-05-21 follow-up で returned closure direct/local call、renamed caller capture、
returned named nested closure call、simple composed function call、returned composed closure の
representative reflection inference も本家同様 `Int64` に揃えた。
同日 follow-up で typed parameter を持つ captured local function も reflection-time
specialization function table に残し、`f(x::Int) = x + a; f(2)` の
representative `Base.return_types` / `Base.infer_return_type` も本家同様 `Int64` に揃えた。
2026-05-21 follow-up で runtime closure value を直接 `Base.return_types` /
`Base.code_typed` に渡す代表ケースも closure captures を反映して `Int64` に揃えた。
同日 follow-up で `Base.Experimental.@opaque` / `Core.OpaqueClosure` は generic unknown
macro/module error ではなく Issue #4289 の explicit unsupported diagnostic に揃えた。
2026-05-21 follow-up で `f = x -> x + 1; T = typeof(f); (::T)(x::String) = ...`
の代表 closure object method は本家同様 dispatch に参加するようにした。
残りは compile-time に見えない method 追加、method table introspection の完全互換、
および no-JIT/iOS VM では unsupported とする full opaque-closure semantics である。

2026-05-19: #4018/#3954 follow-up で 2-D shaped collect allocation helper
`_array_for_inner_shape(T, dims)` は direct `Array{T}(undef, dims...)` branch ではなく
`similar(Array{T}, dims)` を使うようになった。残りは 0-D / 1-D / 3-D+ の
compatibility constructor branches と `Vector{T}(undef, len)` など HasLength path、
public `collect` / `similar` のさらに外側を Memory-backed Array wrapper dispatch へ
統一することである。

2026-05-19: #4052 follow-up で statically known `collect(zip(...))` は
Pure Julia method dispatch を Rust native Zip collect fallback より先に見るようにした。
同日 #3910 follow-up で runtime `Any` 経由の `collect(x)` も Zip user/Pure Julia
method candidates を native collect fallback より先に shared resolver scoring で選ぶようにした。
同日 #4052 follow-up で runtime-`Any` 経由の Enumerate / Rest user method
extension も fixture で固定した。同日 follow-up で native collect fallback surface は
`CollectFallback:` inventory と `scripts/check_collect_fallback_inventory.sh` による
監査へ移し、残る VM-native `Generator` / range representation boundary は
#4265 / #4266 に分割した。

さらに同日 follow-up で runtime `Any` 経由の `Enumerate` / `Rest` も field values
から supported concrete type parameters を復元し、module-qualified parametric user
methods を shared resolver で選ぶようにした。残りは native `Generator` / range
representation boundary、fallback sentinel inventory の縮小、および generic `_collect`
trait pipeline への統一である。

2026-05-19: #3910 follow-up で sjulia extension の callable `GlobalRef(Base, :f)` は
Rust builtin fallback より前に shared callable resolver で user/Pure Julia methods
を見るようにし、non-Base GlobalRef も first method ではなく runtime argument dispatch
を使うようにした。さらに同日 follow-up で dynamic `T(x)` の DataType callable と
legacy `CallTypeConstructor` は pre-dispatch native primitive conversion ではなく
shared callable resolver を先に見るようにした。`IterateDynamic` は同日 follow-up で
`iterate(collection, state)` の full runtime signature scoring と full-signature
cache key に移行した。さらに Generator / `Iterators.map` の DataType callable は
legacy TypeObject conversion fast path ではなく runtime callable dispatch を使うようにした。
さらに `CallDynamicBinaryBoth` の binary primitive fallback policy は明示的な
`PrimitiveFallbackFirst` / `SharedResolverFirst` 判定に集約し、non-primitive both-`Any`
operands は shared resolver を Rust fallback より先に見るようにした。残りはこの
policy 境界を arithmetic execution ladder 全体の compatibility inventory として監査し、
primitive intrinsic fallback を Pure Julia arithmetic methods へ段階的に縮小できるかを
Issue #4262 で扱うことである。

2026-05-19: #3911 の follow-up として membership aliases
`∈` / `∉` / `∋` / `∌` は callable syntax と `Base.:op(...)` syntax で
`Base.in` dispatch を見るようになった。残りは upstream の `const ∈ = in`
と同じ function object identity、one-arg `Fix2` forms、より一般的な operator
call syntax、および user method 追加後も fallback `Base.in(::Any, ::Tuple)` を
誤選択しない shared resolver (#3910) へ広げることである。

2026-05-19: #4239 で explicit `Memory{Real}` / `Memory{Integer}` /
`Memory{AbstractFloat}` などの abstract numeric constructor は boxed abstract
element tag を保持するようにした。残りは `Vector{Real}(undef, n)` など Array 側の
compile-time abstract constructor path と、typed `setindex!` API 経由のすべての
boxed abstract storage 代入経路を Julia 本家 `array.jl` / `genericmemory.jl` に寄せることである。

2026-05-19: #4236 で VM-native generator collection の non-empty path は
`EltypeUnknown` collection に近い typejoin materialization へ寄せた。
`collect(Base.Generator(identity, (1, 2.0)))` と
`collect_similar(::Memory, ::Generator)` の runtime `Memory{Real}` allocation path は
boxed values を保持する。残りは同種の boxed abstract storage 代入経路を Julia 本家
`genericmemory.jl` / `array.jl` に寄せることである。

2026-05-19: #4052/#4018 follow-up で `map(f, ::Memory)` は upstream
`./julia/base/abstractarray.jl` の `map(f, A::AbstractArray)` と同じく
`collect_similar(A, Generator(f,A))` を使い、supported concrete / widened / empty
results で `Memory` container を保持するようにした。残りは `Memory` 以外の
Pure Julia Array wrapper で同じ `AbstractArray` method を自然に共有すること。
generator collection を VM 二段階 materialization 境界からさらに縮小する作業は
#4265 に分割した。

2026-05-18: #4166 で `Value` enum size audit は現在の `Value::DataType(JuliaType)`
projection を前提に 112 bytes 上限へ更新した。これは #3909 の runtime type-object
identity/layout handle 化までの transitional boundary であり、DataType projection を
registry handle へ移した後に再び compactness 上限を下げる。
2026-05-19: #3909 の follow-up として supported `DataType` layout metadata は
`RuntimeTypeRegistry` に集約した。`sizeof(::DataType)` / `isbitstype(::DataType)` /
`fieldnames(::Type)` / `fieldtypes(::Type)` は built-in と user struct の両方で registry
read model を使う。残りは `Value::DataType(JuliaType)` projection 自体を stable
registry handle に置き換えること、full `UnionAll` / `TypeVar` object identity と
parametric/recursive/opaque layout identity を持つことである。
同日 follow-up で `hasfield(T, name)` の supported `Value::DataType` / type-name
path も `RuntimeTypeRegistry` field metadata を読むようにした。これにより
`LineNumberNode` / `Expr` / `QuoteNode` / `GlobalRef` の builtin layout fields は
`fieldnames` / `fieldtypes` と同じ registry source に揃った。残りは `hasfield`
全体の strict upstream signature parity、runtime type objects の stable handle 化、
UnionAll / TypeVar / parametric layout identity の完全化である。
同日 follow-up で `fieldoffset(T, i)` / `fieldoffset(T, name)` を supported
`RuntimeTypeRegistry` layout metadata から返すようにした。残りは opaque /
recursive / parametric layout identity、runtime type objects の stable handle 化、
および Julia 本家の full `DataType` layout semantics との完全一致である。
同日 follow-up で supported runtime type objects の `typeof` は
`RuntimeTypeRegistry` kind projection を読み、`DataType` / `UnionAll` / `TypeVar`
を区別するようにした。残りは `Value::DataType(JuliaType)` projection 自体を stable
registry handle に置き換えること、full `UnionAll` / `TypeVar` object identity、
type-cache canonicalization、opaque / recursive / parametric layout identity である。
2026-05-19: #4172 で `scripts/check_value_array_allowlist.sh` の
`type_ops/iteration.rs` 上限を現状の 35 件に固定し直した。これは iterator/collect
VM-native materialization boundary の現状監査であり、#3908 / #4011 / #4021 で
Pure Julia Array wrapper / Memory primitive へ移した slice ごとに count を下げる。
同日 #4011 follow-up で `type_ops/iteration.rs` ceiling は 35 から 33 に下がった。
残りは ProductIterator / collect / generator materialization で返している transitional
`Value::Array` wrapper construction をさらに Pure Julia Array wrapper / Memory-backed
container path へ移すことである。
さらに同日 follow-up で stale ceiling を現在値へ同期し、`repl/session.rs` は 3→2、
`vm/exec/array_basic.rs` は 20→19、`vm/exec/array_mutate.rs` は 13→12、
`vm/hof_exec/value_mode.rs` は 12→10 に下がった。残りは各ファイル内の現存
compatibility boundary 自体を Pure Julia dispatch / Memory primitive へ移すことである。
2026-05-19: #4021 の host/cache boundary shrink slice として、`REPLGlobals` の
dedicated `array_vars` / `typed_array_vars` maps を削除し、persisted `Value::Array` は
`other_vars` catch-all で保持するようにした。`repl/globals.rs` の `Value::Array`
allowlist ceiling は 3 から 1 に下げた。残りは `repl/session.rs` の injection /
type-hint boundary、FFI/display/frame/host formatting の Array presentation 境界を
分類縮小することである。
同日 follow-up で `compile_and_run_with_output` の result presentation は
`ffi::format::format_value` に一本化し、Array result の Rust debug branch を削除した。
`ffi/basic.rs` の `Value::Array` allowlist ceiling は 2 から 1 に下げた。残りは
`repl/session.rs` の injection / type-hint boundary、`ffi/format.rs` と
`vm/formatting.rs` の Array presentation implementation、frame / host formatting 境界を
分類縮小することである。
同日 follow-up で `StoreAny` と host/frame parameter binding 経由の runtime
`Value::Array` は `locals_any` に保存するようにした。typed `StoreArray` / `LoadArray`
用の `locals_array` map は cache/typed-instruction boundary として残す。残りは
typed `locals_array` map 自体の段階的縮小、`repl/session.rs` injection / type-hint
boundary と display/formatting 側の presentation boundary である。
同日 follow-up で VM `value_to_string(::Value::Array)` は独自 `to_value_vec` path を
やめ、index-based `format_array_value` helper を再利用するようにした。さらに
FFI formatter は shared VM compact formatter に委譲し、`value_to_julia_code` も
index-based array helper を再利用するようにした。`ffi/format.rs` の `Value::Array`
allowlist ceiling は 4 から 1 に下げた。残りは `repl/session.rs` の injection /
type-hint path、`bin/sjulia.rs` host formatting、frame / host formatting 境界を
本家 `show` / `arrayshow` に寄せることである。
2026-05-19: #3908/#4018 の typed undef constructor slice として
`AllocUndefTyped*` instruction path は `ArrayValue::memory_first_undef` へ寄せた。
同日 follow-up で untyped numeric array literal builder `NewArray` も
`ArrayValue::memory_first_with_capacity` へ寄せ、literal audit の対象に含めた。
さらに slicing result builder は `ArrayValue::memory_first_slice_from_values` へ寄せた。
さらに Any-typed receiver 経由で `BuiltinId::Similar` に落ちる Array/Memory `similar`
は direct Rust allocation の前に `similar` / `Base.similar` の method table を引く。
残りは `Value::Array` が semantic owner のまま残る mutation / view / broadcast /
host/cache/display boundary と、method が見つからない場合の cache/bootstrap
compatibility fallback を Memory primitive + Pure Julia Array wrapper に順次移し、
allowlist count を下げること。
2026-05-19: #4052/#3954 の follow-up として `EltypeUnknown + SizeUnknown`
collect は upstream-shaped `grow_to!(_similar_for(...), itr)` へ寄せ、
`push_widen` / `setindex_widen_up_to` を Pure Julia helper として分離した。さらに
`collect_similar(cont::Memory, itr::Generator)` は requested Memory container を尊重して
supported homogeneous vector / matrix / empty generator results を
`similar(::Memory, ::Type, len)` / `_memory_similar_dims(T, dims)` に再配置するようにした。
残りは `@default_eltype` 相当の inference-backed empty iterator eltype と、
generator collection 全体を二段階 materialization ではなく直接 container allocation
pipeline に載せることである。heterogeneous generator collection が `Memory{Real}` /
`Vector{Real}` ではなく `Float64` に寄る既存バグは #4236 に分離した。

## LinearAlgebra Follow-up Notes

2026-05-19: #4020 の representative dispatch-first slice として、compile-time known
`Array * Array` は concrete Matrix `*` method candidates が存在する場合に
`Instr::MatMul` へ直接落とさず、`CallDynamicBinaryBoth` で user/Pure Julia method を
先に試すようにした。同日 follow-up で Matrix/Vector/AbstractMatrix/AbstractVector
method candidates も dispatch-first 対象に含め、matrix-vector overloads も Rust
`Instr::MatMul` fallback より先に試すようにした。残りは vector-matrix / adjoint /
transpose cases、Rust numerical kernel boundary の分類、`Value::Array` allowlist count
の縮小である。
同日 follow-up で local `LinearAlgebra.jl` に upstream-shaped
`Base.:*(x::AbstractVector, A::AbstractMatrix)` を追加し、supported vector-matrix
case も stdlib dispatch path に載せた。`reshape(x, length(x), 1) * A` そのものは
現状の reshape result shape inference に引っかかるため、supported behavior を Pure
Julia loop で表現している。さらに stdlib method が binary-op compile candidate に
まだ入らない境界を補うため、Rust `MatMul` fallback の left-vector interpretation も
upstream shape に合わせた。残りは `using LinearAlgebra` method の compile candidate
統合、reshape-backed implementation への置換、adjoint / transpose cases、Rust
numerical kernel boundary の分類、`Value::Array` allowlist count の縮小である。
同日 follow-up で `builtins_linalg.rs` の real-valued result builders は
`ArrayValue::memory_first_from_f64` / `memory_first_from_i64` へ寄せた。残りは
complex interleaved eigenvalue/eigenvector result builders も
`ArrayValue::memory_first_with_capacity(ArrayElementType::ComplexF64, ...)` と
`ArrayValue::push(Value::complex_struct(...))` 経由に移した。残りは public
LinearAlgebra API wrapper の dispatch-first 化、Rust numerical kernel boundary の
分類縮小である。
同日 follow-up で `rank(A::Array)` / `rank(x::Number)` は stdlib Pure Julia method
dispatch に移し、public `Base.LinearAlgebra.rank` direct route は外した。残りは
`rank` keyword arguments (`atol`, `rtol`) と `svdvals` public wrapper、`AbstractMatrix`
/ `AbstractVector` signature parity、Rust `BuiltinId::Rank` handler の cache
compatibility inventory 化、および他の LinearAlgebra public APIs (`cond`, `qr`,
`eigen`, `cholesky` など) の同様の wrapper dispatch-first 化である。

## HOF / Generator Runtime Boundary Follow-up Notes

2026-05-19: #4052 の collect dispatch-first slice として、statically known
`collect(enumerate(...))` / `collect(rest(...))` は Rust `RangeCollect`
shortcut より先に Pure Julia method dispatch を通るようにした。残りは
VM-native `Zip` / Generator 内部 collect compatibility boundary を Julia Base の
`_collect` trait pipeline に寄せることである。
同日 #3910 follow-up で runtime `Any` 経由の `collect(x)` candidate selection は
Zip user/Pure Julia methods を先に選ぶようになった。さらに #4052 follow-up で
runtime-`Any` Enumerate / Rest extension も user/Pure Julia methods を先に選ぶことを
fixture 化した。残りは VM-native Generator / range fallback sentinel の縮小と、
native collect compatibility boundary 全体を Julia Base の `_collect` trait pipeline
へ寄せることである。
同日 follow-up で direct
`Base._collect(::Memory, ::Generator, ::EltypeUnknown, ::HasLength/HasShape)` は
supported `collect_similar(::Memory, ::Generator)` adapter に接続した。残りは
VM-native Generator を通常 struct field access として扱える表現へ寄せること、
`@default_eltype` 相当の inference を空 iterator 一般に広げること、SizeUnknown
Memory collect の固定長/grow boundary を Julia Base と同じ形で整理することである。

2026-05-19: #4019 の representative Array/HOF slice として、本家
`./julia/base/abstractarray.jl` の 2-source `map!` に合わせた Pure Julia
`map!(f, dest::Array, A::Array, B::Array)` を追加した。残りは N-argument
`map!`、broadcast materialization、reduction family を Pure Julia dispatch-first
route へ移し、対応する Rust `Value::Array` fast path / allowlist entry を下げること。
同日 follow-up で supported 3-source `map!(f, dest::Array, A::Array, B::Array, C::Array)`
も Pure Julia method として追加した。残りは arbitrary source arity の varargs
`map!(f, dest, As...)` / `map_n!` generalization と、broadcast materialization、
対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で supported 4-source
`map!(f, dest::Array, A::Array, B::Array, C::Array, D::Array)` も追加した。
残りは固定 arity 増設ではなく upstream の `map_n!` に近い arbitrary source arity
generalization、5+ argument `zip` / splatting support、broadcast materialization、
reduction family、対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
2026-05-22 follow-up で supported fixed arity の 5-source
`map!(f, dest::Array, A::Array, B::Array, C::Array, D::Array, E::Array)` を追加し、
runtime `Any` 経由の binary `map(f, A, B)` が generic `(Function, Any, Any)`
Base fallback に固定されず、同一 arity のより具体的な user method へ
`CallTypedDispatch` で戻れるようにした。残りは 6+ source や splatting を含む
arbitrary vararg `map_n!` generalization、broadcast materialization、reduction family、
対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で `map!(f, dest::Array, As::Array...)` を追加し、6+ source と
source tuple splat は Pure Julia dispatch に入るようになった。supported hot arity は固定
callback call、さらに広い arity は per-index value splat fallback を使う。残りは
broadcast materialization、reduction family、対応する Rust `Value::Array` fast path /
allowlist entry の縮小である。
2026-05-26 follow-up で binary `map(f, A::Array, B::Array)` の Pure Julia fallback は
`[]` / `Vector{Any}` result ではなく最初の mapped value の runtime type から typed
result を確保するようにした。`map(+, ...)` / `map(*, ...)` の representative typed
Vector paths は singleton-function specializations で upstream Julia と同じ result type を
返す。typed results が `Vector{Any}` に広がる bug は Issue #4610 として報告し、
ローカル修正済み。
同日 follow-up で `map(f, A::Array, B::Array, C::Array, As::Array...)` の
Pure Julia fallback も追加し、3+ source Array map は scalar `map` 誤選択ではなく
typed result allocation path に入るようにした。`map(+, ...)` の representative typed
Vector vararg paths も upstream Julia と同じ result type に合わせた。作業中に見つけた
3-source `map` lowering bug は Issue #4611、n-ary small integer `+` gap は
Issue #4612、Int8 overflow saturation は Issue #4613 として報告済み。残りは
broadcast materialization、reduction family、対応する Rust `Value::Array` fast path /
allowlist entry の縮小である。
同日 follow-up で `prod(A; dims=1/2)` の result allocation は `ones(...)` から
first reduced value の runtime type に基づく typed allocation に移した。small signed
integer は `Matrix{Int64}`、small unsigned は `Matrix{UInt64}`、String は
`Matrix{String}` result を返す。`prod(; dims)` が `Matrix{Float64}` を確保し、
String matrix で `operator(Float64, String)` に落ちる bug は Issue #4614 として
報告し、ローカル修正済み。残りは broadcast materialization、未移行の reduction
family、対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で scalar `prod(arr)` も unconditional `Int64(1)` accumulator から
typed vector methods に移し、small signed integer は `Int64`、unsigned integer は
`UInt64`、`Float32` / `Bool` / `String` はそれぞれ同型 result を返すようにした。
typed empty products も upstream identity type に合わせた。`prod(arr)` が `UInt8` /
`Bool` result type を壊し、String product で `operator(Int64, String)` に落ちる bug は
Issue #4615 として報告し、ローカル修正済み。残りは broadcast materialization、
未移行の reduction family、対応する Rust `Value::Array` fast path / allowlist entry
の縮小である。
同日 follow-up で `prod!(r, A)` の row/column in-place reduction も
`Float64(1.0)` accumulator から shared typed product helpers に移した。String matrix
input は `Matrix{String}` / `Vector{String}` destination に product values を書き込み、
Bool matrix input は `Matrix{Bool}` destination に logical product values を保持する。
`prod!` が String matrix で `operator(Float64, String)` に落ちる bug は Issue #4616
として報告し、ローカル修正済み。残りは broadcast materialization、未移行の reduction
family、対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で `sum!(r, A)` の row/column in-place reduction も `Float64(0.0)`
accumulator から typed sum helpers に移した。Bool / small integer source の
upstream-compatible reduction values を計算し、Bool destination へ 2 以上の sum を
保存しようとする場合は `true` に潰さず例外を投げる。`sum!` が Bool destination で
multi-count sum を `true` に coerces する bug は Issue #4617 として報告し、
ローカル修正済み。残りは broadcast materialization、未移行の reduction family、
対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で scalar `sum(arr)` も typed eltype branches に移し、Bool /
small signed integer は `Int64`、unsigned integer は `UInt64`、`Float32` は
`Float32` result を返すようにした。typed empty sums も upstream identity type に
合わせた。scalar `sum` が empty Bool/Int8/UInt8 と non-empty UInt8 で upstream と
異なる result type を返す bug は Issue #4618 として報告し、ローカル修正済み。
残りは broadcast materialization、未移行の reduction family、対応する Rust
`Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で 4-argument `broadcast` / `broadcast!` entry points と
sample element inference を local `Broadcasted` materialization path へ接続した。
残りは arbitrary callback arity の broad coverage、dimension-aware broadcast、
reduction family、対応する Rust `Value::Array` fast path / allowlist entry の縮小である。
同日 follow-up で supported `Vector + Vector` / `Vector - Vector` は
`base/arraymath.jl` の Pure Julia methods に移し、typed Vector call では user methods
が Rust Array arithmetic fallback より先に勝つようにした。残りは Matrix /
AbstractArray / scalar-array arithmetic、broadcast-preserving zero-dimensional behavior、
shape promotion parity、および runtime `Any` Array arithmetic fallback の縮小である。
同日 follow-up で HOF value-mode の generator / runtime callable result builders は
direct `TypedArrayValue::new(ArrayData::...)` から `ArrayValue::memory_first_*`
helpers へ寄せた。残りは #4276 の fallback audit として low-level numerical
kernel boundary の `Value::Array` allowlist count を縮小し、arbitrary callback
continuation をより Julia Base の lazy iterator pipeline に近づけることである。

2026-05-19: #3972 validation cleanup で、HOF value-mode callback の `ReturnArray` /
`ReturnStruct` は callback result として集約されるようになり、runtime builtin callable の
immediate result も generator collect に取り込めるようになった。これは VM-native
Generator materialization boundary の修正であり、完全 lazy な arbitrary callback
continuation は既存 generator follow-up の範囲に残る。

## Dispatch Follow-up Notes

2026-05-19: #3911 の public Base route audit slice として、`Base.ncodeunits` /
`Base.codeunit` / `Base.codeunits` は `BASE_FUNCTION_ROUTES` の dispatch-first marker に登録した。
user method は `compile_builtin_string` の Rust fallback より先に選ばれる。残りは
string direct builtin routes 全体の分類強化、audit script が string-based
`compile_builtin_string` arms も public route registry と照合すること、boundary-only
handlers の inventory 縮小である。
同日 follow-up で `scripts/check_base_routing_registry.sh` は
`compile_builtin_string` の direct `Instr::CallBuiltin` emitter を route-classified /
explicit exemption に分類する監査を追加した。残りは exemption 側に残る
`bitstring` / `codepoint` / `isnumeric` / `parse` / `tryparse` /
`unescape_string` などを dispatch-first または boundary-only inventory として
順次 `BASE_FUNCTION_ROUTES` に移し、Pure Julia/user methods が勝つ範囲を広げること。
さらに follow-up でこれら public string fallbacks は `BASE_FUNCTION_ROUTES` に
分類済みになった。`_regex_replace` / `_substring_retag` の internal helper exemption は
文書化が audit で必須になった。続く fixture slice で `bitstring` / `codepoint` /
`isnumeric` / `unescape_string` / `parse` / `tryparse` も Base-qualified user methods が
fallback より先に選ばれることを確認し、#4209 で primitive fallback 用の
`is_base_function` inventory も揃えた。残りは string 以外の string-based direct routes が
増えないように audit を維持し、必要に応じて他 public route の実動 fixture coverage を
広げることである。
2026-06-01: #5115 で `<:` / `>:` / `isa` の一級関数値化を実装(`(<:)(A, B)`、
`f = (<:); f(A, B)`、`Base.:(<:)` / `Base.:(>:)` / `Base.:(isa)`、`filter`/`map`
述語内)。`builtins.rs from_name` に `<:`、`vm/builtins_types.rs` に `SupertypeOp`
ハンドラ、parser/lowering で引用演算子フィールド名(`Base.:(op)`)を解決。残りは
upstream の `Fix2` 風 1 引数 curry 形式だが、`<:`/`>:`/`isa` は本家でも 2 引数必須
(`(<:)(Number)` は `ArgumentError`)なので curry は対象外。

2026-05-19: #3911 follow-up で `∈` / `∉` / `∋` / `∌` は
`BASE_FUNCTION_ROUTES` の dispatch-first marker に登録し、supported infix syntax は
user `Base.in` extension を経由する fixture で固定した。残りは parser が
`Base.:∈(...)` / `Base.:(∈)(...)` の function-call syntax を受けることと、
upstream `Fix2` one-arg forms (`∉(itr)`, `∋(x)`, `∌(x)`) の表現である。
同日 follow-up で `keytype` / `valtype` も `BASE_FUNCTION_ROUTES` の
dispatch-first route に移し、`BuiltinOp::Keytype` / `BuiltinOp::Valtype` は primitive
fallback としてのみ残した。さらに `eltype` も dispatch-first route に移し、
`BuiltinOp::Eltype` は primitive Array/Memory/Tuple/Range fallback として残した。
さらに `sizeof` も upstream `./julia/base/essentials.jl` の public wrapper に合わせて
dispatch-first route に移し、`BuiltinOp::Sizeof` は primitive layout fallback として残した。
さらに `hasfield` も upstream `./julia/base/runtime_internals.jl` の public method に合わせて
dispatch-first route に移し、`BuiltinOp::Hasfield` は #3909 の registry metadata fallback
として残した。残りは `typeof` / `isa` のような Core/runtime boundary と、
`isbits` / `isbitstype` / `ismutable` など layout-adjacent public routes の
dispatch-first 化可否を本家実装単位で精査することである。
同日 follow-up で `isbits` / `isbitstype` / `ismutable` も upstream
`./julia/base/runtime_internals.jl` の public methods に合わせて dispatch-first route に
移し、Rust handlers は primitive layout fallback として残した。残りは `typeof` /
`isa` のような Core/runtime boundary、`objectid` / `subtypes` / `hasmethod` などの
reflection/runtime-table boundary の分類精査である。
同日 follow-up で `objectid` も upstream `./julia/base/runtime_internals.jl` の public
method に合わせて dispatch-first route に移し、Rust handler は primitive/object
identity fallback として残した。残りは `typeof` / `isa` のような Core/runtime
boundary、`subtypes` / `hasmethod` などの reflection/runtime-table boundary の分類精査である。
同日 follow-up で `hasmethod` も upstream `./julia/base/reflection.jl` の public
2-arg method に合わせて dispatch-first route に移し、Rust handler は supported
method-table fallback として残した。残りは
historical world filtering を含む完全な `hasmethod` world semantics、`subtypes` ownership、
`typeof` / `isa` のような Core/runtime boundary の分類精査である。
同日 follow-up で `Base.in` も dispatch-first route に移し、
`BuiltinOp::In` は primitive tuple/string/dict/set fallback として残した。残りは
`∈` / `∉` aliases と missing-aware membership semantics を含む operators.jl parity を
別 slice で精査することである。

2026-05-19: #4165 で `Type{Any}` singleton dispatch regression は解決済み。
本家 Julia `./julia/src/gf.c` / `./julia/src/subtype.c` の method specificity に合わせ、
`f(::Type)` / `f(::Type{Any})` は `f(Any)` で `Type{Any}`、`f(Int64)` で bare `Type`
を選ぶ。現状は iterator trait fallback 互換のため transitional broad `Type{Any}` match
を保持し、shared dispatch scoring で非 exact case を降格している。完全な `Type` /
`UnionAll` lattice matching への移行は #3910 / #3909 の resolver/type-object follow-up
で継続する。
2026-05-19: #3910 の instruction migration slices として、
`CallDynamicBinaryNoFallback` と `CallDynamicBinaryBoth` の candidate selection は shared
`dispatch_resolver` helper へ移した。続く slices で generic `CallDynamic` の single-argument
candidate selection と `CallDynamicOrBuiltin` の user-method-before-builtin candidate selection
も同じ shared helper へ移した。さらに `CallTypedDispatch` の user-defined abstract covariant
fallback も typed resolver helper へ移し、native generator collect sentinel / builtin fallback
sentinel / `Value::DataType -> Type{T}` encoding / representation mismatch filters は命令側境界として
維持した。続く slice で `IterateDynamic` の wrapper-family candidate selection も shared
resolver helper へ移した。さらに callable-value multi-candidate selection も shared resolver
helper に移し、VM 固有の type match / exact match だけを callback 境界にした。2026-05-19
follow-up で callable-value single-candidate path も同じ resolver に統合した。同日
follow-up で one-`Any` binary operand の `CallDynamicBinary` candidate selection も
shared resolver helper へ移した。2026-05-22 follow-up で generic `CallDynamic` も
runtime `Value::DataType(T)` を `Type{T}` として encoding し、`Type{T} <: Type{<:Bound}`
subtype fallback を使うようになったため、`Any` 経由の type object は exact
`Type{DispatchDog3910}` method と covariant `Type{<:DispatchAnimal3910}` method を本家同様に
選ぶ。残りは closure/composed-function special cases、keyword/splat 付き callable path の
candidate metadata 統合、method ambiguity / world-age / cache semantics の Julia 本家 parity
である。

## Iterator Wrapper Follow-up Notes

2026-05-19: #4168 で `SkipMissing` は upstream `./julia/base/missing.jl` と同じ
`SizeUnknown` collect path へ寄せた。現状の local `SkipMissing` は non-parametric wrapper
なので、`nonmissingtype(eltype(T))` まで含む full type-level parity は今後の type-object /
iterator trait work に残る。

## Collect Trait Pipeline Remaining Work

2026-05-17: #4052 の typed collect slice として `collect(::Type{T}, itr)` は
`IteratorSize(itr)` dispatch に戻り、`collect(Float64, matrix)` の shape preservation と
`Base.collect_similar([0.0], (1, 2.0))` の tuple widening は本家挙動に寄った。
2026-05-17 follow-up で `collect_type_inference.jl` /
`collect_tuple_type_preservation.jl` の redundant `eltype(collect(...))` assertions は
`typeof(x) === Vector{T}` assertions に置き換え、`iteration::` category は 53s で
pass するようになった。ただし full `collect(itr)` parity はまだ完了ではない。
heterogeneous tuple-of-tuples loop 内の `eltype(collect(c))` timeout は bug #4098 として
分離し、同日 follow-up で解決済み。その後の #4052 slices で supported untyped
`collect(itr)` は Julia Base `_collect` trait pipeline に寄せ、残る VM-native
Generator / Range representation boundary は #4265 / #4266 に分割した。
同日 follow-up で Tuple は `IteratorEltype(::Tuple)=HasEltype()` と Pure Julia
`eltype(::Tuple)` を追加し、本家の known-eltype tuple iterator boundary に寄せた。
default iterator trait と `_similar_for` / `_array_for_inner` /
`collect_to_with_first!` の representative untyped `collect(itr)` path は後続
#4052 slices で解決済み。
同日 follow-up で `_similar_shape` / `_array_for_inner` / `collect_to_with_first!` の
代表 helper、Memory trait/collect、`EltypeUnknown + HasLength/HasShape` widening、
および prelude method merge の signature-preserving 修正 (#4102) は実装済み。
同日 follow-up #4103 で、本家 `./julia/base/generator.jl` の
`Generator(::Type{T}, iter)` と `./julia/base/array.jl` の `@default_eltype`
special-case に相当する typed `Base.Generator(::Type, itr)` support は解決済み。
同日 follow-up #4106 で、本家 `Generator(f, I1, I2, Is...)` の vararg
constructor は non-empty `collect` / runtime `collect(x::Any)` 境界で解決済み。
同日 follow-up #4107 で、直接 `iterate(::Base.Generator)` の function
application は supported cases の値互換として解決済み。完全 lazy な
function-frame continuation への置き換えは #4111 として分割した。
2026-05-18 follow-up #4108 で、empty vararg generator の `@default_eltype`
preservation は supported numeric function-callable cases について解決済み。
2026-05-18 follow-up #4109 で、typed vararg `Base.Generator(::Type, I1, I2, Is...)`
は `Complex{Int64}` などの supported tuple-splat constructor cases について解決済み。
2026-05-18 follow-up #4111 で、function-callable VM-native `Base.Generator` の
direct iteration は eager wrapper ではなく 1-step function-frame continuation に戻した。
2026-05-18 follow-up #4115 で、本家 `./julia/base/iterators.jl` の
`Iterators.map(f, arg, args...) = Base.Generator(...)` に対応し、user-facing lazy API
parity は解決済み。2026-05-18 follow-up #4118 で runtime callable `Base.Generator`
と function-valued `Iterators.map` の supported direct-call parity も解決済み。
2026-05-18 follow-up #4119 で stdlib module function rvalue は qualified identity
を保持し、`Iterators.filter` / `Base.Iterators.filter` なども runtime dispatch できる
ようになった。残りの generator/collect gap は type-based
`IteratorSize(typeof(itr))` / `IteratorEltype(typeof(itr))` dispatch への移行である。
2026-05-18 follow-up #4123 で、module-qualified positional splat calls は本家
`./julia/src/julia-parser.scm` / `./julia/src/julia-syntax.scm` の `...` parse と
`_apply_iterate`-style lowering に合わせ、positional `splat_mask` を保持して runtime
splat dispatch path へ流すようになった。#4123 は keyword splats (`kwargs...` /
`; kw...`) の lowering や `kwargs_splat_mask` semantics は対象外。
2026-05-18 follow-up #4052 slice で `Iterators.take` / `Iterators.drop` は
inner iterator の `IteratorEltype` / `eltype` と wrapper-specific `IteratorSize` を
Pure Julia で持ち、`collect(::Take)` / `collect(::Drop)` は empty result でも
`Vector{T}` を保持するようになった。同日 follow-up で supported type-object
`IteratorSize` / `IteratorEltype` は `Vector{T}` / `Matrix{T}` /
`UnitRange{T}` / `StepRange{T,S}` について本家互換へ寄せ、value range
`IteratorSize` は `HasShape{1}` へ移行した。残りは default
`IteratorSize(::Type)` / `IteratorEltype(::Type)` dispatch 全体の本家互換化である。
2026-05-18 follow-up #4127 で simple unfiltered `f(x) for x in matrix` generator
expression は lazy `Generator` lowering へ移行し、`collect` が `HasShape` matrix shape
を保持する supported case は解決済み。2026-05-18 follow-up #4134 で、本家
`./julia/base/iterators.jl` の lazy `Filter` と `./julia/base/generator.jl` /
`./julia/base/array.jl` の generator collect path に合わせ、`f(x) for x in itr if p(x)`
の named unary function body/predicate slice と `Iterators.filter` lazy construction は
解決済み。残りは arbitrary guard expression / closure body を含む filtered generator の
full lazy lowering である。
2026-05-18 follow-up #4052/#4130 で default `IteratorSize(x)` /
`IteratorEltype(x)` は `typeof(x)` dispatch に寄り、user iterator が
`Base.eltype(::Type{T})` を定義するだけで `collect` の known-eltype path に乗る
supported case は解決済み。同日 follow-up #4131 で `Type{Any}` singleton method
specificity は解決し、`IteratorSize(::Type{Any})` / `IteratorEltype(::Type{Any})` は
workaround なしの多重ディスパッチで本家 `./julia/base/generator.jl` と一致する。
2026-05-18 follow-up #4135/#4137/#4138/#4139 で `Count` / `Take` /
`Flatten` / `Cycle` / `Repeated` / `FlatMap` の代表 iterator-wrapper trait blockers は
本家 `./julia/base/iterators.jl` の `IsInfinite` / `SizeUnknown` collect semantics に寄せ、
`iterators::` category は pass するようになった。2026-05-18 follow-up #4141 で
supported fixed arity `Zip` / `Zip3` / `Zip4` / `Zip5` は本家 `zip_iteratorsize` /
`_zip_min_length` semantics に寄り、finite/infinite mix の `length` と trait-shaped
`collect(zip(...))` は解決済み。2026-05-18 follow-up #4142 で `Rest` /
`Enumerate` の state semantics と iterator trait methods は本家
`./julia/base/iterators.jl` に寄せた。2026-05-18 follow-up #4145 で runtime
`collect(::Enumerate)` / `collect(::Rest)` boundary は本家
`./julia/base/iterators.jl` の state semantics と `./julia/base/array.jl` の
`SizeUnknown` grow collect に寄せて解決済み。2026-05-18 follow-up #4143 で
supported 2-argument `product` の upstream order/traits と `partition` の
length/traits/validation は解決済み。2026-05-18 follow-up #4149 で
`ProductIterator` の supported vararg slice (`product()`, singleton, 3/4-argument
products, and splat dispatch) は本家 `./julia/base/iterators.jl` に寄せた。
2026-05-18 follow-up #4150 で fixed arity branches を generic tuple-state carry loop に
置き換え、5/6 引数 product の order / trait / shape collect behavior は本家互換へ寄せた。
2026-05-18 follow-up #4153 で `map(f, empty)` の supported `DataType` / named function
callables は本家 `./julia/base/abstractarray.jl` / `./julia/base/array.jl` の default eltype
behavior に寄せ、`Vector{Int64}` / `Vector{Float64}` を保持するようになった。
2026-05-18 follow-up #4052 で typed `Array{Tuple{...}}(undef, dims...)` allocation は
`ArrayElementType::TupleOf` を保持するようになり、nonzero-rank `ProductIterator` collect は
`Vector{Tuple{T}}` / `Array{Tuple{...},N}` を返すようになった。同日 follow-up で
rank-0 `collect(product())` も本家 `./julia/base/iterators.jl` / `./julia/base/array.jl`
に合わせて `Array{Tuple{},0}` / `Tuple{}` eltype / scalar value `()` を返すようになった。
2026-05-18 follow-up #4052 で runtime value として束縛された VM-native
`Base.Generator` は `IteratorSize` / `IteratorEltype` と `collect_similar` が
upstream trait behavior に寄り、matrix shape と empty default eltype を保持するようになった。
同日 follow-up で inline `Base.collect_similar(cont, Base.Generator(...))` も同じ
generator collect boundary に戻るようになった。残る VM-native generator
materialization boundary の縮小は #4265 に分割した。
2026-05-19 follow-up #4052 で known-eltype と supported `EltypeUnknown`
`_collect(cont, itr, et, isz)` は
`_similar_for(cont, T, itr, isz, shp)` を経由するようになり、supported `LinRange` /
`StepRangeLen` / `OneTo` / `LogRange` collect は trait pipeline に戻った。残りは
public VM-native range boundary のさらなる縮小 (#4266) と VM-native generator
boundary のさらなる縮小 (#4265) である。
同日 follow-up で `EltypeUnknown + SizeUnknown` も first-yield 型から
`_similar_for(cont, T, itr, SizeUnknown(), nothing)` で allocation し、後続要素は
`typejoin` widening する Pure Julia path に戻した。同日 follow-up で retained native
collect fallback は `docs/vm/COLLECTIONS.md` の inventory で分類し、CI 監査に登録した。
残りは upstream `PartitionIterator` の view/range-specialized chunk behavior、filtered/arbitrary
generator の full lazy lowering など、#4052 から分割済みの個別 iterator / generator
follow-up である。

## Type System Semantic Parity (Milestone 17 後の残課題)

**Tracking Issue**: [#3855](https://github.com/AtelierArith/ailujsoi/issues/3855)

Milestone 17 で shared `CoreType` bridge、structured method signature bridge、
runtime type-object reflection registry、method-dispatch-first routing の土台は
実装済み。残る Julia 完全互換の型 semantics は hidden heuristic にせず、次の
follow-up として追跡する。

2026-05-12: sjulia type-system/dispatch architecture roadmap goal (#3869) の
子 Issue #3870-#3875 は完了。CoreType single source of truth、shared dispatch
resolver、`LatticeType`/`CoreType` inference bridge、unified `RuntimeTypeRegistry`、
dispatch-first public Base fallback boundary、AoT `CoreType` projection boundary は
実装済み。実装中に見つかった bugs (#3876-#3882) も bug Issue 化して修正済み。
2026-05-19: #3911 follow-up で `Base.pushfirst!`, `Base.popfirst!`,
`Base.insert!`, `Base.deleteat!` は Base-qualified call でも dispatch-first route に
入り、user extension が Rust builtin fallback より先に選ばれるようになった。primitive
Array / `Any` receiver は従来の in-place Rust fallback を保持している。#3911 の残りは
`BASE_FUNCTION_ROUTES` 上の他 public Base names の監査と、Rust-retained boundary の
さらなる削減である。

2026-05-13: sjulia Julia-compatible Pure Julia parity goal (#3883) の子 Issue
#3884-#3890 は完了。#3885 の代表 value parameter gap として `CoreValueParam`,
fixed-length `Vararg{T,N}`, `Val{1}`, `Array{T,1}` / `Array{T,2}`,
`NTuple{N,T}` の structured CoreType 化と代表 subtype checks を追加した。

2026-05-13: Array / Memory boundary follow-up (#3908) として、Pure Julia
`Array{T}` wrapper の基本 `size` / `length` / `ndims` / indexing / mutation
methods を追加し、legacy `Value::Array` には `_mem` / `_size` projection を
持たせた。続く #3908/#3917 で Pure Julia `wrap(Array, Memory, dims)` と
`Memory{T}` runtime type binding を追加し、logical length/bounds は backing
Memory 容量ではなく `dims` に従うようにした。続いて bug #3920 として Rust fallback `reshape`
は source Array の `shape` を in-place mutation しないようにし、#3919 として
reshape-created arrays が source storage owner を共有する bridge を追加した。bug #3923
では formatting / equality / hash の user-visible linear reads を `ArrayValue::get_linear`
へ寄せ、reshape の shared backing storage を観測するようにした。#3925 で
runtime `Value::MemoryRef` と parent Memory + offset model、`memoryref*`
primitive boundary、`parent(ref::MemoryRef)` / `memoryindex(ref::MemoryRef)` を
追加した。#3926 では Pure Julia `wrap(Array, MemoryRef, dims)` を追加し、既存
2-field `Array{T}` wrapper 互換を保ったまま parent Memory + offset metadata で
logical dims と shared offset mutation を扱うようにした。残りは `Array{T,N}`
次元型パラメータ、public array API の Rust fallback 削減、最終的な core
`Value::Array` 退役。#3927 で direct `ArrayValue.data` consumer の分類と
audit を追加し、real broadcast と HOF/iteration extraction は logical reads に
移行した。#3928 で `Array{T}` を rank 任意の Array pattern として扱い、
`Vector{T}` / `Matrix{T}` を `Array{T,1}` / `Array{T,2}` alias、3D+ を
`Array{T,N}` として `typeof` / `isa` / dispatch / Pure Julia wrapper runtime
projection に伝播するようにした。#3908 の次スライスで Pure Julia wrapper の
3D `getindex` / `setindex!` を追加し、`wrap(Array, mem, (d1,d2,d3))` は
column-major order で backing Memory を共有して読み書きできる。#3933 で
public method lookup helper の parametric vararg dispatch を修正し、この 3D
wrapper indexing は upstream-style `I::Int64...` method path へ統合済み。
#3908 の次スライスで `reshape(a::Array{T}, dims...)` を Pure Julia wrapper に
追加し、Rust `reshape` builtin は non-`Value::Array` 引数を Pure Julia dispatch
へ fallback する。MemoryRef-backed wrapper の offset metadata も reshape 後に
保持され、shared parent `Memory` mutation が維持される。
#3908 の次スライスで same-eltype `similar(a::Array{T}, dims...)` を Pure Julia
wrapper に追加し、Rust `similar` builtin は wrapper 引数を Pure Julia dispatch
へ fallback する。#3937 で `::Type{S}` where 引数から得た type parameter value
を Pure Julia method body で使えるようにし、runtime DataType value から
`Memory{S}(n)` を割り当てられるようにしたため、typed
`similar(a, ::Type{S}, dims...)` も Pure Julia wrapper 側へ追加済み。
`scripts/check_value_array_allowlist.sh` は残る Rust `Value::Array` matches を
ファイル単位の上限として固定し、削減は許容しつつ新規・増加を分類なしでは
通さないようにした。
2026-05-15: #4017 で `wrap(Array, Memory, dims)` の Pure Julia wrapper
`getindex(::UnitRange)` / `getindex(::Colon)` を追加し、`IndexSlice` の
non-`Value::Array` target は Rust slice fast path ではなく `getindex` dispatch
へ流せるようにした。bug #4023 で `Value::Range` / `Colon` の runtime dispatch
type identity を `typeof` / reflection と揃えた。残る大きな未実装は
Rust-backed `Value::Array` 自体の indexing / slicing / mutation fast path を
storage primitive に縮小し、public behavior を Pure Julia wrapper dispatch へ
移す作業であり、#4017 の後続 slice と #4018 以降で継続する。
2026-05-19: #4018 follow-up で `IndexLoadTyped` / `IndexStoreTyped` は
Pure Julia `Array{T}` wrapper target を `getindex` / `setindex!` method dispatch へ
fallback できるようになった。native `Value::Array` typed indexing fast path は残している。
残りは slice/range/broadcast/HOF/linalg などの Rust `Value::Array` public behavior を
同じく wrapper dispatch または Memory primitive boundary へ移す作業である。
2026-05-15: #4018 の allocation/similar slice として、Pure Julia wrapper に
`similar(::Type{Array{T}}, dims::Tuple)` / `similar(::Type{Array{T}}, dims::Int64...)`
を追加し、`Memory{T}` + `wrap(Array, mem, dims)` で本家 `julia/base/abstractarray.jl`
および `julia/base/array.jl` の type-form allocation contract に寄せた。bug #4025
で nested `Type{Array{T}}` where parameter binding も修正済み。#4018 の残りは
`collect` / `_collect` / `fill` / `zeros` / `ones` / Array constructor 系の Rust
materialization fast path を Pure Julia allocation dispatch へ移す作業として継続する。
2026-05-15: #4018 の fill slice として、`fill(value::T, dims...) where T` は
本家 `julia/base/array.jl` と同じ `Array{T}(undef, dims)` + `fill!` allocation path
に寄せ、`fill(Float32(...), n)` が `Vector{Float32}` を保つようになった。bug #4028
で `convert(Symbol, :x)` identity conversion も追加済み。ただし `fill(:x, n)` の完全対応
は #4027 として継続する。#4029 では `similar(Array{T}, dims)` wrapper の
`typeof` / `eltype` preservation を修正し、type-form `similar` が `Vector{T}` を返す
contract に揃えた。
2026-05-16: #4027 で `fill` は method body 内の `T = typeof(value)` から
`Array{T}(undef, dims...)` を確保するようになり、本家 `julia/base/array.jl` の
`Array{typeof(v)}` allocation contract に合わせて `fill(:x, n)` / tuple dims Symbol fill が
`Vector{Symbol}` / `Matrix{Symbol}` を返すようになった。bug #4034 で
`Memory{Symbol}` の `setindex!` が numeric `IndexStore` path に落ちる既存バグも修正し、
boxed `Symbol` storage と logical `Symbol` element tag preservation を追加した。
2026-05-16: #4036 で `zeros` / `ones` は Rust direct allocation builtin から
Pure Julia `base/array.jl` multiple dispatch へ移り、`zeros(::Type{T}, dims...)` /
`ones(::Type{T}, dims...)` は `Array{T}(undef, dims...)` を `fill!` する本家
`julia/base/array.jl` の構造に寄せた。bug #4037 で `::Type{T}, dims...` method が
pure vararg fallback に負ける dispatch specificity も修正し、bug #4038 で
`Memory{Complex{Float64}}` への Complex `StructRef` 書き込みも `fill!` path で通るようにした。
bug #4040 で negative dims は allocation 前に本家 `checked_dims` / `GenericMemory`
相当の catch 可能な `ArgumentError` へ揃えた。bug #4041 で `size(a)[1]` 由来の
mixed inferred dims も typed `zeros` / `ones` allocation 前に `Int64` 正規化するようにし、
`adjoint(collect(1:3))` / `(1:3)' .* (1:3)` が本家同様に `Matrix{Int64}` で通る。
#4018 の残りは `collect` / `_collect` / Array constructor 系の Rust materialization
fast path を Pure Julia allocation dispatch へ移す作業として継続する。
2026-05-16: #4039 で直接の `zeros(Complex{Float64}, n)` /
`ones(Complex{Float64}, n)` reflection は本家 `Vector{ComplexF64}` / `ComplexF64`
相当に揃えた。#4044 で型値を変数経由で渡す
`function make_ones(T); ones(T, 2); end` も runtime `Type{T}` dispatch として通るようにした。
2026-05-16: bug #4047 で `Array{T,N}(undef, dims::Tuple)` が tuple dims を
`d...` として展開せず `Int64` coercion error になる既存バグを修正し、本家
`julia/base/boot.jl` の explicit-rank tuple constructor family に合わせた。
2026-05-16: bug #4048 で cached Base bytecode 内の `promote_type` が後続 user
program の `promote_rule` extension を見ない既存バグを修正した。user code が
`import Base: promote_rule` して method を追加する場合は全体コンパイルに戻し、
本家 `julia/base/promotion.jl` の `promote_type` / `promote_rule` 多重ディスパッチ
contract に合わせる。
2026-05-16: bug #4050 で `collect(::Type{T}, itr)` が旧単項
`BuiltinOp::Collect` fallback に拒否される既存バグを修正した。本家
`julia/base/array.jl` の `collect(::Type{T}, itr)` / `_collect(::Type{T}, itr,
::SizeUnknown)` に合わせ、sjulia の Pure Julia `base/array.jl` で
`Vector{T}(undef, 0)` + `push!` により `Vector{T}` を materialize する。#4018 の
残りは untyped `collect` / Array constructor 系の Rust materialization fast path
を Pure Julia allocation dispatch へ移す作業として継続する。
2026-05-19: #3954/#4018 follow-up で `Memory` container の `collect_similar` は
`_similar_for(cont::Memory, T, itr, isz, shp)` から Pure Julia
`similar(::Memory, ::Type, ...)` を経由するようになり、rank-1 は `Memory{T}`、
matrix shape は `wrap(Array, Memory{T}, dims)` を返す。本家
`./julia/base/genericmemory.jl` の allocation contract に寄せた slice であり、
残りは public `similar(a::Array, ...)` compiler fast path と Rust `Value::Array`
materialization boundaries の削減で継続する。
同日 follow-up で `Array{T}` / `Memory{T}` receiver pattern から method body の
`T` を fallback 復元できるようにした。これで本家 `./julia/base/array.jl` の
`similar(a::Array{T}, dims...) where T` shape へ public `similar` fast path を寄せる
前提が固まった。残りは known `Array` receiver の compiler/Rust fast path を
Pure Julia dispatch へ段階的に戻す作業である。
同日 follow-up で compile-time known `Array` receiver の `similar` early builtin route は
外し、Pure Julia method dispatch と user extension が builtin fallback より先に選ばれる
ようになった。残りは `Any` receiver / cached bytecode 互換で残る Rust
`BuiltinId::Similar` fallback と、Array constructor / collect materialization fast path の
削減である。
2026-05-22 follow-up で 2-D+ `similar(::Memory, dims::Int64...)` と
`similar(::Memory, ::Type, dims::Int64...)` は Pure Julia `genericmemory.jl` method に移り、
1-D は `Memory`、2-D+ は Memory-backed `Array` wrapper を返す本家 shape に揃えた。
残りは `Any` receiver / cached bytecode 互換で残る Rust `BuiltinId::Similar` fallback、
Array constructor / collect materialization fast path、および broader `Value::Array`
materialization boundary の削減である。
同日 follow-up で Pure Julia `Array{T}` wrapper の `push!` は、新しい `Memory{T}` へ
copy + append して `_mem` / `_size` を更新できるようになった。legacy `ArrayPush`
fallback も wrapper target では `push!` method dispatch に戻る。これで
`similar(a, 0)` が wrapper を返す path を後続 `push!` に流す土台はできたが、
bare `Array` receiver の `similar` dispatch-first 化は vcat/collect 全体の wrapper
mutation coverage を広げてから継続する。
同日 follow-up で bare `Array` receiver の `similar` early builtin route も外し、
Pure Julia `similar(a::Array, ...)` は `eltype(a)` 経由で typed allocation helper に
委譲するようになった。残る `BuiltinId::Similar` の `Value::Array` branch は
`Any` receiver / cached bytecode compatibility fallback として残しており、次は
runtime fallback 側で method dispatch を優先できる範囲をさらに狭める。
この過程で発見した wrapper Array と native Array literal の `==` 不一致は
Issue #4189 として切り出し、Array element comparison を追加して解消した。
同日 follow-up で VM-native `collect(LogRange)` の direct `ArrayValue::from_f64`
materialization は `ArrayValue::memory_first_from_f64` へ移し、collect result storage
audit は `type_ops/iteration.rs` の direct f64 ArrayData construction を検出しなくなった。
2026-05-16: #4052/#4053 で generic `collect(itr)` は
`IteratorEltype(itr)` / `IteratorSize(itr)` / `_collect` の trait-shaped Pure Julia
entry に寄せ、homogeneous tuple は `Vector{T}` を返すようにした。#4055/#4056 で
heterogeneous tuple の abstract join (`Vector{Real}` / `Vector{Integer}` /
`Vector{Signed}`) と indexed tuple value path も本家 `collect_to_with_first!` /
`push_widen` 形に寄せた。残りは range/generator の Rust fallback 縮小を
#4265 / #4266 follow-up として継続する。
2026-05-19: #4052/#3954 follow-up で `HasEltype + SizeUnknown` の `_collect`
は本家 `./julia/base/array.jl` と同じく `_similar_for(cont, eltype(itr), itr,
isz, nothing)` で container allocation してから `push!` するようになった。
検証中に発見した tuple-valued Array equality の `MethodError` は Issue #4191 として
切り出し、Pure Julia tuple `==` を追加して解消した。
残る range/generator Rust fallback のさらなる縮小は #4265 / #4266 で継続する。
2026-05-17: #4056 で `collect(x::Any)` の runtime candidate に VM-native
`collect(::Tuple)` を追加し、indexed/runtime tuple value が generic
`collect(::Any)` fallback ではなく tuple-specific method を multiple dispatch で
選ぶようにした。`base/iterators.jl` の `_collect(::EltypeUnknown)` 内
`isa(itr, Tuple)` workaround は削除済み。
2026-05-17: #4059 で empty tuple collect は本家 `julia/base/array.jl` の empty
iterator branch に合わせて `Vector{Union{}}` / `Union{}` を返すようにした。
`Vector{Union{}}(undef, 0)` と runtime `Vector{T}` where `T === Union{}` も
Bottom element tag を保持する。
2026-05-17: #4061 で `collect(::String)` は Pure Julia multiple dispatch に寄せ、
literal / variable / runtime-Any string collect が `Vector{Char}` / `Char` を返すようにした。
VM `collect_iterator` の `Value::Str` materialization branch は削除済み。検証中に
`_collect(..., HasEltype(), HasLength())` がまだ trait argument dispatch で
`Vector{Any}` へ落ちる既存バグを #4062 として切り出した。
2026-05-17: bug #4062 で上記 string trait-shaped `_collect` path は解決済み。
原因は trait dispatch ではなく、HasEltype method 内の `eltype(itr)` が runtime
String value から `Char` を返せず `Any` へ落ちていたことだった。`eltype(::String)` と
DataType `String => Char` projection により、`Base._collect(..., Base.HasEltype(), ...)`
は `Vector{Char}` / `Char` を返す。
2026-05-17: #4065/#4066 で range の trait-shaped `_collect` path も改善した。
`IteratorEltype(::AbstractRange) = HasEltype()` / `IteratorSize(::AbstractRange) = HasLength()`
を Pure Julia multiple dispatch で追加し、`eltype(x)` where `x::Any` が runtime
`Value::Range` に対して `eltype(::String)` を誤選択する既存バグを `BuiltinId::Eltype`
fallback routing で修正した。`Base._collect(..., range, Base.HasEltype(), Base.HasLength())`
は `Vector{Int64}` / `Vector{Float64}` を返す。VM-native `Value::Range` の public
`collect(range)` fallback は、`Value::Range` が Pure Julia struct ではなく field access
body に dispatch できない representation boundary として残る。deeper
`Value::Range` fallback 縮小は #4266 に分割した。
2026-05-17: #4068 で direct `Base.Generator(f, iter)` と runtime
`collect(x::Any)` where `x` is a VM-native `Base.Generator` は、generic `collect(::Any)`
fallback ではなく既存 `RangeCollect` / `collect_generator` boundary に dispatch するようにした。
これにより本家 `julia/base/generator.jl` の `iterate(g::Generator, s...) = (g.f(y[1]), y[2])`
と `julia/base/array.jl` の `collect(itr::Generator)` に合わせ、`f(x)` 適用と
`Vector{Int64}` / `Vector{Float64}` result eltype preservation は通る。残る
generator expression syntax は eager wrapped-array path で値は正しいが result eltype が
`Any` に落ちるため bug #4069 として分離し、空 `Base.Generator` collect が本家の
empty-generator result eltype を保てない問題は bug #4070 として分離した。
2026-05-17: bug #4069 で generator expression collect の result eltype は解決済み。
`collect(double(x) for x in [1,2,3])` と `g = (double(x) for x in [1,2,3]); collect(g)`
は本家同様 `Vector{Int64}` / `Int64` を返し、`to_float` は `Vector{Float64}` /
`Float64` を返す。generator expression lowering はまだ eager wrapper だが、loop
variable の compiler type environment と call-site return inference を本家
`julia/base/array.jl` の first-value typed collect behavior に近づけた。
2026-05-17: bug #4070 で empty direct `Base.Generator` collect の result eltype は解決済み。
`collect(Base.Generator(double, Int64[]))` と runtime `collect(x::Any)` where `x` is an
empty `Base.Generator` は、本家 `julia/base/array.jl` の `@default_eltype` / empty
`collect(itr::Generator)` branch と同様に `Vector{Int64}` / `Int64` を返す。
2026-05-17: #4074 の最初の slice / bug #4075 で runtime `collect(x::Any)` where
`x` is a VM-native `Value::Range` は generic `collect(::Any)` fallback ではなく既存
`RangeCollect` / `collect_iterator` representation boundary へ入るようにした。
`runtime_collect(1:5)` / empty / stepped / reverse / floating ranges は本家
`julia/base/range.jl` の `collect(r::AbstractRange) = Array(r)` と同様に
`Vector{Int64}` / `Vector{Float64}` を返す。#4074 の残りは static `Value::Range`
compile-time short-circuit と struct-backed range method boundary のさらなる縮小。
2026-05-17: #4077 の最初の slice / bug #4078 で `collect(range(...))` が
compile-time `Any` / runtime dispatch 経由で generic `collect(::Any)` fallback に落ち、
`LinRange` / `StepRangeLen` の result eltype が `Any` になる問題を修正した。
#4075 により VM-native `Value::Range` は candidate scoring 前に `RangeCollect`
boundary へ入るため、real Pure Julia struct-backed `LinRange` / `StepRangeLen`
collect methods を runtime native candidates に戻せる。#4077 の残りは static
`Value::Range` compile-time short-circuit 自体の局所化または必要性の明文化。
2026-05-17: #4077 で static `Value::Range` collect short-circuit を外す実験を行い、
`x = collect(1:5)` が generic `collect(::Any)` に落ちて `Vector{Any}` / `Any` に退行する
ことを確認した。このため、VM-native range が Pure Julia struct ではない現状では
static `Value::Range` は `RangeCollect` representation boundary に残す。#4074 の
range collect slice は runtime Range (#4075) と struct-backed Range (#4078) を解決し、
static VM-native Range boundary は必要性を明文化済み。
2026-05-21: #4266 で user-defined `collect(::UnitRange{Int64})` は static
`collect(1:3)` と `collect(x::Any)` runtime path の両方で `RangeCollect` fallback より
先に dispatch scoring へ入るようになった。通常の VM-native range materialization
fallback は維持している。残りは `Value::Range` を本家 `AbstractRange` / `_collect`
trait pipeline に完全参加させ、special-case sentinel と representation boundary を
さらに縮小することである。
2026-05-21: #4266 follow-up で user range method が存在しない direct integer
`UnitRange` (`collect(1:3)`) は sjulia Base の `collect(::UnitRange{T})` bridge から
`_collect(..., IteratorEltype, IteratorSize)` trait path に入るようになった。
user override のない direct integer `StepRange` も後続 slice で同じ Base bridge に
入るようになった。2026-05-22 follow-up で
user-defined `collect(::StepRange{Int64,Int64})` は direct / `Any` runtime path とも
`RangeCollect` fallback より先に dispatch へ入るようにした。同じ follow-up で
user-defined `collect(::AbstractRange)` が sjulia-internal `UnitRange` / `StepRangeLen`
collect bridge より優先されるようにし、direct / `Any` runtime path の代表ケースを
本家互換にした。2026-05-22 follow-up で direct integer `StepRange` は
`collect(::StepRange{T,S})` bridge から `_collect(..., IteratorEltype, IteratorSize)`
path に入り、同日 follow-up で direct floating numeric range も static
`RangeCollect` ではなく Base bridge へ入るようにした。さらに runtime
`collect(x::Any)` の integer non-unit `StepRange` は Base `StepRange` collect
candidate を scoring して pre-score native materialization を避ける。2026-05-27 に
current checkout で再検証し、#4266 の goal である broad `RangeCollect` shortcut の縮小と
trait-shaped public collect dispatch への representative migration は closed。残る
runtime floating `StepRangeLen` など、Pure Julia range methods が field access を必要とする
VM-native `Value::Range` object-model / final native carrier cleanup は #4568 へ渡す。
2026-05-17: #4081 で `Base._collect(..., ::HasEltype, ::HasLength)` は
`Vector{T}(undef, length(itr))` を preallocate して index assignment で埋める
Pure Julia method に寄せた。本家 `julia/base/array.jl` の known-length collect
allocation shape へ近づき、range / string trait path は `Vector{Int64}` /
`Vector{Float64}` / `Vector{Char}` を保つ。残りは `HasShape` / `_similar_for`
shape-preserving collect と `collect_to_with_first!` 相当の widening path のさらなる統合。
2026-05-17: #4083 で matrix `_collect` は本家 `julia/base/generator.jl` の
`IteratorSize(::AbstractArray) => HasShape{N}()` と `julia/base/array.jl` の
`_collect` / `_similar_shape` / `_similar_for` に寄せ、`similar(itr, T, size(itr)...)`
で shape-preserving allocation する Pure Julia path を追加した。primitive Array の
trait boundary が現時点で `HasLength()` を返す場合も multi-dimensional Array は同じ
shape-preserving path に入るため、matrix collect は `Matrix{T}` を保つ。残りは
`Base.HasShape{N}()` / `Val{N}()` の zero-field value-parameter struct constructor
gap (#4084) と、`collect_to_with_first!` 相当の widening path 統合。
2026-05-17: bug #4084 で `Base.HasShape{N}()` / `Val{N}()` の代表
zero-field value-parameter struct constructor gap は解決済み。Base-qualified
parametric struct names は unqualified Base definition へ正規化して instantiate し、
`IteratorSize(::Array)` は `ndims(a)` に応じて `HasShape{1/2/3}()` を返すようになった。
残りは 4D 以上の Array trait と `collect_to_with_first!` 相当の widening path 統合。
2026-05-17: bug #4087 で direct trait-shaped
`_collect(..., EltypeUnknown(), HasLength()/HasShape())` は `HasEltype` shape path へ
誤 dispatch せず、first element から `typejoin` で widening する path に入るようになった。
tuple direct trait path は本家 `collect_to_with_first!` / `push_widen` 相当の
`Vector{Real}` を返す。残りは 4D 以上の Array trait。
2026-05-17: bug #4088 で `Base.IteratorEltype` / `Base.IteratorSize` user extension
visibility は代表ケースで解決済み。custom iterator の `IteratorEltype(::T)=EltypeUnknown()`
と `IteratorSize(::T)=HasLength()` が `collect(custom)` から見え、既存
`Base.IteratorEltype(1:5)` も `HasEltype()` を保つ。残りは 4D 以上の Array trait。
2026-05-17: #4091 で 4D Array の trait-shaped collect は本家
`IteratorSize(::AbstractArray{<:Any,N}) => HasShape{N}()` / `_collect(..., ::HasShape)`
に寄せ、`IteratorSize(::Array)` が `HasShape{4}()` を返し、`Base._collect` が
`Array{T,4}` / 元 shape を保つようになった。2026-05-18 follow-up #4052 で rank 5-8 の
`IteratorSize(::Array)` と supported rank-5 `_collect` fixture は本家互換へ寄せた。
残りは rank 9 以上の Array allocation boundary と、tuple dims dispatch /
dynamic value-parameter constructor を含む arbitrary-rank 化。
2026-05-15: bug #4030 で `promote(big(2), 1//3)` の 2 番目の値が
`Rational{Any}` / type field に崩れる既存バグを修正し、`Rational{Int64}` などの
concrete Rational から `Rational{BigInt}` への conversion を本家 `julia/base/rational.jl`
および `julia/base/gmp.jl` の BigInt promotion contract に揃えた。
2026-05-17: bug #3973 で `big(2) + 1//3` など direct BigInt/Rational mixed
arithmetic が broad `Integer` / `Rational` path で `Rational{Int64}` に戻る問題を
修正し、`+`, `-`, `*`, `/` の両 operand order が `Rational{BigInt}` を返すようにした。
bug #4032 で `copy(Memory{T})` 後の `eltype` が `Any` に落ちる既存バグを修正し、
Pure Julia `genericmemory.jl` の `eltype(m::Memory)` は `typeof(m).parameters[1]`
から `T` を返すようになった。
bug #3941 で `memory_to_array_ref` allowlist も exact count から上限方式へ
揃え、#3937 で削減された `builtins_types.rs` bridge count を 3 に固定した。
bug #3943 で `copy(::Memory{T})` が `Memory{Any}` に落ちる既存バグを修正し、
Pure Julia `genericmemory.jl` の copy は同じ `Memory{T}` を返すようになった。
bug #3945 で `Memory` の shape protocol と `similar` が Julia 互換ではない既存
バグを修正し、`size(m)`, `size(m, d)`, `ndims(m)`, same-eltype/typed
`similar(::Memory{T}, ...)` は Pure Julia `genericmemory.jl` 側で扱うようにした。
Rust fallback も rank 超過の正の次元に `1` を返し、`ndims(::Memory) == 1` に揃えた。
bug #3947/#3948 で `copyto!` の negative-count guard 欠落を修正し、Memory-specific
Pure Julia `unsafe_copyto!` / `copyto!` と Array 5 引数 `copyto!` の checked boundary
が upstream と同じ `ArgumentError` を投げるようになった。
2026-05-15: #3998 で `value_to_julia_code(Value::Memory)` の debug/source-code
formatting bridge は temporary Array wrapper を作らず Memory storage を直接読むようにし、
`formatting.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
2026-05-15: #4000 で `NewStructSplat` の Memory splat bridge は temporary Array
wrapper を作らず Memory storage を直接 field values として読むようにし、
`exec/struct_ops.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
2026-05-15: #4002 で `ArrayPush` / `ArrayPop` の Memory mutation bridge は
temporary Array wrapper を作らず、fixed-size Memory の unsupported mutation として
MethodError path に落とすようにし、`exec/array_mutate.rs` の `memory_to_array_ref`
allowlist ceiling は 0 になった。
2026-05-15: #4004 で `StackOps::pop_array` の blanket Memory bridge は削除され、
Array 専用 VM helper が fixed-size Memory を temporary Array wrapper として受け入れない
ようになり、`stack_ops.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
bug #3950 で Pure Julia `Array{T}` wrapper の `size(a, d > ndims(a))` が `1` ではなく
bounds error になる問題を修正し、upstream `AbstractArray` の trailing dimension rule
に揃えた。
#3952 で Phase 2 の足場として `ArrayValue::memory_first_*` helper と constructor
監査を追加し、VM array constructor builtins の Array-result allocation は direct
`ArrayValue::from_memory` ではなく helper 経由になった。
#3953 で typed array literal builder (`NewArrayTyped`) は
`ArrayValue::memory_first_with_capacity` 経由になり、primitive `MemoryValue` を先に
確保して transitional `ArrayValue` builder として包む。
#3954 で `collect(range)` / numeric tuple `collect` / `collect(string)` result
materialization は `ArrayValue::memory_first_from_*` helper 経由になり、typed result
buffer を primitive `MemoryValue` として所有してから transitional `ArrayValue` に包む。
#3955 で kw/default literal array compatibility construction は `compile/utils.rs` の
`literal_array_value` helper 1 箇所に集約し、同ファイルの `Value::Array` allowlist ceiling
を 3 から 1 に下げた。
#3960 で `collect(::Array)` / VM array iterator copy path は
`ArrayValue::memory_first_copy_from_array` 経由になり、Pure Julia
`collect(arr::Array)` も `similar(arr)` による shape-preserving copy に揃えた。
#3961 で generic collect/grow value materialization は
`ArrayValue::memory_first_collect_values` に集約し、tuple collect と generator
expression collect の result storage は Memory-first helper 経由になった。bug
#3966 で VM lazy `Value::Generator` が `f(x)` を適用しない問題を Issue 化し、
完全対応までは compiler が壊れた lazy path を出さないようにして誤結果を防いだ。
#3966 で VM `collect(Generator(f, iter))` は wrapped iterator を materialize した後、
既存の value-mode HOF frame path で各要素へ `f(x)` を適用するようになった。
generator expression syntax は、一般の `iterate(::Generator)` がまだ同期プロトコルで
function frame に入れないため、引き続き eager wrapped-array path を使う。
#3962 で low-level VM broadcast fallback result と HOF fallback buffers は
`ArrayValue::memory_first_from_i64` / `memory_first_from_f64` 経由になり、
`scripts/check_broadcast_hof_memory_first.sh` で direct typed ArrayValue
result materialization の再導入を防ぐようにした。
#3963 で compile-time `Literal::Array*` constant conversion と REPL persistence
literal reconstruction は `ArrayValue::memory_first_from_*` / logical
`ArrayValue::get_linear` 経由になり、reshaped/shared-storage arrays を raw
`ArrayData` から stale に再注入しないようにした。
#3964 で REPL `Value::Memory` persistence は `memory_to_array_ref` 経由の
temporary `Value::Array` construction をやめ、`MemoryValue` から直接
`Literal::Array*` へ変換するようにした。これにより `repl/converters.rs` の
`Value::Array` allowlist ceiling は 2 から 1 に下がった。
2026-05-15: #4006 で `REPLSession` に残っていた `memory_to_array_ref`
呼び出しも削除し、global injection / global type tracking は direct Memory
storage / element type read に移した。`repl/session.rs` の
`memory_to_array_ref` allowlist ceiling は 0 になった。`Literal::Memory` が無いため
REPL 再注入の完全な Memory 型 fidelity は #4009 に分離している。
2026-05-15: #4007 で `vm::util::memory_to_array_ref` helper を削除し、
`pop_array_or_values` / `bind_value_to_frame` も temporary `ArrayRef` を作らず
Memory storage / `locals_any` を直接使うようにした。`vm/util.rs` の
`memory_to_array_ref` allowlist ceiling は 0 になった。
2026-05-15: #4008 で `scripts/check_memory_to_array_ref_allowlist.sh` を
zero-use audit に変更した。`memory_to_array_ref(` は `subset_julia_vm/src`
配下で 0 件であり、再導入は audit failure になる。
2026-05-15: #4009 で REPL global / `ans` persistence は `Memory{T}` を
`Literal::Array*` へ落とさず、`Memory{T}(undef, n)` + `setindex!` で
type-faithful に復元するようになった。残る REPL 非 Literal 型は個別の
specialized reconstruction または dedicated Literal が必要。
2026-05-15: #4010 で残る `Value::Array` fast path を監査し、#4017
indexing/mutation、#4018 allocation/collect/similar、#4019 broadcast/HOF/reduction、
#4020 LinearAlgebra、#4021 host/cache boundary shrink に分割した。各 Issue は
本家 Julia `julia/base/array.jl`, `abstractarray.jl`, `broadcast.jl`, `reduce*.jl`,
および `julia/stdlib/LinearAlgebra/src/*.jl` を参照元として明記している。
#3976 で `sizeof(::Memory)` と `in(x, mem)` は `builtins_types.rs` 内で
temporary `ArrayValue` を作らず、`MemoryValue` storage を直接読むようになった。
`memory_to_array_ref` allowlist ceiling は同ファイルで 3 から 1 に下がり、残る
bridge は `isa` の旧 array-facing reflection compatibility に限定された。
#3978 で `isa(::Memory, T)` も `MemoryValue.element_type()` から直接判定するようになり、
`builtins_types.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
`Memory{T}` は upstream と同じく `AbstractVector{T}` / `AbstractArray{T,1}` 系だが、
`Vector{T}` / `Array{T,1}` ではない。
#3980 で dynamic arithmetic の input normalization も `MemoryRef` direct read に移し、
`dynamic_ops/helpers.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
#3982 で `CallDynamicBinaryBoth` の Any 動的 binary fallback も direct Memory-aware
branch に移し、`exec/binary_both.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
#3984 で equality/hash builtins も direct Memory traversal に移し、`builtins_equality.rs`
の `memory_to_array_ref` allowlist ceiling は 0 になった。
#3986 で Dict collection fallback に残っていた `keys(::Memory)` / `pairs(::Memory)` の
temporary Array wrapper も削除し、`builtins_dicts.rs` の `memory_to_array_ref`
allowlist ceiling は 0 になった。実装中に見つかった bug #3987 として
`iterate(::Memory)` を VM iteration protocol に追加し、`values(m) === m` の
反復と `pairs(m)` の index/value iteration は upstream `AbstractVector` contract に
寄せた。#3988 で sjulia-representable な `Base.Pairs{K,V,I,A}` view も追加し、
`pairs(::Array)` / `pairs(::Tuple)` / `pairs(::Memory)` は value indexing と
Pair iteration を持つ `Pairs` family を返すようになった。残る差分は full
`AbstractArray` index-style hierarchy と `LinearIndices` の upstream exact type
identity であり、Array / Memory boundary の後続 Issue 群で継続する。
#3990 で `String(::Memory)` conversion も direct `Memory{Char}` storage read に移し、
`builtins_strings.rs` の `memory_to_array_ref` allowlist ceiling は 0 になった。
#3992 で `vm/mod.rs` の struct-field comparison に残っていた Memory temporary
Array wrapper も削除し、`vm/mod.rs` の `memory_to_array_ref` allowlist ceiling は
0 になった。検証中に見つかった default struct equality の reference-like field
互換差分は bug #3993 として分離した。
#3995/#3996 で `count(f, ::Memory)` は Pure Julia `genericmemory.jl` method と
`Memory{T}` preserved dispatch inference で処理するようになり、`exec/hof.rs` の
`memory_to_array_ref` allowlist ceiling は 0 になった。
VM lazy Generator の general async `iterate(::Generator)`、残る Rust-retained public array
fallback builders はまだ別 follow-up 対象。
残る direct storage access は typed fast path、
FFI/cache compatibility、内部 storage helper として分類しながら縮小する。最終的には
core `Value::Array` 退役と Rust-retained public API fallback のさらなる削減が残る。

| Area | Remaining work |
|------|----------------|
| Subtyping / intersection | #3858 と #3871/#3885 で `UnionAll` 右辺 pattern、diagonal `TypeVar`、trailing/fixed `Vararg` tuple、代表 value parameter (`Val{1}`, `Array{T,2}` の次元値, `NTuple{N,T}`)、shared dispatch resolver の代表ケースは CoreType/shared resolver に移行済み。残りは arbitrary bits value parameter, nested / lower-bound-heavy `UnionAll`, method ambiguity lattice などの full Julia surface |
| MethodTable ownership | #3859 と #3871/#3877/#3881 で stored `core_signature`、duplicate replacement identity、compile-time specificity scoring、shared resolver、user struct ancestry fallback、diagonal rule guard は移行済み。残りは cache key 全体、ambiguity detection、runtime `FunctionInfo` dispatch projection の完全統合 |
| Runtime type objects | #3862 と #3873/#3882 で TypeVar metadata、built-in/user hierarchy query、DataType flag predicates は `RuntimeTypeRegistry` read model に集約済み。残りは必要に応じて full `DataType` / `UnionAll` / `TypeVar` object identity / layout identity semantics へ拡張 |
| AoT inference | #3860 と #3875/#3890 で `StaticType::core_typejoin()` と AoT numeric classifier projection は CoreType-backed になった。残りは widening / meet / constant typing / abstract semantic supertypes / `UnionAll` / arbitrary value parameters まで `StaticType` を backend projection に寄せる作業 |
| Public Base routing | #3861 と #3874 で migrated public names の module-qualified route と mutating collection fallback boundary は method-dispatch-first inventory に乗せた。残りは Rust-retained public routes の分類を CI audit で完全固定し、future builtin additions を自動検出する作業 |

---

## AoT Pipeline (部分完了)

**Tracking Issue**: [#2596](https://github.com/AtelierArith/ailujsoi/issues/2596)

AoT パイプラインの復旧作業のうち、コンパイルエラー修正 (#2590)、インポート修正 (#2592)、E2E テスト有効化 (#2593) は完了。Mandelbrot broadcast/Complex/@time の pure-rust AoT コンパイルも完了（PR #2818, #2819）。残りの作業:

2026-05-11: P2 Native AoT 境界の最初の土台として、CodeInstance-like specialization unit と dependency enqueueing (#3718)、backend-neutral ABI value boundary (#3719)、generated runtime multidispatch dispatcher (#3720)、named pass diagnostics / verifier hooks (#3721)、rooting/safepoint contract (#3722)、ccall/llvmcall native-call boundary classification (#3723) は完了。

| Issue | Title | Status |
|-------|-------|--------|
| [#2591](https://github.com/AtelierArith/ailujsoi/issues/2591) | Enum support in AoT pipeline | 🔧 Open — `Stmt::EnumDef`, `JuliaType::Enum` の全 AoT コンポーネントでの対応 |
| [#2594](https://github.com/AtelierArith/ailujsoi/issues/2594) | Complete Cranelift JIT backend | 🔧 Open — 関数呼び出し、配列/フィールドアクセス、phi ノード、libm リンク |
| [#2595](https://github.com/AtelierArith/ailujsoi/issues/2595) | End-to-end pipeline verification | 🔧 Open — Julia → Rust → compile → run の完全検証 |

### AoT Phase 2 Stubs (Issue #3116)

以下の TODO スタブは意図的な Phase 2 プレースホルダー:

| Location | Issue | Description |
|----------|-------|-------------|
| `aot/codegen/aot_codegen/program.rs:365` | [#3133](https://github.com/AtelierArith/ailujsoi/issues/3133) | 未初期化グローバル変数の codegen — `lazy_static!` or `OnceLock` が必要。現在は `// TODO: static X: T;` コメントを出力 |

---

## VM Error Handling (✅ 完了)

All `panic!()` calls have been removed from the VM runtime:

- Issue #1599: `call.rs` の panic!() 修正
- Issue #1792: `array_value.rs` (6 panics) と `hof_exec.rs` (5 panics) を `Result`/`VmError` に変換
- Issue #1807: `call_dynamic.rs` (5 `.unwrap()`) と `return_ops.rs`, `hof_exec.rs`, `builtins_reflection.rs` の `.unwrap()` を `ok_or_else(VmError)` に変換

残りの `.unwrap()` 呼び出し（49箇所）はテストコードまたはガード条件の後にあり安全。

---

#### iOS UI 検証
- iOS 側の検証項目は `docs/ios/UNIMPLEMENTED.md` に集約

## Julia Base 未実装関数一覧（完全版）

**最終更新**: 2026-07-07 (Threads single-thread shim は Issue #8991 で実装済み)

Julia の Base モジュール（`julia/base/exports.jl`）からエクスポートされている 998 項目のうち、SubsetJuliaVM で未実装のものを網羅的にリストアップ。

### 1. モジュール

| モジュール | 説明 | 状態 |
|-----------|------|------|
| `Meta` | メタプログラミング | ✅ 部分実装（`parse`, `isexpr`, `quot`, `isidentifier`, `isoperator`, `isunaryoperator`, `isbinaryoperator`, `ispostfixoperator`, `unblock`, `unescape`, `show_sexpr`） |
| `StackTraces` | スタックトレース | ❌ |
| `Sys` | システム情報 | ⚠️ 部分実装（`WORD_SIZE` module binding; `Sys.is*` は `@static` 条件のみ） |
| `Libc` | C ライブラリバインディング | ❌ |
| `Docs` | ドキュメント | ⚠️ plain-text `@doc(f)` retrieval のみ (Issue #8997)。Markdown metadata / REPL `?` rendering は未対応。 |
| `Threads` | single-thread compatibility shim は実装済み（`nthreads/threadid/maxthreadid`, `@threads`, `@spawn`, `Atomic`, `SpinLock`; Issue #8991）。真のマルチスレッドは [SINGLE_THREADED_VM.md](./SINGLE_THREADED_VM.md) の設計判断どおり未対応。 | ⚠️ shim のみ |
| `Iterators` | イテレータモジュール | ⚠️ 一部関数のみ（`enumerate`, `zip`, `rest`, `countfrom`, `take`, `drop`, `takewhile`, `dropwhile`, `cycle`, `repeated`, `product`, `flatten`, `flatmap`, `partition`, `peel`, `nth`, `filter`, `map`, `reverse`, `accumulate`） |

---

### 2. 型（Types）

> **関連 Issue**: [#343](https://github.com/AtelierArith/ailujsoi/issues/343) (Regex: 残課題), [#344](https://github.com/AtelierArith/ailujsoi/issues/344) (Float16)
> **Closed Issues**: #527 (Set), #528 (IOContext), #529 (LinRange/StepRangeLen), #530 (Iterators), #531 (Pair), #532 (VersionNumber), #533 (Irrational)

#### 未実装の抽象型
`AbstractChannel`, `AbstractSlices`, `AbstractMatch`, `AbstractPattern`, `AbstractVecOrMat`

#### 未実装の具象型
| 型 | 説明 |
|---|------|
| `Cmd` | コマンド |
| `Colon` | `:` 型 |
| `Accumulate` | イテレータ型（`Count` は `countfrom()` として実装済み、`Generator` と `Filter` は Pure Julia で実装済み） |
| `ColumnSlices`, `RowSlices`, `Slices` | スライス型 |
| `DenseMatrix`, `DenseVecOrMat`, `DenseVector` | 密配列型 |
| `Dims` | 次元タプル |
| `ExponentialBackOff` | 指数バックオフ |
| `IdDict`, `IdSet` | ID ベース辞書/集合 |
| `IndexStyle` | インデックススタイル |
| `InsertionSort`, `MergeSort`, `QuickSort`, `PartialQuickSort` | ソートアルゴリズム型 |
| `IOStream` | I/O ストリーム（✅ IOBuffer + file-backed cursor subset: `open`/`close`/`isopen`/`eof`/`flush`/`position`/`seek`/`skip`/`read(io, Char)` は Issue #8996 で実装。`redirect_*` / `Pipe` / full concrete `IOStream` surface は #9577） |
| `LazyString` | 遅延文字列 |
| `Lockable`, `OncePerProcess`, `OncePerTask`, `OncePerThread` | 同期プリミティブ |
| `NTuple` | N 要素タプル |
| `OrdinalRange` | 順序範囲 |
| `PermutedDimsArray` | 次元転置配列 |
| `RoundingMode` 系 | 丸めモード（8種類） |
| `StridedArray`, `StridedMatrix`, `StridedVecOrMat`, `StridedVector` | ストライド配列 |
| `SubArray` / `ReshapedArray` | サブ配列ビュー / reshape wrapper | ✅ 部分実装（Issue #5137/#5583 前進: `Vector{Float64}` / `Vector{Int64}` / `Vector{Int8}` の 1D range view は `SubArray{T,1,Vector{T},Tuple{UnitRange{Int64}},true} <: AbstractArray{T,1}` surface と `AbstractVector{T}` membership をサポート。`reshape(::SubArray{Int64,1,...}, 2, 2)` は `ReshapedArray{Int64,2,SubArray{...},Tuple{}} <: AbstractArray{Int64,2}` surface と parent mutation aliasing をサポート。残りは多次元 `SubArray{T,N,P,I,L}` と range-backed / arbitrary-rank `ReshapedArray`） |
| `SubString` | サブ文字列 |
| `SubstitutionString` | 置換文字列 |
| `Timer` | タイマー |

#### 実装済みの具象型（以前は未実装としてリストされていた）
| 型 | 説明 |
|---|------|
| `ComposedFunction` | 関数合成（`Value::ComposedFunction` として VM で完全サポート、`∘` 演算子対応） |
| `Float16` | 16ビット浮動小数点（`Value::F16` として VM で完全サポート） |
| `Enum` | 列挙型（`@enum` マクロ、VM・コンパイラ対応） |
| `Channel{T}` | 並行処理チャンネル（parametric struct, cooperative blocking, Issues #348/#445/#3450/#3451）。真の非同期コルーチン基盤は未実装 |
| `WeakKeyDict` | 弱参照辞書（Issue #8990/#10088）。基本的な `setindex!` / `getindex` / `get` / `haskey` / `delete!` / `iterate` / GC 後 cleanup と bracket syntax `d[k]` / `d[k] = v` をサポート |

#### 未実装の Ccall 型（全20種）
`Cchar`, `Cdouble`, `Cfloat`, `Cint`, `Cintmax_t`, `Clong`, `Clonglong`, `Cptrdiff_t`, `Cshort`, `Csize_t`, `Cssize_t`, `Cuchar`, `Cuint`, `Cuintmax_t`, `Culong`, `Culonglong`, `Cushort`, `Cwchar_t`, `Cstring`, `Cwstring`

---

### 3. 例外（Exceptions）

> **関連 Issue**: [#342](https://github.com/AtelierArith/ailujsoi/issues/342), [#534](https://github.com/AtelierArith/ailujsoi/issues/534)〜[#553](https://github.com/AtelierArith/ailujsoi/issues/553)

#### 未実装例外（主要）

| 例外 | 説明 |
|-----|------|
| `CanonicalIndexError` | インデックスエラー |
| `CapturedException` | 捕捉例外 |
| `CompositeException` | 複合例外 |
| `EOFError` | ファイル終端 |
| `InvalidStateException` | 無効状態 |
| `MissingException` | Missing 例外 |
| `ProcessFailedException` | プロセス失敗 |
| `TaskFailedException` | タスク失敗 |
| `SystemError` | システムエラー |

---

### 4. グローバル定数・変数

> **関連 Issue**: [#340](https://github.com/AtelierArith/ailujsoi/issues/340)

| 定数/変数 | 説明 | 状態 |
|----------|------|------|
| `ENV` | 環境変数 | ❌ |

---

---

### 7. 配列関数

> **関連 Issue**: [#353](https://github.com/AtelierArith/ailujsoi/issues/353)

#### 未実装
| 関数 | 説明 |
|-----|------|
| `circcopy!` | 循環コピー（破壊的）（`circshift!` は実装済み） |
| `extrema!` | 最大最小（破壊的） |
| `hvcat`, `hvncat` | 水平垂直連結 |
| `parent`, `parentindices` | 親配列（`SubArray` のみは実装済み。一般化は未対応） |
| `promote_shape` | 形状プロモーション |
| `to_indices` | インデックス変換 |
| `view` | ビュー作成（✅ 部分実装: 現状 `Vector{Float64}` の 1D view が中心） |

#### 実装済み（Pure Julia — array.jl, sort.jl）
| 関数 | 説明 |
|-----|------|
| `maximum!`, `minimum!` | 最大最小（破壊的） |
| `sum!`, `prod!` | 積/和（破壊的） |
| `permutedims!` | 次元転置（破壊的、2D/3D 対応） |
| `partialsortperm`, `partialsortperm!` | 部分ソート順列 |
| `sortperm!` | ソート順列（破壊的） |
| `sortslices` | スライス単位ソート（`dims` キーワード引数） |

#### 実装済み (Issues #1942, #1946, #1952, #1958, #2153, #2157)
| 関数 | 説明 |
|-----|------|
| `stack` | 1D 配列をマトリクスの列として結合 |
| `selectdim` | 次元 d のインデックス i でスライスを選択 |
| `dropdims` | シングルトン次元の除去（`dims` キーワード引数） |
| `insertdims` | シングルトン次元の挿入（`dropdims` の逆操作、Issue #2153） |
| `eachrow` | 行ごとのイテレータ（`EachRow` 構造体、`iterators.jl` で実装） |
| `eachcol` | 列ごとのイテレータ（`EachCol` 構造体、`iterators.jl` で実装） |
| `cat` | 汎用連結（`dims` キーワード引数、vcat/hcat の一般化） |
| `eachslice` | スライスイテレータ（`EachSlice` 構造体、`iterators.jl` で実装） |
| `mapslices` | スライスへの map（関数型配列操作） |
| `sum(; dims)` | 次元指定の合計（dims=1: 列、dims=2: 行） |
| `prod(; dims)` | 次元指定の積 |
| `maximum(; dims)` | 次元指定の最大値 |
| `minimum(; dims)` | 次元指定の最小値 |
| `extrema(; dims)` | 次元指定の (min, max) タプル |
| `stride`, `strides` | 列方向ストライド（Issue #2157） |

---

### 9. 線形代数

> **関連 Issue**: [#349](https://github.com/AtelierArith/ailujsoi/issues/349), [#851](https://github.com/AtelierArith/ailujsoi/issues/851) (builtin認識バグ修正済み)

#### 未実装
| 関数 | 説明 |
|-----|------|
| `.'` | 転置演算子（非共役 transpose） |

#### 実装済み (Issues #1921-#1960)
| 関数 | 説明 |
|-----|------|
| `pinv` | Moore-Penrose 擬似逆行列（SVD ベース） |
| `eigvecs` | 固有ベクトル行列の抽出 |
| `normalize` | ベクトルの正規化（1-arg, 2-arg） |
| `diag` | 対角要素の抽出（1-arg, 2-arg with offset） |
| `issymmetric` | 対称行列の判定 |
| `ishermitian` | エルミート行列の判定 |
| `triu` | 上三角行列の抽出（1-arg, 2-arg） |
| `tril` | 下三角行列の抽出（1-arg, 2-arg） |
| `diagm` | ベクトルから対角行列を作成 |
| `opnorm` | 作用素ノルム（p=1, 2, Inf） |
| `nullspace` | 零空間の正規直交基底（SVD ベース） |
| `logdet` | 行列式の対数 |
| `logabsdet` | 行列式の絶対値の対数と符号 |
| `adjoint` | 共役転置 |
| `isdiag` | 対角行列の判定 |
| `istriu` | 上三角行列の判定（1-arg, 2-arg with offset） |
| `istril` | 下三角行列の判定（1-arg, 2-arg with offset） |
| `isposdef` | 正定値行列の判定（手動 Cholesky） |
| `hermitianpart` | エルミート部分: `(A + adjoint(A)) / 2` |
| `eigmax` | 最大固有値 |
| `eigmin` | 最小固有値 |
| `checksquare` | 正方行列の判定（サイズ n を返す） |
| `axpy!` | BLAS Level 1: `Y = a*X + Y`（破壊的） |
| `axpby!` | BLAS Level 1: `Y = a*X + b*Y`（破壊的） |
| `rmul!` | 右乗算: 配列をスカラーでスケーリング（破壊的） |
| `lmul!` | 左乗算: 配列をスカラーでスケーリング（破壊的） |
| `mul!` | 行列積（in-place）: `C = A*B` および `C = α*A*B + β*C` |
| `ldiv!` | 左除算（in-place）: ガウス消去法 |
| `rdiv!` | 右除算（in-place）: 転置ガウス消去法 |
| `kron!` | クロネッカー積（in-place）: `C = kron(A, B)` |

---

### 11. コレクション

> **関連 Issue**: [#351](https://github.com/AtelierArith/ailujsoi/issues/351)

#### 未実装
| 関数 | 説明 |
|-----|------|
| `all!`, `any!`, `count!` | 破壊的バージョン |

#### 実装済み (Issues #1813, #2746)
| 関数 | 説明 |
|-----|------|
| `memoryref` | メモリ参照 (Memory{T} primitive — PR #2776) |

#### 実装済み (Issue #1813)
| 関数 | 説明 |
|-----|------|
| `mergewith`, `mergewith!` | Dict マージ（カスタム結合関数付き） |

#### 実装済み (Issue #1810)
| 関数 | 説明 |
|-----|------|
| `sizehint!` | サイズヒント（no-op） |

#### 実装済み (Issue #1454, #1122, #1459)
| 関数 | 説明 |
|-----|------|
| `getkey` | キー取得 |
| `keytype`, `valtype` | キー/値型 |
| `intersect!`, `setdiff!`, `symdiff!`, `union!` | 集合演算（破壊的） |
| `in!` | 包含チェック＆挿入 |

---

### 12. 文字列

#### 未実装
| 関数 | 説明 |
|-----|------|
| `hex2bytes!` | バイト⇔16進（破壊的） |
| `ctruncate`, `ltruncate`, `rtruncate` | 切り詰め |
| `digits!` | 桁配列（破壊的） |
| `replace!` | 置換（破壊的） |
| `transcode` | トランスコード |

#### 実装済み (Issue #1994, #3457)
| 関数 | 説明 |
|-----|------|
| `eachrsplit` | 逆方向分割イテレータ（Pure Julia、`EachRSplit` 構造体） |
| `Char(n)` 範囲チェック | `n < 0` または `n > 0x10FFFF` のとき `TypeError` を送出。`n as u32` による無音ラップを修正 (Issue #3457) |

---

### 13. テキスト出力

> **関連 Issue**: [#337](https://github.com/AtelierArith/ailujsoi/issues/337) (IOContext full support)

#### 実装済み (Issue #1461)
| 関数 | 説明 |
|-----|------|
| `showerror` | エラー表示 |
| `summary` | サマリ |

---

---

### 18. タスク・同期（一部実装）

> **関連 Issue**: [#348](https://github.com/AtelierArith/ailujsoi/issues/348) (基本API実装済み), [#3432](https://github.com/AtelierArith/ailujsoi/issues/3432) (task macro 互換), [#444](https://github.com/AtelierArith/ailujsoi/issues/444) (残タスク)

#### 実装済み (Issue #348)
| 型/関数 | 状態 |
|---------|------|
| `Task` | ✅ VM frame/stack continuation を持つ cooperative Task (Issue #10349) |
| `@task` | ✅ thunk を `Task` に包む compiler macro (Issue #3432) |
| `@async` | ✅ `Task` を作成して VM の FIFO runnable queue に `schedule` する compiler macro (Issues #3432, #8989, #10349) |
| `@sync` | ✅ body 内の実 Task 集合を待機し、失敗を `CompositeException` に集約 (Issues #3432, #10349) |
| `schedule` | ✅ VM task table / runnable queue 登録。body は scheduler yield point で開始 (Issue #10349) |
| `fetch` | ✅ `Task` 結果取得、非 Task は恒等 |
| `wait(::Task)` | ✅ 現 Task を park し、対象 Task 完了時に continuation を wake (Issue #10349) |
| `istaskdone`, `istaskstarted`, `istaskfailed` | ✅ Task 状態確認 |
| `yield()` | ✅ 呼び出し位置で continuation を suspend し次の runnable task へ切替 (Issue #10349) |
| `yield(::Task)` | ✅ 対象 Task を schedule して cooperative wait (Issue #10349) |
| `ReentrantLock`, `SpinLock` | ✅ 単一スレッド向け簡易 lock |
| `lock`, `unlock`, `trylock`, `islocked` | ✅ lock 操作 |
| `Condition` | ✅ waiter continuation queue (Issue #10349) |
| `notify(::Condition)` | ✅ one/all waiter wake と通知値 (Issue #10349) |

#### 制限
- native Rust re-entry (`run_until_frame_return` を使う eval / HOF / show / iteration 等) の内側では continuation を capture できず、明示的で catch 可能な `cannot suspend task across a native VM call boundary` を返す。
- 全 Task が block した場合、upstream の無期限待機ではなく CI/iOS 向けに catch 可能な deadlock error を返す。

#### 未実装
`asyncmap!` (`yieldto` は FIFO scheduler 経由の部分互換)

> `current_task`, `task_local_storage`, `waitany`, `waitall`, `errormonitor` は VM task completion と連動する。
> `asyncmap` は #3500 で cooperative single-thread 実装済み。OS-thread parallelism は対象外。

---

### 19. チャンネル

> **関連 Issue**: [#348](https://github.com/AtelierArith/ailujsoi/issues/348) (基本API実装済み), [#445](https://github.com/AtelierArith/ailujsoi/issues/445) (大部分解決済み)

#### 実装済み (Issue #348 + #445 + #3450 + #3451 + #3454 + #3455 + #3456 + #10349)
| 型/関数 | 状態 |
|---------|------|
| `Channel{T}` | ✅ 型パラメータ付き parametric struct (Issue #3450) |
| `put!` | ✅ buffered full / unbuffered rendezvous で現 Task を park (Issue #10349) |
| `take!` | ✅ 空なら park、FIFO 値取得後に blocked putter を wake (Issue #10349) |
| `fetch` | ✅ 空なら park、先頭値を非破壊取得 (Issue #10349) |
| `isopen`, `isbuffered` | ✅ 状態確認 |
| `isfull` | ✅ 状態確認。unbuffered channel で常に `true` を返すバグ修正済み (Issue #3456) |
| `isready`, `isempty` | ✅ 即時取得可能な buffer 値で判定 (Issue #10349) |
| `close` | ✅ close と close with exception |
| `length` | ✅ sjulia compatibility extension: 即時取得可能な buffer 値数 |
| `iterate`, `push!`, `popfirst!` | ✅ collection 互換 API |
| `empty!` | ✅ buffer を空にし blocked putter を wake (Issue #10349) |
| `bind(c, task)` | ✅ タスク完了時にチャンネル自動クローズ (Issue #445) |
| `Channel(func, size)` | ✅ live producer Task。完了時 close、失敗時は buffered 値消費後に `TaskFailedException` (Issues #3455, #10349) |

#### 残制限
- native Rust re-entry floor 内の blocking operation は Task section 記載の明示エラーになる。
- 全 Task block 時は無期限 hang ではなく deadlock error を返す。

---

### 20. Missing 値

| 関数 | 状態 |
|-----|------|
| `nonmissingtype` | ✅ 実装済み (Issue #1316) |

---

### 21. 時間

| 関数 | 状態 |
|-----|------|

---

### 22. エラー処理（大部分未実装）

`backtrace`, `catch_backtrace`, `current_exceptions`, `systemerror`, `stacktrace`

---

### 23. 型・プロパティ

#### 未実装
| 関数 | 説明 |
|-----|------|
| `isdispatchtuple` | ディスパッチタプル判定 |
| `instances` | 列挙インスタンス |
| `typeintersect` | 型共通部分（部分実装済み: Union distribution、Tuple、invariant parametric、Vararg/NTuple、diagonal `Tuple{T,T} where T` の bound narrowing、および代表的な `Tuple{T,Vector{T}} where T` invariant-container occurrence は対応。残りは #5048 の arbitrary nested / lower-bound-heavy full set-theoretic UnionAll） |

#### 実装済み (Issue #1450, #1451, #1463)
| 関数 | 説明 |
|-----|------|
| `getproperty`, `setproperty!` | プロパティ操作 |
| `fieldindex`, `fieldtypes`, `fieldtype(T, Symbol)` | フィールド情報 |
| `fieldname`, `fieldnames` | フィールド名取得 |
| `propertynames`, `hasproperty` | プロパティ存在確認 |
| `setfield!` | フィールド設定（builtin） |
| `oftype` | 型変換 |

---

### 25. リフレクション・ヘルプ（大部分未実装）

`code_typed`, `code_lowered`, `fullname`, `functionloc`, `isconst`, `isinteractive`, `parentmodule`, `pathof`, `pkgdir`, `pkgversion`, `names`, `@invoke`, `invokelatest`, `@invokelatest`, `@world`

---

### 26. ソースファイル読み込み

| 関数 | 状態 |
|-----|------|
| `__precompile__` | ❌ |
| `evalfile` | ✅ 実装済み（eval 対応式のみ） |
| `include_string` | ✅ 実装済み（eval 対応式のみ） |
| `include_dependency` | ❌ |
| `include` | ⚠️ Native のみ |

---

### 27. RTS 内部（部分実装）

`precompile`

`GC.gc` / `GC.safepoint` / `GC.in_finalizer` と `finalizer` / `finalize` は
Issue #8990 で基本実装済み。

---

### 28. I/O・イベント（大部分未実装）

> **関連 Issue**: [#347](https://github.com/AtelierArith/ailujsoi/issues/347), [#9577](https://github.com/AtelierArith/ailujsoi/issues/9577), [#9578](https://github.com/AtelierArith/ailujsoi/issues/9578)

#### 実装済み / 部分実装

- IO stream cursor subset (Issue #8996): `IOBuffer`, `open(path[, mode])`, `close(io)`,
  `isopen(io)`, `eof(io)`, `flush(io)`, `position(io)`, `seek(io, n)`,
  `skip(io, n)`, `read(io, Char)`, `readline(io)`。
- File path helpers: `countlines`, `eachline`, `readdir`, `readline(path)`,
  `readlines(path)` は既存実装済み。
- `write(io, x)` は IOBuffer/file/stdout/stderr/devnull に書き込み byte count を返す。
  ただし numeric raw-byte semantics は未対応 (#9578)。
- `redirect_stdout(stream)` / `redirect_stderr(stream)` と do-block 形式
  `redirect_stdout(f, stream)` / `redirect_stderr(f, stream)` は stdout/stderr の
  VM sink を一時差し替える。`redirect_stdio(f; stdout=..., stderr=...)` は
  stdout/stderr subset を合成する。`Pipe()` は最小 IO subtype surface として利用可能
  (Issue #9577)。stdin redirection と libuv pipe I/O は未実装。

#### 未実装 / 残スコープ

`closewrite`, `readeach`, `fd`, `fdio`, `gethostname`, `htol`, `hton`, `ltoh`, `ntoh`, `ismarked`, `isreadonly`, `mark`, `unmark`, `reset`, `bytesavailable`, `peek`, `pipeline`, full libuv `Pipe` I/O, `PipeBuffer`, `seekend`, `seekstart`, `skipchars`, `RawFD`, `read!`, `readavailable`, `readbytes!`, `readchomp`, `readuntil`, `copyuntil`, `copyline`, `redirect_stdin`, full `redirect_stdio` stdin/path/file-descriptor support, `truncate`, `unsafe_read`, `unsafe_write`

---

### 29. マルチメディア I/O（部分実装）

> **関連 Issue**: [#455](https://github.com/AtelierArith/ailujsoi/issues/455) (PR #1559 で部分実装済み)

#### 実装済み (Issue #1559, 2026-01-25)
| 関数/型 | 説明 |
|--------|------|
| `AbstractDisplay` | 抽象ディスプレイ型 |
| `TextDisplay` | テキストディスプレイ型 |
| `MIME` | MIME 型コンストラクタ |
| `@MIME_str` | `MIME"text/plain"` リテラル |
| `display` | 値の表示（text/plain のみ） |
| `displayable` | テキスト MIME は true |
| `istextmime` | テキスト MIME 判定 |
| `showable` | text/plain は true |
| `redisplay` | display へ委譲 |
| `pushdisplay`, `popdisplay` | スタブ実装（制限あり） |

#### 未実装
| 関数/型 | 説明 |
|--------|------|
| `HTML` | HTML 型（表示用） |
| `Text` | Text 型（表示用） |

> 制限: ディスプレイスタックの完全なバックエンド選択機能は未完成

---

### 30. パス・ファイル名（大部分未実装）

> **関連 Issue**: [#346](https://github.com/AtelierArith/ailujsoi/issues/346)

`abspath`, `expanduser`, `contractuser`, `homedir`, `normpath`, `realpath`, `relpath`, `splitdrive`

---

### 31. ファイルシステム操作（全て未実装）

`cd`, `chmod`, `chown`, `cp`, `ctime`, `diskstat`, `download`, `filemode`, `filesize`, `gperm`, `hardlink`, `isblockdev`, `ischardev`, `isdir`, `isexecutable`, `isfifo`, `isfile`, `islink`, `ismount`, `ispath`, `isreadable`, `issetgid`, `issetuid`, `issocket`, `issticky`, `iswritable`, `lstat`, `mkdir`, `mkpath`, `mktemp`, `mktempdir`, `mtime`, `mv`, `operm`, `pwd`, `readlink`, `rm`, `samefile`, `stat`, `symlink`, `tempdir`, `tempname`, `touch`, `uperm`, `walkdir`

---

### 32. 外部プロセス（全て未実装）

`detach`, `getpid`, `ignorestatus`, `kill`, `process_exited`, `process_running`, `run`, `setenv`, `addenv`, `setcpuaffinity`, `setuid`, `setgid`, `success`, `withenv`

---

### 33. C インターフェース（全て未実装）

`@cfunction`, `@ccall`, `cglobal`, `disable_sigint`, `pointer`, `pointer_from_objref`, `unsafe_wrap`, `unsafe_string`, `reenable_sigint`, `unsafe_copyto!`, `unsafe_load`, `unsafe_modify!`, `unsafe_pointer_to_objref`, `unsafe_replace!`, `unsafe_store!`, `unsafe_swap!`

---

### 34. マクロ

#### 未実装（パーサー/ノテーション）

> **関連 Issue**: [#554](https://github.com/AtelierArith/ailujsoi/issues/554) (string macro literals), [#556](https://github.com/AtelierArith/ailujsoi/issues/556) (big_str)

`@__FUNCTION__`, `@cmd`, `@Kwargs`, `@lazy_str`

> **実装済み**: `Int128"..."`, `big"..."`, `s"..."`, `text"..."`, `html"..."`, `raw"..."`, `r"..."`, `v"..."`, `b"..."`, `MIME"..."`, `@NamedTuple{...}` / `@NamedTuple begin...end` (Issue #5120)、型レベル `NamedTuple{(:a,:b)}` / `NamedTuple{(:a,:b),Tuple{...}}` の isa/subtype/dispatch/construct (Issue #5063; 完全ジェネリック形 `NamedTuple{names,T} where {names,T}` の variance/diagonal は継続追跡)

#### 未実装（プロファイリング）

> **関連 Issue**: [#350](https://github.com/AtelierArith/ailujsoi/issues/350)

`@lock_conflicts`

#### 未実装（タスク）
`@threadcall`

> **実装済み**: `@task`, `@async`, `@sync` (Issue #3432; 逐次・協調モデル互換)、
> `Threads.@threads`, `Threads.@spawn` (Issue #8991; single-thread compatibility shim)

#### 未実装（パフォーマンス）
`@fastmath`, `@specialize`, `@polly`

> **実装済み（compatibility slice）**: `@boundscheck`, `@inbounds`, `@inline`, `@noinline`,
> `@nospecialize`, `@constprop`, `@assume_effects`
> (`@nospecializeinfer function ... end` は Issue #4286 の構文互換 slice)

#### 未実装（その他）
`@atomic`, `@atomicswap`, `@atomicreplace`, `@atomiconce`, `@__dot__`, `@main`

> **実装済み**: `@enum` (基本機能), `@static`, `@label`, `@goto`

---

## 最近実装された機能（2026-02-13 → 2026-02-22）

以下の機能は以前このリストに含まれていたが、現在は実装済み:

| 機能 | Issue/PR | 状態 |
|------|----------|------|
| `methods(f, types)` 型フィルタ付きメソッド検索 | Issue #3257, PR #3272 | ✅ 実装済み |
| Narrow integer typed locals routing | Issues #3255, #3321 | ✅ 実装済み |
| Named-function splat calls のランタイムディスパッチ | Issue #3256, PR #3323 | ✅ 実装済み |
| REPL/Literal pipeline: Char, narrow ints, F32, Regex, Enum, Float16 | Issues #3293-#3316 | ✅ 実装済み |
| Struct field persistence の自動同期 | PR #3315 | ✅ 実装済み |

### REPL Persistence の残課題

| 項目 | Issue | 状態 |
|------|-------|------|
| BigInt/BigFloat の REPL injection | Issue #3301 | ❌ 未実装（Literal 表現なし） |
| GlobalRef, Pairs, Set, RegexMatch の injection | Issue #3301 | ❌ 未実装（Literal 表現なし） |
| Memory{T} Phase 5: native-array compatibility 縮小 | Issue #3908/#4568/#6653 | ✅ `Value::Array(ArrayRef)` enum バリアントは撤去済み。`scripts/check_value_array_allowlist.sh` は zero-match audit。public route は `Array{T,N}` wrapper へ移行済みで、`Value::NativeArray` は cache/VM/host 互換境界として残す。 |
| Array{T,N} wrapper foundation | Issue #6648/#6649/#6653 | ✅ faithful `Array{T,N} <: DenseArray{T,N}` + `MemoryRef{T}` storage、public constructor/materialization routing、native carrier 降格まで完了済み。 |

---

## 🐛 既知のバグ

既知のバグは GitHub Issues で管理しています:
https://github.com/AtelierArith/ailujsoi/issues?q=is%3Aissue+is%3Aopen+label%3Abug

---

## 未サポート Julia 基本構文（網羅的一覧）

> **原則**: SubsetJuliaVM は Julia の基本構文をサポートする。妥協は許されない。
> 以下は現時点で未サポートの基本構文の完全なリスト。

### リテラル構文

| 構文 | 例 | 状態 | 回避策 |
|------|-----|------|--------|
| 大きな整数リテラル | `9223372036854775808` | ⚠️ BigFloat として解析 | **Issue**: [#316](https://github.com/AtelierArith/ailujsoi/issues/316) |
| コマンドリテラル | `` `ls -la` `` | ❌ | プロセス実行未サポート (iOS制約) |

### 制御フロー構文

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| `try...catch...else` | `try ... catch ... else ... end` | ❌ | **Issue**: [#317](https://github.com/AtelierArith/ailujsoi/issues/317) |
| `@goto` / `@label` | `@goto label; @label label` | ✅ | 低レベル制御フロー（実装済み） |

### 内包表記・ジェネレータ

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| 多次元内包表記 | `[i+j for i in 1:3, j in 1:3]` | ❌ | 複数 `for` 句（ネスト/多重）は未対応 |
| Dict 内包表記 | `Dict(k => v for k in iter)` | ✅ | 辞書生成（テスト: `collections/dict_comprehension.jl`） |
| Set 内包表記 | `Set(x for x in arr)` | ✅ | 集合生成（テスト: `collections/set_comprehension.jl`） |

### 関数定義構文

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| 複数 dispatch 無名関数 | 複数メソッドの無名関数 | ❌ | - |

### 関数呼び出し構文

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| キーワード引数省略記法 | `f(;x, y)` | 🐛 | **Issue**: [#1288](https://github.com/AtelierArith/ailujsoi/issues/1288) - 回避策: `f(;x=x, y=y)` を使用 |

### 型システム構文

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| `typealias`（廃止） | - | ❌ | Julia 0.6 で廃止 |

### 文字列構文

| 構文 | 例 | 状態 | 説明 |
|------|-----|------|------|
| LaTeX 文字列 | `L"x^2"` | ❌ | - |
| HTML 文字列 | `html"<b>bold</b>"` | ❌ | - |

## ⚠️ Subset Compatibility 違反関数

**最終更新**: 2026-01-13 (range, collect, big 実装済みに更新)

> **注意**: SubsetJuliaVM Base にあるが Julia 本家 Base に存在しない関数は、本来存在してはいけません。
> これらの関数は互換性を壊すため、削除または Julia 準拠の実装に置き換える必要があります。

### 現在の違反関数

（削除済み - `_insertion_sort!` は `sort!` にインライン化されました）

### 実装が異なる関数（互換性注意）

以下の関数は Julia 本家にも存在するが、SubsetJuliaVM では簡略化されており動作が異なる:

| 関数 | SubsetJuliaVM | Julia 本家 | 影響 |
|------|---------------|------------|------|
| `sprint` | 1-4引数の固定関数 | 可変長引数 + context | 5引数以上で非互換 |
| `dump` | 汎用構造体対応、大配列制限あり | 詳細な構造表示 | 出力フォーマットがやや異なる（改善済み） |

### 型プロモーション / convert の制限

| 制限 | 説明 | Issue |
|------|------|-------|
| ~~`convert(::Type{T}, x::T) where {T}` が struct 型に効かない / ユーザー定義 `Base.convert` が builtin に shadow される~~ | **修正済み**: `where T` の再束縛照合と `BuiltinId::Convert` の method-priority fallback により、ユーザー定義 `Base.convert(::Type{T}, x)` が Rust fallback より先に実行される | Issue #2468 / #3764 (Fixed) |
| `promote_rule` が具体型のみ | ジェネリック `where {T,S}` パターンは未対応。`Rational{Int64/Int32/Int16/Int8}` と各整数型・浮動小数点型の具体的な組み合わせを網羅的に定義済み | - |
| ~~`Int32 + Rational{Int32}` 未対応~~ | **修正済み**: `Int8/Int16/Int32` + `Rational{T}` の混合型演算をサポート。コンパイラが `(Primitive, Struct)` の組み合わせをランタイムディスパッチに委譲し、`+(::Number, ::Number)` プロモーション経由で解決 | Issue #2475 (Fixed) |
| 整数演算の型保持が不完全 | `MulInt`/`AddInt` 等の intrinsic は常に `Value::I64` を返す。`Rational{Int16}` 同士の四則演算は中間値が `Int64` に昇格し `Rational{Int64}` を返す | - |
| ~~`Rational{T}(...)` 明示型パラメータコンストラクタが不完全~~ | **修正済み**: `Rational{Int8}(6,4)` が要素型 `T` へ強制 + 約分(`3//2`)、単一引数 `Rational{Int8}(3)` と `Rational{T}(r::Rational)` 変換をサポート。完全名テーブル参照 + `CallTypedDispatch` による実行時多重ディスパッチ | Issue #5132 (Fixed) |
| cross-type `Rational{BigInt}` 算術が未対応 | `Rational{BigInt} + Rational{Int64}`(混合型 Rational 同士の四則演算)は `expected I64, got BigInt` で失敗する。origin/main から存在する pre-existing 制約で #5132 の範囲外 | #5151 epic (#5153/#5154) |

### BigInt/BigFloat 関連の制限

BigInt/BigFloat の提供機能は `DONE.md` に集約しています。ここでは残っている互換性上の注意点のみ扱います。

2026-06-02: `BigInt` / `BigFloat` の `===` reference identity は Issue #4886 で
upstream Julia と一致済み。same-value independent allocation は `false`、alias は
`true`、`==` は value equality を維持する。

2026-06-18: `BigFloat` の `floor`/`ceil`/`round`/`trunc` と `div`/`fld`/`cld`/
`divrem`/`fldmod` は Issue #6801 で対応済み(`DONE.md`)。関連も対応済み:
`Int(::BigFloat)`(→ `floor(Int, ::BigFloat)`)変換(#6890、`DONE.md`)、
汎用 `div`/`divrem` が負値で floor(本来 trunc)になる既存バグ(#6891)、
tuple `==` の BigFloat 要素比較(#6892)。

> **注意**: これらの関数はコードの互換性は保たれるが、出力や型が異なる場合がある。

---

## 実装不可/非推奨の項目

以下は iOS App Store 制限（JIT 禁止）やアーキテクチャ制限により実装困難:

| 項目 | 理由 |
|-----|------|
| タスク/スレッド全般 | マルチスレッドランタイムが必要 |
| ファイルシステム操作 | サンドボックス制限 |
| 外部プロセス実行 | セキュリティ制限 |
| C インターフェース | FFI が必要 |
| 完全な線形代数 | 行列数学ライブラリが必要 |

> **注**: `@generated` は Phase 1/2/3 で部分サポート済み（フォールバック + Val 特殊化 + Quote アンクォート）

---

### Tier 3（高労力）
（`mapslices` は実装済み - Issue #1952）

### 非推奨（アーキテクチャ制限）
- 完全な線形代数 - 行列数学ライブラリが必要
- 注: `@generated` は Phase 1/2/3 で部分サポート済み（上記「部分対応構文」参照）

---
