# 現状分析

**最終更新**: 2026-07-20. 新しい項目は下の日付別「最新対応」セクションを正とし、先頭メタデータには長い issue 要約を重複させない。

> - 実装済みの機能は [DONE.md](./DONE.md) を参照してください。
> - 未実装の機能は [UNIMPLEMENTED.md](./UNIMPLEMENTED.md) を参照してください。
> - 更新方針 (Issue #3760): 新しい項目は日付ごとの共有 `## ...YYYY-MM-DD...` 見出しの下に、Issue ごとの `### ... (Issue #NNNN)` 小見出しとして追加する。同日の見出しが既にある場合は、その下に新しい「最新対応」ブロックを増やさない。
> - 3,000 行の live-file budget を超えた過去分は [archive/STATUS-2026.md](./archive/STATUS-2026.md) にアーカイブ済み (Issues #6341/#11263)。年が変わったら前年分を `archive/STATUS-<YYYY>.md` へ移す。

---

## 最新対応 (2026-08-18)

### Experimental general Wasm AoT backend

`aot-wasm` は既存の parse/lower/inference/optimization と backend-neutral
`IrModule` を再利用し、import のない standalone core Wasm を生成する。初期 subset
は Int64/Float64/Bool、direct call、branch/loop、v1 UInt8 descriptor load/store。
未対応 IR は diagnostic で拒否し fallback しない。Node E2E と RGBA 888×862
20-iteration benchmark の詳細は `COMPILER_SPIKE.md`。

## 最新対応 (2026-07-20)

### Windows の既定 LOAD_PATH 修正 (Issue #11800)

Windows でも既定 loader path が embedded stdlib と bundled packages を直接参照するようにし、
インストール済み REPL の初回評価で `InteractiveUtils` の自動 import が失敗しないようにした。
OS 固有 separator を含む文字列の再 parse を避け、構造化された既定 entry を生成する。

### exception parity Phase 3 完了 (Issues #10813/#11148)

46-case corpus の exception type/catchability を upstream と比較し、Issue 付き
allowlist の新規乖離・stale 行を両方拒否する。full-suite premerge に配線済み。
現残差は #11559/#11794/#11390 の3件。generic fallback sweep で見つかった
`conj`/`isreal`/`flipsign` と `real`/`signbit`/`abs` の無型 signature は
`::Real` に修正し、#11522/#11525/#11797 fixture と corpus sentinel で固定。
残る math fallback 群は #11799 に追跡し、実行環境異常も ratchet が fail-closed に扱う。
詳細は DONE.md / EXCEPTION_PARITY.md。

### control-flow struct の inner constructor activation (Issue #11679)

non-parametric runtime struct の explicit inner constructor を予約 type ID に対して事前 compile し、宣言 marker 到達時に型 binding と constructor world を一括 publish する。未到達 branch は型・method とも不可視、raw/default constructor suppression、REPL の後続 eval と catchable error recovery も upstream semantics を維持する。parametric runtime struct は #11678 の残スコープ。詳細は DONE.md 参照。
### typed array literal の convert gate 廃止 (Issue #10835)

main compiler と runtime specializer は、すべての非空 `T[...]` 要素を無条件に
`convert(T, x)` してから `MemorySet` する。Base の convert 対応追加と手書き
型 allowlist の同期を不要にし、`Char[97]` も upstream 同様 `['a']` になる。
fixture は boxed container、Char、exact-type Regex control を、specializer unit test
は `Any[...]` にも Convert 命令があることを固定する。numeric-only Union target は
generic numeric method より先に member identity のみを処理し、user-defined Union
convert dispatch も維持 (#11781)。convert target は表示名を再構築せず元の型式を一度だけ
評価して再利用し、nested parametric Union の identity と要素評価中の binding 変更に対する
snapshot semantics を保持する (#11783)。dynamic target の論理 eltype metadata は #11787。
詳細は DONE.md 参照。
### chomp の multibyte 対応 (Issue #11642)

- multibyte 文字列末尾の `\n`/`\r\n` が正しく除去されるようになった(upstream lastindex/prevind 形)。

### AoT inline 残骸の bare path statement 解消 (Issue #10796)

- inliner が statement 位置の effect-free 結果を drop し、生成 crate が `-D warnings` でも通る形に。

### broadcast の Any-eltype per-element 型保存 (Issue #10787)

- non-concrete eltype 配列への broadcast は Any materialize → promote_typejoin narrow で upstream と一致(silent truncation 解消)。新規起票: #11776 (vec が flatten しない)。

### Semantic-ID Phase 4 TypeVar 分類の機械化 (Issue #10992)

14 箇所の生の `HashMap<String, CoreType>` を、単一 dispatch candidate 内だけで
使う `LexicalTypeBindings` と、完全な rendered type 名を入力とする純粋 parser
memo `RenderedTypeParseCache` の2 authority に集約した。監査はこの2宣言だけを
lexical boundary として除外し、未分類 site をゼロ固定する。同数置換 mutation
test により、旧 count-only ratchet では見逃した semantic map の差し替えも拒否する。
Struct 側は #11046 により両監査カウントがゼロで、残る Phase 4 は #11095/#11089
の function/method identity と using-scope visibility である。
### runtime specializer の typed array literal codegen (Issue #10746)

- `T[a, b]` literal を含む array 引数関数が specialization を維持。要素型マップは bytecode crate に共有化し main compiler と乖離しない構造に。

### regex match/findnext の範囲外 index エラー型 parity (Issue #10736)

- 3-arg `match` の past-end offset は ErrorException、regex `findnext` の `i<1` は InexactError を送出し upstream と一致(従来は両方 silent `nothing`)。

### SubstitutionString の AbstractString surface (Issue #10735)

- `s"abc" == "abc"` / `length(s"abc")` などが upstream と一致するようになった。base/strings/util.jl に upstream 形の転送メソッド + sjulia 都合の狭い具象メソッド群を追加し、CallDynamic の dispatch-miss パスに builtin fallback を追加(builtin 裏付け名に Pure Julia メソッドを足しても String 呼び出しが壊れない一般修正)。
- 新規起票: #11751 (unsupported-feature: 1-arg codeunit)、#11753/#11756/#11757 (macro 引数内リテラルの lowering 系)、#11754 (1-arg hash 非転送)、#11755 (string() 結果の直接 == 誤判定)。

### REPL error 前に到達した selective import の回復 (Issue #11748)

using/import 文の完了を owner module path + local index の activation として
bytecode/VM に記録し、catchable error 時は Main で実行済みの文だけを session
state へ保存する。未到達 import の静的 binding surface を持つ errored VM は再利用せず、
sanitized state から再構築するため、到達済み import は即時・full rebuild 後とも残り、
source-later import は callable にならない。詳細は DONE.md 参照。

## 最新対応 (2026-07-19)

### no-suffix conditional method の full-compile error 回復 (Issue #11745)

recovery eligibility の method count を hoisted-only から既存の
hoisted+inline named 合算 authority へ修正。source-later method が無くても、error
前に到達した conditional method が即時 probe と full rebuild 後の双方で残る。
詳細は DONE.md 参照。

### fresh full-compile error 後の到達済み method 回復 (Issue #11742)

runtime nominal を含まない fresh compile でも、error 前に到達した conditional
method の activation から live-VM recovery plan を作るよう修正。source method が
なく自動生成 constructor/helper marker だけの通常入力は recovery 対象外のまま。
未到達 method は復活せず、次の full rebuild 後も到達済み method が残ることを回帰 test で固定。
詳細は DONE.md 参照。

### plain String replacement の literal 化 (Issue #10721)

regex replace の plain String replacement が `$1` を誤展開する問題を修正
(builtin で `$` エスケープ)。`s"..."` の展開は不変。詳細は DONE.md 参照。

### HOF/callable sqrt の BigFloat 対応 (Issue #10604)

`map(sqrt, [big"2.0"])` の MethodError を修正(callable lane に builtin と同じ
BigFloat arm を追加)。broadcast lane の縮退は #11727 起票。詳細は DONE.md 参照。

### Undefined typed-empty-array head の UndefVarError 化 (Issue #10583)

`SomeUndefName[]` の silent `Any[]` 化を修正。identifier 形の未解決 head は
upstream 同様 `getindex(head)` へルーティング(undefined → UndefVarError)。
詳細は DONE.md 参照。

### UnionAll trailing unbounded binder の省略表示 (Issue #10505)

`Array{T,N} where {T<:Real,N}` → `Array{T} where T<:Real` の upstream
`show_can_elide` 正規化を実装(bounded binder は保持、#10635 ガード維持)。
詳細は DONE.md 参照。

### Rank>=3 Array 表示の upstream ;;-literal 化 (Issue #10385)

rank>=3 配列の print/string/show が内部表現を漏らしていた問題を修正。再帰
N-d compact renderer(dim-k は k 個のセミコロン + space)+ 空配列の undef
constructor 形式。詳細は DONE.md 参照。

### typed exception payload を keyed one-shot carrier に統一 (Issue #11647)

`MethodError`、`DomainError`、`TypeError`、`StringIndexError`、`ParseError`、
field-index `BoundsError` の独立 pending slot を単一の
`PendingExceptionPayloadCarrier` に置換した。producer は payload key から対応する
`VmError` を同時生成し、funnel は exception class 判定前に carrier を必ず一度
consume する。全6種の表駆動 test が exact/mismatch/internal/nested replacement/
unhandled clear/same-session recovery を固定し、4種の既存 fixture も real nested
catch/rethrow を追加した。source audit は disguised slot、consume 順序、required
registry row の drift を mutation test 付きで拒否する。

### Module-scope Ref index-assign の型保存 (Issue #10363)

module global `Ref` への `R[] = v` が Int64 を Float64 に破壊する問題を修正。
zero-index store は Ref cell 対象なので legacy F64 coercion を適用しない。
詳細は DONE.md 参照。

### print/println の引数全評価 → 書き込み順序 (Issue #10351)

多引数 print 系が引数評価と書き込みを交互に行い、後続引数の出力副作用が
本呼び出しの出力の後に出る乖離を修正。副作用があり得る場合のみ temp spill で
全引数先行評価。詳細は DONE.md 参照。

### Script/gate 保守 3 件 (Issues #11592 / #10946 / #11474)

loc_report.sh の crate-split 対応 (#11592)、corpus 不在 worktree での gate
false-green 防止 `SJULIA_REQUIRE_CORPUS` (#10946)、dispatch seed sweep の
`@time` 出力 normalization 登録簿 (#11474)。詳細は DONE.md 参照。

### Nested @testset の summary 集計 (Issue #10338)

外側 `@testset` の summary が最後の内側 set のカウントを重複表示していた flat
カウンタ簿記を、`testset_stack` frame(begin で push・end で親へ fold)に置換。
builtin 経路と legacy `Instr::TestSet*` 経路が同一 helper を共有。非 nested の
出力は不変。詳細は DONE.md 参照。

### .sjvmbc ロードの promotion registry replay (Issue #10339)

`.sjvmbc` 実行が空の promotion registry で reflection を走らせるギャップを修正。
形式 v7 で payload に promotion rules を同梱し、ロード時に Base cache ヒット経路と
同じ replay + initialized マークを実施。詳細は DONE.md 参照。
### Seeded PROGRAM_CACHE ヒットの compile context 復元 (Issue #10335)

seeded PROGRAM_CACHE ヒットが `compile_context: None` のまま返り fresh compile と
乖離する latent bug を修正。decode 直後に `.sjvmbc` / Base cache と同一の
`restore_compile_context_from_program` を通す (Issue #10265 parity invariant)。
注入駆動の回帰テストと CACHE_ARCHITECTURE.md の該当節を更新。詳細は DONE.md 参照。

### Module-owned type display の Main 可視性対応 (Issue #11365)

#11395 クラスタ第二弾。struct instance / typeof の表示を upstream の可視性規則
(Main から unqualified で到達可能なら bare、そうでなければ `Main.M.B`) に揃えた。
`using` import が Main に残す bare-leaf DataType global を authority に、
`Vm::run()` シード + global store choke point のインクリメンタル更新で thread-local
可視性レジストリを維持 (cache restore / REPL 継続対応)。詳細は DONE.md 参照。

### REPL の HOF / do-block / generator helper を held VM へ live install (Issue #9784)

通常の top-level lambda/HOF、do-block、generator body、filtered-generator
predicate が生成する marker-less function を fresh full recompile に送らず、
現在の REPL VM へ source method と一緒に append する。公開権威は従来の
source-function prefix count ではなく、`ReplDefinitionActivation` が指す
primary / refresh index set。set member は marker 到達まで dormant、非 member
helper は world 1 で即時可視となる。

catchable error 後も source activation の厳密な到達 prefix だけを commit し、
helper body は index 整列のため保持する。`__lambda_*` / `__do_block_*` /
`__gen_body_*` / `__gen_pred_*` は Julia-visible generic snapshot には入らない。
mixed helper+named method、error 前後の helper、filtered generator を含む回帰行は
`last_vm_build_nanos() == Some(0)` を検証する。#9784 の残りは
Base/preload method、parametric/inner-constructor/redefined type、module/import/
package/macro/type-alias/baremodule、opaque runtime `eval`、最終 mirror 撤去。

### Owner-aware identity cluster: alias 汚染と Array wrapper spoof を修正 (Issues #11452 / #11388 / #11395)

#11395 (short-name identity 債務) の第一弾。(1) lowering の bare alias fallback を
可視 owner (lexical 包含 + using/import エッジ) に制限し、sibling module の
builtin 同名 alias が別 module の signature 注釈を汚染する #11452 を修正。
(2) array-wrapper 認識述語を owner-aware 化し、user module の `Faux.Array` が
native array 高速経路に入る #11388 の spoof を解消 (display/field alias/routing/
specializer)。残: `isa(a, Base.Array)` リーク (signature 側 owner 解決が必要、
#11078 StructId 系)、#11365 display の `Main.M.B` 修飾 (Main 可視性 authority の
設計が必要 — 各 Issue にコメント済み)。#11360 は 0e679faf3 で修正済みを検証し close。

### @enum member 定数を upstream Dict 展開順で公開 (Issues #11656 / #11666)

`@enum` の member metadata と `instances(Enum)` は source order を保つ一方、
定数 binding は upstream `Base.Enums.@enum` と同じく integer-keyed `Dict` の
slot iteration 順で emit する。Julia 1.12 の 64-bit integer hash、linear probe、
load-factor rehash と probe-limit rehash の双方を bytecode 層の単一 authority
で再現し、compiler の `PushEnum` 順と VM の既存-global collision guard 順を
共有した。catchable collision 後の exact published subset は full rebuild 後も
同じ順序で replay される。

duplicate value/name は lowering で全件検出し、type/member を一つも公開する前に
拒否する。基底型の範囲変換と `UInt64`/`UInt128` までの wide value carrier は
#11667 で継続する。

## 最新対応 (2026-07-18)

### throw(value) が Exception 以外の任意の値をそのまま保持 (Issue #11554)

`throw(value)` はコンパイル時に `value` の静的型が `Struct`/`Any`/`Str` の
いずれでもない場合(`DataType`、数値、`Symbol`、`Tuple`、`Array` など)、
`ToStr` + `ThrowError` を経由して `ErrorException(string(value))` に
coerce していた。upstream は任意の値を throw 可能で `catch` は元の値を
そのまま bind する(例: `throw(Int32)` は `Int32` という `DataType` を
そのまま bind し、`ErrorException("Int32")` にはならない)。`throw` の
compile-time dispatch を単純化し、値の静的型に関係なく常に `ThrowValue`
(既存の `pending_exception_value` 経由の value-preserving 命令)を発行する
ように変更。これにより `Str` の特別扱いも撤去され、`throw("msg")` も upstream
同様に生の `String` を保持するようになった(従来は `ErrorException("msg")`
に wrap されていた)。`error(msg)` / `throw(ErrorException(...))` などの通常の
Exception 経路は無変更。fixture `exceptions_throw_type_value_preserve_11554`。
副産物として発見した、無関係の別バグ(local scope での型変数を使った
method 定義の dispatch 失敗)は Issue #11574 として起票済み(本 PR の
スコープ外)。

### 静的 Tuple{T,T} リテラル型引数の repeated where-param 無視を修正 (Issue #11490)

`same_tup(::Type{Tuple{T,T}}) where T = true; same_tup(::Type) = false` に対し
`same_tup(Tuple{Int,Int})` / `same_tup(Tuple{Int,String})` を STATIC call で
呼ぶと、両方が同じ `CallResolved` 候補に解決されていた(2 回目も `true`)。
runtime dispatch(変数束縛経由の呼び出し)は既に正しい。根本原因は Issue #11231
とは別: 単一引数 static dispatch の高速パス
`core_static_datatype_exact_match`(`subset_julia_vm_compile/src/compile/expr/call/dispatch.rs`)
が `Tuple{...}`/`Struct{...}` の where 束縛型パラメータの各要素を独立にチェックし、
兄弟要素間で状態を共有していなかったため、`Tuple{T,T}` の反復する `T` が任意の
要素型の組み合わせにマッチしていた(#11231 は一般 CoreType typemap マッチャが使う
struct-to-struct binding extractor 側の同種バグを修正済みだが、この高速パスは
`.find()` でマッチした時点で一般マッチャを完全にバイパスするため #11231 の修正の
恩恵を受けていなかった)。`HashMap<String, CoreType>` の型変数束縛を再帰マッチに
スレッドし、同名の where 型変数の 2 回目以降の出現は最初の束縛と一致必須に(不一致
なら一般 `table.dispatch` パスへフォールバック、これは既に正しい)。匿名の
covariant/contravariant bound placeholder 名 `"_"` は対象外(独立した existential
であり共有 binder ではないため)。`repeated_where_param_conflict_11231.jl`
fixture に `same_tup` ケースを追加、upstream julia 1.12.6 で検証済み。

### 3 つ目の sqrt ルータを exact-or-Any 不変条件に合わせる (Issue #11486)

`BuiltinOp::Sqrt`(`subset_julia_vm_compile/src/compile/expr/builtin.rs`)は
#11526(Issues #11436/#11468/#11469/#11481/#11510/#11511)が対応しなかった
sqrt ルータの最後の1つだった。「静的に Complex と分かっていない」分岐が
`Instr::SqrtF64` を無条件に発行しており、`Any`/`Union` の静的型や
Complex 以外の未解決 `Struct` を「実数と証明済み」として扱っていた。
`compile_builtin_math` のガードと `compile_sqrt` の既に正しい `Any` 分岐に
揃え、証明済みの exact primitive 数値型のみが `SqrtF64` に到達し、それ以外は
`CallTypedDispatchOrBuiltin` / `BigFloat` builtin に委譲するよう修正した。
`--dump-bytecode` で確認した結果、この分岐は現状 source からの `sqrt(...)`
呼び出し(scalar / generic / broadcast)経由では到達不能(`compile_sqrt` が
常に先取りする)であることを確認済み — 新規に観測可能なバグではなく、
一貫性・防御的な修正として #11486 をクローズする。#11526 が既に修正した
2 件の実バグの回帰テストは既存の `constructor_return_exact_or_any_11436.jl`
のままカバーする。lattice の provenance ビット再設計と単一 routing
authority への統合(#10461)は follow-up として残す。

### bundled StaticArraysCore が upstream 4 パラメータ SMatrix をモデル化 (Issue #11542)

`subset_julia_vm/packages/StaticArraysCore/src/types.jl` の `SMatrix{M,N,T}`
struct に upstream 由来の第4パラメータ `L`(`L == M*N`)を追加し、
`struct SMatrix{M,N,T,L} <: StaticMatrix{M,N,T}` とした(独立した bundled
StaticArrays パッケージ自身の `SMatrix` struct に対する #11432 の修正を踏襲)。
`using StaticArraysCore` を直接 `using` した場合の `W::SMatrix{2,2,Float64,4}`
field 注釈が `too many parameters for type StaticArraysCore.SMatrix` になら
なくなった。`SMatrix{M,N,T}` など不完全パラメータ化形は引き続き構築可能。
`try_make_static_array`(`subset_julia_vm_vm/src/vm/exec/struct_ops.rs`)は
`"StaticArrays."` 接頭辞のみを認識するため、Rust 側の変更は不要(修正前後とも
`StaticArraysCore.SMatrix` にはマッチしない)。fixture:
`static_arrays_core_smatrix_four_param_11542`。実装中に発見した既知の別ギャップ
(単一 flat-Tuple 引数での `SMatrix{M,N,T,L}` 構築が `check_array_parameters` の
長さ検証を素通りする — sjulia の dispatch がその呼び出し形に対して
auto-generated default `(data::Tuple)` inner constructor を選ぶため、real Julia
の specificity 規則と同じ挙動。real upstream は `new` を呼ぶ明示的 inner
constructor を持つため素通りしない)は Issue #11573 として起票し、本 PR では
未修正(#11542 のスコープ外)。

### UTF-8 string index validation を scalar/vector/range で統一 (Issue #11621)

VM-local validator が Julia の one-based code-unit index を valid character
start / numeric out-of-bounds / in-bounds non-character boundary に構造分類する。
scalar `String`/`StrBytes`、index vector、unit/step range endpoint は同じ分類を
使い、Julia inclusive endpoint から Rust exclusive byte end への変換は validation
後だけ行う。fixture matrix は ASCII/multibyte/malformed bytes の境界を固定する。

### Base cache schema fingerprint を必須 premerge ownership で固定 (Issue #10688)

schema fingerprint audit の `premerge_default=true` row を sync checker の
required ownership に追加した。negative self-test は registry row の弱化と、
manifest 記載済みの実 source だけを変更する root-cause case の双方を検証する。
maintainer checklist は新 fingerprint 時の `CACHE_VERSION` bump、version 履歴、
snapshot update を同一 PR で要求する。

### shadow audit の guard grammar を独立 matrix で固定 (Issue #11604)

compile-expression local-shadow audit は production Rust を走査する前に、
direct `name` / `name.as_str()` / whitespace 変形を受理し、unguarded 比較・
non-negated lookup・無関係な identifier を拒否する grammar matrix を実行する。
negative self-test は従来の unguarded source injection に加え、`.as_str()` 対応を
削除する sandbox mutation で専用診断を必須とし、carrier 移行時の false red
を予防する。

### invoke declared signature を4 callable lane で固定 (Issue #11619)

direct/stored callable × static/runtime-held signature × positional/keyword
を交差し、declared `Any` / `Integer` の双方を upstream parity で検証する16セル
matrix を追加した。stored callable は4種の `InvokeFunctionVariable*` opcodeを
実際に emit することも bytecode regression で固定する。shared call audit は全4
arm が共通 declared-signature helper を使うことを要求し、通常call用のvalue-based
runtime refinementへ差し替える mutation を検出する。これによりliteral `Any`を
推論holeとして扱った #11609 の再発を防ぐ。

### abstract / primitive / enum を held REPL VM 上で source-order 公開 (Issues #11635 / #9784)

Main に新規定義する abstract type、非 parametric primitive type、`@enum` は、
function / concrete struct と同じ live definition transaction に入る。compiler
が整列済み registry tail を先に生成しても、runtime binding は
`DefineEvalAbstractType` / `DefineEvalPrimitiveType` / `RegisterEnum` の source
位置に到達するまで private のままなので、宣言前の `isdefined` と bare-name
read は upstream と同じく false / `UndefVarError` になる。後続 eval では
subtype dispatch、primitive の `sizeof` / conversion、enum constructor /
`instances` / display を held VM のまま使用し、`last_vm_build_nanos() == Some(0)`。

catchable error 時は function・concrete・abstract・primitive・enum が混在する
activation trace の厳密な到達 prefix だけを VM / compiler snapshot / session
mirror へ commit し、未到達 type と enum member は公開しない。live VM を take
する前に specialization / activation / nominal queue の全 setup を preflight し、
拒否された activation 設定は既存状態を変更しない。thread-local enum registry
も guard が未 commit transaction を復元する。cache schema は 172。残る #9784
は parametric / inner-constructor struct、type redefinition、Base/preload-owned
method、module/import/macro/type-alias/baremodule、opaque runtime `eval` と最終
full-recompile mirror 撤去。

### ParseError / StringIndexError の placeholder payload を撤去 (Issues #11572/#11615/#11618)

parser 起点の `ParseError.detail` は `nothing` ではなく、
`JuliaSyntax.ParseError(SourceFile, Vector{Diagnostic}, incomplete_tag)` を保持する。
`SourceFile` は parse 対象 substring、absolute `byte_offset`、1-based
`line_starts` を、各 `Diagnostic` は absolute 1-based byte span、`:error`、span を
含まない構造化 message を保存する。`Meta.parse(source, start)` の shifted parse
も upstream と同じ offset/span になる。`Base.JuliaSyntax` の binding 自体は
#11614 のままなので、fixture は owner suffix と全 field を検証する。

`StringIndexError.string` は空文字 placeholder ではなく、raise 元の exact
`Str` / `StrBytes` を一回限りの message-keyed carrier で funnel に渡す。scalar、
index-vector、range の全 producer を同じ atomic helper に接続し、in-bounds の
UTF-8 continuation byte を vector でも `StringIndexError` に分類した (#11615)。
range は Julia の inclusive code-unit endpoint を先に検証してから Rust の
exclusive slice end に変換し、multibyte endpoint の誤受理/誤拒否を解消した
(#11618)。true numeric OOB は `BoundsError` のまま残し、その `.a` receiver は
#11616 で追跡する。carrier は class 判定前に無条件消費し、key mismatch と
internal error でも stale payload を残さない。

strict range validation で旧挙動へ依存していた Base の `split` / `rsplit` /
`chopprefix` / `chopsuffix`、predicate `strip`、multi-pattern `replace` と
Irrational display workaround も露出したため、inclusive endpoint と走査を
`firstindex` / `lastindex` / `nextind` / `prevind` で統一した (#11638)。
`lastindex` / `thisind` / `nextind` / `prevind` / `isvalid` は malformed UTF-8 の
standalone continuation byte を独立した character start として扱う (#11624/#11628)。
String の range/vector/colon index は `Str`/`StrBytes` と captured `Any` を同じ
runtime classifier に通し、non-integer vector/range は MethodError/ArgumentError、
巨大 range は materialize せず BoundsError へ到達する
(#11627/#11629/#11630/#11640/#11643/#11644)。

`Meta.parse` は incomplete 入力を `Expr(:incomplete, error)` として返し (#11633)、
diagnostic span を parse segment に限定する (#11634/#11637)。同一行の semicolon
group は `Expr(:toplevel, ...)`、newline は1つだけ消費し、範囲外 start は
BoundsError、UTF-8 interior start は host panic なしの ParseError となる
(#11636/#11639/#11641)。nested catch の payload は handler capture 時に固定し、
後続 catch が上書きしない (#11632)。

同名 `JuliaSyntax.ParseError` の追加で #10445 の top-level inner-constructor
owner gap が再現したため、concrete allocation target は lexical bare alias で
なく宣言 owner から解決するよう修正した。cached synthetic default の残差は
explicit `Base.ParseError` field constructor (W-75) として登録済み。再発防止の
string-index validation 統合は #11621。

parser、VM/compiler/integration、exception/string fixture、workaround/source/
fixture audits、fmt/clippy、独立 adversarial re-review に加え、release full suite は
5,838/5,838 green (slow 2、skip 4) を確認済み。

### direct dynamic call も shared CallRequest resolver へ統合 (Issue #10461 Phase 1b)

`CallDynamic` の匿名 `(fallback, arity, candidates)` payload を boxed
`DynamicCallOperands` に置換し、compiler が lexical/module 解決した
`callee_name` を bytecode に保持する。bare/qualified direct call の runtime
cache miss と callable-value call は、同じ `runtime_call_request` で positional
type、lexical scope、world、span、candidate set を構築し、同じ
`resolve_runtime_call_request` scorer を使用する。candidate 順から callee 名を
推測しない。audit は anonymous emission、carried identity の無視、private scorer
直呼びを mutation control 付きで拒否する。schema 変更のため Base cache version
を 172 へ更新した。complete `ResolvedCall` bindings/keywords、resolved-ID
intercept/specializer は #10461 の次 phase として継続する。runtime path 到達前に
拒否される qualified single-argument `Any` overload は #11622 で別途追跡する。

### `where` / keyword method mutation を held REPL VM 上で公開 (Issue #9784)

Main-owned method の新規定義、extension、same-signature replacement は、ordinary
形に加えて bounded/repeated TypeVar の `where`、複数 keyword/default、keyword
splat、positional vararg、`where`+typed keyword の組合せでも fresh full recompile
を使わない。source method と marker-specific transitive caller refresh を1つの
activation group として dormant install し、`DefineEvalFunction` 到達時に同じ
world increment で公開する。caller-before/caller-after の両順序と catchable error
後の reached prefix を固定し、対象定義は `last_vm_build_nanos() == Some(0)`。

source-order 中の keyword call が未来の replacement index を直接参照していた
経路は、world-aware function-value dispatch に統一した。これにより marker 前は
到達済み旧method、marker 後は primary+refresh の新methodを選ぶ。syntax whitelist
は撤去し、Main ownership を意味ゲート、function/specialization/activation alignment
を relocatable extraction の独立 structural gate とする。残る #9784 は
Base/preload-owned extension、abstract/primitive/enum/parametric/inner-constructor/
redefined type、module/import/macro/type-alias/baremodule、opaque runtime `eval` と
最終 mirror 削除。

### runtime callable resolver を production 経路へ統合 (Issue #10461 Phase 1a)

stored function、callable struct、DataType constructor、HOF callback、qualified
runtime call、splat の method selection を
`dispatch_function_variable_for_values` に集約した。shared resolver は concrete
`Value` による scorer を最初に実行し、no-match と ambiguity を区別して、match が
ない場合だけ legacy string scorer へ fallback する。parametric constructor は
full callable `Type{...}` signature の統合が未完なため、同じ boundary 内の明示的な
legacy bridge として残す (#11610)。`invoke` は declared-signature adapter に分離し、
function variable/keyword form でも declared `Any` を runtime type へ refine しない
よう修正した (#11609)。3つの `CallFunctionVariable*` opcode から local scorer を
撤去し、audit は3 arm の実在、comment ではない shared call、value-before-legacy
order を2つの mutation control で固定する。compile-time direct call、complete
TypeVar/keyword binding、resolved-ID intercept/specializer は残るため #10461 は
継続する。

### AoT gate の binary lookup を Cargo target と統一 (Issue #11598)

`scripts/test_aot.sh` と AoT/metamorphic/fixture parity helper は、明示された
`SJULIA_BIN` / `JULIARS_BIN` を優先し、未指定時は
`CARGO_TARGET_DIR/release/{sjulia,juliars}` を使用する。相対 target は repository
root 基準で解決し、未指定時の `target/release` は維持する。これにより shared
target を使う worktree でも Cargo の出力先と後続 consumer が一致し、full gate
完走後の固定パス failure を防ぐ。登録済み source-only audit は default / external
absolute / relative / explicit override を実行確認し、`$ROOT/target/release` への
差し戻し mutation を拒否する。adversarial review 後、全 `aot_*.sh` と
fixture-parity wrapper を自動 discovery する11-consumer inventory へ拡張し、
正規 assignment を残したまま `${ROOT}` 固定パスへ後続再代入する mutation も
拒否する。

### call resolver の structured comparison boundary を追加 (Issue #10461 Phase 0)

shared inference layer に `CallRequest -> ResolvedCall` contract を追加し、callee
owner、structured positional/keyword type、lexical module/method、world、source
span、candidate set、selected target、TypeVar binding observation を1つの診断境界
で表現する。`SJULIA_CALL_RESOLVER_COMPARE=1` は stored function/callable/HOF が
共有する `dispatch_function_variable_for_values` で、production callable scorer
と VM runtime selector を同一 request 上で比較し、差分だけを stderr に出す。
production の legacy result は先に計算してそのまま返すため default/off と
compare/on の意味論は同じ。qualified Base function、stored value、HOF、runtime
specialization、parametric constructor の corpus では差分ゼロ。entry-point
inventory と fast-path review rule は `CALL_RESOLUTION.md` に集約した。残作業は
complete TypeVar/keyword binding の共有、intercept の resolved target ID 化、
residual scorer の撤去/ execution-only 分類であり、本 Phase 0 は #10461 を close
しない。

### compile-expression shadow audit の InternedStr guard を認識 (Issue #11602)

`check_compile_expr_local_shadow_guard.sh` の guard regex が旧 `String` 形の
`contains_key(name)` だけを認識し、現在の `Expr::Var` が使う canonical
`InternedStr` projection `contains_key(name.as_str())` / `contains(name.as_str())`
を認識しなかったため、clean main の guarded 20 site を false positive として
reject していた。optional `.as_str()` projection を guard grammar に追加し、
brace-region/annotation/zero-match fail-loud と既存 injected unguarded negative
self-test は維持した。

### structured type semantics の display-string 依存を撤去 (Issue #10460)

inference の tuple `typejoin`、runtime `_typeintersect`、type-object reflection、
method cache serialization が `CoreType` の同一 structural graph を使用する。
qualified nominal owner、ordered `UnionAll`、dependent bounds、bound/free
`TypeVar` identity を変換後も保持し、diagonal intersection は名前一致で別 scope
の変数を capture しない。unrelated user family の context-free nominal
`typejoin` は upstream 同様 `Any` に widening。nested/same-name binder、dependent
bound、partial application、value parameter、alpha-equivalent wrapper を generated
upstream parity corpus で固定した。type-representation audit は semantic site の
normalized exact inventory を SHA-256 で固定し、同数の site 差し替えも negative
self-test で検出する。

### splat forward された vararg のランタイム型適用 curly を解決 (Issue #11539)

parametric outer constructor 内で `T{A,B,C,expr}(xs...)`(同じ vararg を forward
splat し、`expr` がランタイム値/inline call)が `UndefVarError: T{A,B,C,expr} not
defined` になっていた。root cause は2箇所: (1) compiler の
`compile_call`(`subset_julia_vm_compile/src/compile/expr/call/mod.rs`)は
`has_splat` のとき `try_compile_parametric_constructor_call` へ到達する前に
`compile_splat_call` へ分岐しており、curly のテキスト全体
(`Foo{M, N, T, n}`)を未定義変数名として扱っていた。(2)
VM の `CallFunctionVariableWithSplat` ハンドラ
(`subset_julia_vm_vm/src/vm/exec/call_function_variable.rs`)は callee 値の
match が `Function`/`Closure` のみで、非 splat の兄弟命令 `CallFunctionVariable`
がすでに持っていた `DataType`/`Struct`/`StructRef` callee arm が欠けていた
(vararg バインディング自体は既に実装済みで到達不能だっただけ)。
新設の `try_compile_splat_parametric_constructor_call` が
`emit_parametric_type_arg_value` + `ApplyTypeDynamic` でランタイム DataType を
構築してから splat-aware runtime call へ渡し、VM 側は
`collect_runtime_callable_candidates`(既存の DataType/Struct/StructRef 対応
candidate collector)を再利用して非 splat 経路と揃えた。この新チェックは
`compile_call` 内で `owned_constructor_name_in_scope` より前に置く(curly でな
い名前には `Ok(None)` を返すので `Owner.Foo(xs...)` の非 curly forward は従来
どおり)— `owned_constructor_name_in_scope` の先の
`compile_runtime_datatype_value_call` は型引数を
`resolve_instantiation_with_type_expr` で即時解決しようとし、`where`-bound
なランタイム型変数を拒否するため、module 修飾された struct
(`M.Foo{M,N,T,n}(xs...)`)も同じバグを踏んでいた。fixture
`dispatch_splat_vararg_forward_type_apply_11539` で local 変数/inline call/
module 修飾 struct の3パターンについて `typeof` と `x.data` の中身(ネストし
ない flat tuple)を upstream と一致させて確認。

### apply-type TypeError の payload 復元 (Issue #11399)

`x{T}`(x が型でない値)が raise する TypeError が、upstream の
`.func = Symbol("Type{...} expression")` / `.expected = UnionAll` /
`.got = 値そのもの` を保持するようになった。従来は funnel が
`:unknown`/`nothing` の placeholder を作っていた。DomainError と同じ
message-keyed side-channel(`pending_type_error_payload`)+
`type_error_with_payload` helper。typeassert 等は従来どおり(pure-Julia struct
throw で real fields)。fixture `exceptions_typeerror_applytype_11399`。
これで #11399 の VM-raise error payload(Method/Bounds/Domain/TypeError-applytype)が
出揃った。tech-debt #11399。
### 条件分岐内定義への splat/kwargs 呼び出しが runtime visibility を尊重 (Issue #11320)

未到達の top-level `if` 分岐内で定義したメソッド(`if cond; f(x)=x; end`、`cond ==
false`)は direct call 同様 runtime 上で未定義のままでなければならない。原因は二つ:
(1) `compile_main` の eager top-level-definition activation drain が、statement
scan で見つかった関数(未到達の `if`/loop 分岐内のものも含む)を source position
だけで無条件到達とみなし `DefineEvalFunction` を activate していた、(2)
positional-splat の dynamic call path が `PushFunction` で可視性チェックなしに
callable token を作り、callee の存在確認より先に splat 引数を評価していた。
`if`/`while`/`for`/`try` body 内に nested した関数定義を drain の対象から除外
(`compile_stmt` 自身の branch-gated `Stmt::FunctionDef` 処理が正しく activate 済み)
し、`Instr::RaiseUndefVarErrorIfFunctionInvisible` を kwargs なしの splat/positional
call の引数評価前に emit(upstream の callee-before-arguments 順序と一致、
`Meta.lower` で検証済み; kwargs 付き呼び出しは upstream 側でも引数評価が先のため対象外)。
`CallFunctionVariable` 系 4 dispatch path が `Vm::function_name_exists_but_invisible`
という単一の visibility 判定を共有するようになった。同系統の残課題(`CallSpecialize`
の未チェック、stored function value の `candidate_indices` bypass、struct 側の同型
drain バグ)は Issue #11581 として起票(siblings #11286/#10461)。

### getfield の BoundsError が実際の receiver と 1-based index を報告 (Issue #11509)

`getfield`/`_getfield` を Rust-backed 複合値(`Expr`、`Base.Generator`、
`RegexMatch` 等)に範囲外インデックスで呼ぶと `BoundsError(nothing, <off-by-one
な index>)` になっていた(Issue #11382 の副産物として発見)。共有の
`VmError::FieldIndexOutOfBounds -> BoundsError` 変換が receiver `Value` を
一切持たず、internal 0-based `field_idx` からメッセージを再構成していたのが
原因。`pending_domain_error_val`(Issue #11399)と同じ side-channel パターンで、
`Vm::field_index_out_of_bounds_with_receiver` helper が raise の瞬間に
`(index, field_count)` でキー付けして receiver を park、funnel が
unconditionally 消費して `.a`/`.i` を構築するよう修正。raise site は
`field_idx` ではなく呼び出し元の元の index を報告するようになった。
`Base.Generator` 専用の `generator_projected_field_by_index` も、receiver 無しで
inline に raise する代わりに `RegexMatchValue::field_by_index`(Issue #11382)と
同じ形で `Option<Value>` を返すよう変更し、範囲外インデックスが共有の None 分岐
を通るようにした。Fixture `exceptions_getfield_boundserror_payload_11509`
(`Expr`/`Base.Generator`、upstream julia 1.12.6 で検証済み)。

同修正へのマージ前 adversarial re-review で cross-contamination 回帰を検出
(Issue #9787 と同種の non-transactional pending side-channel バグ)。初期案は
field lookup の**前**に無条件で receiver を park していたため、成功した
getfield の後にも stale な receiver が残り、この side-channel を一切 park し
ない別の raise site(範囲外 index の `setfield!` 等)が起こす無関係な
`FieldIndexOutOfBounds` に誤って付着していた。raise の瞬間にのみ park する
(`field_index_out_of_bounds_with_receiver`)よう修正し、`(index,
field_count)` でキー付け。Fixture 3つ目の `@testset` がこの contamination
を再現(別オブジェクトへの成功した getfield → 別オブジェクトへの範囲外
setfield!)し、修正前コードでは失敗する。作成中に、`setfield!` 自身の
BoundsError が receiver を一切持たないという別の(本 issue の getfield 限定
scope 外の)既存 gap も発見し、Issue #11596 として起票。

### REPL の concrete type を source order で公開し、error 前の prefix を保持 (Issues #9784 / #11546)

live append する新規 concrete struct は compile 時に private な type registry tail
として予約し、top-level bytecode の `DefineEvalStruct` が source order で到達した時だけ
Julia binding と runtime registry を公開する。`PushDataType` / `NewStruct` /
`NewStructSplat` と typed fused-return path は未到達 type を catchable な
`UndefVarError` として拒否するため、function body や forward reference から suffix を
先取りできない。後続の runtime error では VM、compiler snapshot、session mirror の
すべてが同じ interleaved function/type activation trace から到達済み prefix だけを
commit し、未到達予約は破棄する。runtime `eval(:(struct ... end))` は marker 到達と同時に
activate して従来どおり即時 construct 可能。Base cache version は 165。
abstract / primitive / enum、parametric / inner-constructor struct、module form と残る
fallback retirement は Issue #9784 の後続スライスとして継続する。
### DomainError.val の payload 復元 (Issue #11399)

VM 内部で raise される DomainError(`sqrt(-1.0)` 等の負数域)が、upstream の
`.val`(実際の域外値)を保持するようになった。従来は funnel が `val=nothing`
の placeholder を作っていた。MethodError/BoundsError と同じ message-keyed
side-channel(`pending_domain_error_val`)+ `domain_error_with_val` helper を
funnel が一度だけ消費。sqrt の f64/BigFloat サイト(builtins_math.rs /
arithmetic.rs)を変換。ユーザーの `throw(DomainError(val, msg))` は struct 直接
throw で従来どおり val を保持。fixture `exceptions_domainerror_val_11399`。
tech-debt #11399 の継続(#11374 の payload 復元と同系)。
### cache 復元で specialization-disable 判定を忠実に再生 (Issue #10334)

fresh compile が最終 method table から決定した array `getindex`、array
`setindex!`、field access の3つの specialization-disable flag を、一度だけ
`CompiledProgram::specialization_disable_flags` に記録して永続化する。
in-memory clone、whole-program serde (`.sjvmbc`/manual restore)、sectioned Base
cache は同じ snapshot を運び、restore は top-level IR を再走査せず transient
compile context へそのままコピーする。module 内 override と alias-typed `Vector`
receiver を含む regression corpus で fresh/manual/`.sjvmbc` の3 flag が完全一致し、
#10334 は解決した。seeded `PROGRAM_CACHE` の context hydration は #10335、promotion
registry と `main_scope_names` の復元は #10339 として独立に未解決である。

### Base/stdlib 型の const alias をパラメータ注釈で dispatch (Issue #11113)

sibling of #11104: `const MyPair = Pair` のように alias 先が Base/stdlib 宣言の
型の場合、lowering の alias gate (`is_likely_type_name`) には見えない (Base は
隔離された lowering pass、あるいは Base cache 使用時は lowering されない)。
`shared_ctx.struct_table` / compiler-visible builtin-type registry の両方を
使い、lowering 後も alias 未登録なトップレベル/module-local な const/global
binding を compile 時に解決する compile-time top-up を追加。Pair 系は
`struct_table`、Regex/UnitRange 系は builtin-type registry 経由。

### #10592 の単一型パラメータ default-ctor-after-miss 再現せず、多パラメータの兄弟ギャップを起票 (Issue #10592, #11549)

現行 `main` で再検証: PR #11476 (Issue #11404 の明示 where-parametric outer
優先修正) が Issue #10592 の default-ctor-after-miss クラスも解消していること
を確認 — issue 本文の3形態(直接呼び出し・bound callable `f = B{Int64}; f(7)`・
自己参照 outer body)すべて正しく construct するようになった。
`parametric_ctor_callable_parity_10502.jl` の従来 deferred だった negative
guard (`CtorAuditBox10502{Int64}(7)`, `CtorAuditSib10502{Int64}(7)`、direct
+ bound-callable) をアサート化。多型パラメータ構造体(フィールド2つ以上)で
concrete outer constructor のアリティが default field constructor と異なる
呼び出しは、別クラスの未解決クラッシュ (uncatchable compile error または
runtime `InternalError`) として Issue #11549 に起票 — 本 fixture 強化の対象外。

### `Vector{expr}` の不正パラメータが TypeError を raise するように (Issue #11555)

sjulia には parametric-type construction の経路が2つあった: 動的 base
(`T{x}`, `Core.apply_type`) を扱う `apply_type_to_runtime_base` は
`type_arg_value_to_julia_type` で無効な値 (ErrorException インスタンス、
`String` など) を検証し正しく `TypeError` を raise するが、literal/
compile-time-known base (`Vector{e}` など、`Instr::ConstructParametricType`)
は `build_parametric_type` を検証なしで直接呼んでおり、認識できないパラメータ値が
黙って `Any` プレースホルダに劣化していた (`Vector{Any}`)。`build_parametric_type`
の先頭に upstream `jl_valid_type_param` 相当の検証
(`is_valid_type_param_value`: Type/TypeVar/Symbol/Module/isbits 値/
全要素が isbits な Tuple/`is_isbits` な struct instance のみ有効) を追加し、
`ConstructParametricType`・`ConstructParametricTypeSplat`・
`apply_type_to_runtime_base` の3呼び出し元すべてで共有。`Complex`/`Rational`/
plain isbits user struct は upstream 同様 valid のまま (レンダリングは既存の
`Any` fallback のまま、struct-value type-param レンダラ自体は別スコープ)、
`nothing`/`missing`/`Module` も valid (upstream 通り)。`BigInt`/`BigFloat`
(upstream で non-isbits) は `expected Int64` の TypeError。無捕捉の bare
`Function`/`@enum` 値は常に isbits、`Closure`/`ComposedFunction` は捕捉変数/
内包する関数すべてが isbits な場合のみ valid — 共有の `isbitstype` builtin
機構はこれらの種別を一切分類できない (別の pre-existing gap、Issue #11589 で
起票) ため、直接判定して false-positive な回帰 (`Vector{sin}` が新規に
TypeError になってしまう) を回避。fixture
`types_construct_parametric_type_invalid_param_typeerror_11555`。

## 最新対応 (2026-07-17)

### AoT reduced numeric matrix: div-family を非 Int64 幅へ拡張 (Issue #9687 slice 3)

`scripts/aot_numeric_matrix_reduced.sh` の対応スロットを 85→105 行に拡張。
Issue #10131 で AoT `Value` に I8/I16/I128/U8..U128 variant が追加され、
div-family (div/fld/cld/rem/mod) の native 発行が boxed slot のときのみ
`Value::from(...)` で包まれるようになったため、`Int8`/`Int16`/`Int32`/`Int128`
同型 div-family (20 行) が Int64 と同じスロットで実行可能になった
(`string(x) == repr(x)` が成立する signed-integer tower に限定するのは slice 2
と同じ制約)。`docs/vm/NUMERIC_MATRIX_AOT_REDUCED_SKIPLIST.tsv` の
catch-all カウントを 5117→5097 に縮小。UInt8/16/32/64/128 div-family は
Issue #10131 でコンパイル自体は通るようになったが、oracle の `repr()`
(16進) と probe の `string()` が今も乖離するため、この comparator では
引き続きスキップリスト対象(#10131 が解決したのは div-family の
codegen ギャップのみで、repr()/string() 乖離は別問題)。isless / 混合型
min-max も #10131 で AoT codegen としては動くようになったが、この
comparator の `supported()`/`key_for()` に未配線のため今回のスコープ外
(将来の slice 候補として skiplist の reason に記載)。

### catch binder が composite signature 注釈内のエイリアスを shadow (Issue #11321)

`catch T` は `T` の新しいレキシカル束縛を導入するが、lowering の型エイリアス
pre-scan(#5055、source-order 非依存)は `try`/`catch` 本体へ再帰しないため、
composite 注釈(`Vector{T}`)内の `T` が外側の `const T = Int64` に凍結され、
メソッド定義が(非 Type の runtime 束縛値に対して)黙って成功していた —
upstream は `TypeError` を送出する。同一節内で `T` を解決可能な型へ再代入して
から定義した場合(`catch T; T = Int64; f(x::T) = 99`)も、値が alias table に
一度も登録されず nominal placeholder `Struct("T")` のまま dispatch に失敗して
いた(有効ケースが `MethodError` になっていた)。

`control_try.rs` の実 lowering で、既存の alias-table 登録/可視性プリミティブ
(`register_prescanned_non_alias` / `register_prescanned` / `AliasScope`
snapshot+restore)のみを使い、節の開始位置に non-alias tombstone を、節内の
同一節内再代入をその代入自身の位置に real alias entry として登録 —
新しい解決ロジックはゼロ、節の `end` で必ず restore して漏れを防ぐ。
composite 注釈内で shadow された名前は `Struct(name)` のまま残るため、
`emit_signature_definition_probes` に新しい pass を追加: composite
注釈を再帰して bare identifier leaf を収集し、それが CURRENT runtime local
(`self.locals` かつ `self.initialized_locals`)なら、upstream が任意の
parametric 型引数に課す実際のルール(Type/TypeVar、`Symbol`、`isbits` 値は
すべて合法 — `Vector{7}` は upstream の実 `DataType` であり `TypeError` では
ない)で検証する。動的 base 適用(`T{x}`)が既に使っている
`ApplyTypeDynamic` の `type_arg_value_to_julia_type` 分類へ固定 `Vector` head
で ルーティングし、結果は破棄する。この probe の前バージョンは素の
`name <: Any` を emit しており、値が literal に Type であることを要求する
ため、`x = 7; q2(v::Vector{x}) = 1` のような upstream で合法な isbits 型
パラメータを誤って `TypeError` にしていた(マージ前レビューで発見した
upstream/main 双方に対する回帰)。`initialized_locals` ゲートが #11114/#11118
の forward-reference probe 領域との干渉を防ぐ。fixture
`exceptions_catch_binder_signature_shadow_11321`
(元の 2 MWE + no-leak control + isbits 型パラメータの非回帰ケース)。

### 捕捉例外の typed payload (Issue #11374)

catch した例外が upstream のフィールドを保持するようになった。BoundsError は
実コンテナと完全 index タプル(`A[10]` → `.i == (10,)`、`M[9,9]` → `(9, 9)`、
show も `[9, 9]` 形式)。MethodError は compile-time 検出の dispatch miss
(新 builtin `ThrowMethodErrorWithArgs`(wire 317)が Pop+ThrowMethodError を
置換、CACHE_VERSION 161)と named numeric fast path(sqrt 2 サイト)+ runtime
dispatch 4 サイトで実 callable と引数値を funnel に運ぶ
(`pending_method_error_payload` side-channel、message 完全一致ガード付き)。
残余レーン(const 伝播 alias 呼び出し等)は #11399 で追跡。fixture
`exceptions_payload_fields_11374`。
### finally 内 rethrow() をネスト catch が飲み込む問題 (Issue #11306)

`finally` ブロック自身のネストした `try/catch` が明示的な `rethrow()` を
捕捉すると、finally に入るきっかけとなった元の例外が外側の `catch` へ届か
なくなっていた。根本原因: unwind 中の finally に "自分自身の末尾で再送出
すべき" という状態がスカラー1個 (`rethrow_on_finally`) で表現されており、
finally 内で発生する無関係な例外処理(ネスト catch の `ClearError` を含む)
が無条件にこれを上書き/クリアしていた。深さを持つスタック
(`Vm::pending_finally_rethrows`) に置き換え、`Handler::finally_pending_len`
で各ハンドラの push 時点の深さを記録して `handle_error` がハンドラ pop 時
にそこまで truncate するようにした -- enclosing finally のマーカーは、内側
の catch が「自分の」例外を処理しても生き残る。fixture
`exceptions/finally_rethrow_swallow_11306.jl`: MWE、ネスト catch が再度
`rethrow()` するケース、二重ネスト finally、finally 内で無関係な例外を完
全に処理するケースをカバー。
### exact-or-Any constructor identity の監査 (Issue #11436)

#11434 の根本原因(hash-backed `StructRegistry` の same-base first-match が
`Any` constructor return を seed 依存で誤 sharpening)をクラス単位で封じた。
不変条件「constructor return identity は exact(owner + 完全型パラメータ)か
`Any` のまま。same-base スキャンは列挙のみで identity を確立しない」を
docs/vm/CODE_AUDITS.md に文書化し、新監査
`check_struct_registry_first_match.sh` が hash-backed struct registry への
`.iter()` + find/find_map/position/next チェーンを発見し、レビュー済み分類
(unique-guarded / exact-key-equivalent / enumeration)付きインベントリと照合。
未分類の新規サイトは fail。negative self-test 登録・CI/source_only_audits.tsv
登録済み。最後の順序依存サイト
(`try_struct_field_count_default_ctor_fallback` の 2 キー `iter().find`)は
ordered exact-key probes(`struct_table.get`)へ書き換え。tech-debt #11447 の一部。
### cache 復元後の inference-global 型 parity (Issue #10333)

fresh compile が const binding を具体型のまま、mutable global を `Any` へ widen
した最終 `inference_global_types` を、名前順の
`CompiledProgram::inference_global_types_snapshot` として永続化する。
`.sjvmbc`/manual serde と Base cache の section serializer の双方が同じ snapshot
を運び、restore は空 map ではなくこれを transient compile context へ再構成する。
`Base.infer_return_type` / `Base.return_types` の const/mutable global MWE は
fresh と `.sjvmbc` で一致し、#10462 scoreboard の #10333 allowance は削除された。
Base cache version は 162、`.sjvmbc` version は 5。

### REPL runtime error 前の function 定義 prefix 保持 (Issues #9784 / #11477)

live append した function body は `DefineEvalFunction` が source order で到達するまで
dormant とし、runtime error 時は VM の method world、compiler snapshot、fallback replay
の3層で到達済み prefix だけを commit する。未到達 suffix は reflection、direct/dynamic
call、forward reference、IR inline のいずれからも見えず、同一 generic の複数 method
でも到達分だけが残る。これにより `f() = 1; error(...); g() = 2` 後は `f` のみ利用でき、
次 eval は同じ live VM を継続する。type 定義 delta と runtime `@eval` による間接的な
definition-world 変更は保守的 drop のままで、Issue #9784 の次スライスとして継続する。
### comprehension の runtime element 型で dispatch (Issue #10315)

body 型を静的に解決できない1次元 comprehension は、runtime type-join により
`Vector{Any}` placeholder から実際の `Vector{Int64}` などへ狭まる。この結果型を
`ArrayOf(Any, None)` として保存していたため、代入後の値が確定済み
`Vector{Any}` と誤認され、`Vector{Any}` / `Vector{Int64}` overload set で前者へ
静的 bind していた。indexed path と iteration-protocol path は
rank-known/element-unresolved (`ArrayOf(_, Some(1))`) を保持し、tuple-destructuring
path は内部の empty-`Union{}` sentinel を同じ unknown-element 表現へ射影する。
dispatch matcher 自体は変更せず、既存の runtime deferral policy が concrete runtime
vector を選ぶ。range / Set / tuple と heterogeneous / explicit-`Any` controls を
fixture で、`CallDynamic` bytecode 形状を統合 regression test で固定した
(prevention Issue #11513)。
### slot-backing dominance verifier (Issue #10820, prevention for #10819/#7556)

`subset_julia_vm_compile/src/compile/slot_backing_verifier.rs` を追加: 単一関数の
コンパイル済み命令列を `cfg.rs` の CFG 上で定義済み到達解析 (forward "must"
dataflow, meet=intersection) にかけ、`LoadSlot`/`LoadAny` を含む local
Load/Store 命令族の各読み出しが、全ての先行パスで同じ local への Store に
支配 (dominate) されているかを検証する。test-only pass(`#[cfg(test)]`) として
実装し、本番コンパイル/VM 経路には一切コストを追加しない。負のセルフテスト
(#10819 修正前の shape を再現し、非代入分岐で違反を検出) を含む8ユニット
テスト全て green。`cfg.rs` を拡張し `PushHandler(catch_ip, finally_ip)` を
実 CFG エッジとしてモデル化 (try 本体内の Store は catch/finally に対して
支配しない、try 開始前の Store は支配する、を正しく区別) — try/catch と
ゼロ回反復ループの widening を #10819 のマトリクスに追加
(`nothing_initialized_trycatch_loop_widen_10820.jl`, upstream 8/8 一致)。

### constructor reflection の構造的解決 (Issue #11402)

DataType callee の `Base.infer_return_type` / `Base.return_types` が
構造的に解決されるようになった: applied 綴り (`S{Int64}`) は自分自身を、
bare family (`S`) は引数型から型パラメータを推論した instantiation を返し
(runtime 動的 constructor と同じ unifier
`infer_parametric_type_args` を再利用)、arity 不一致は `Union{}`。
従来は関数名 reflection 経由で `Any` / `Union{}` に広がっていた。
explicit inner constructor の applied 形も解決。fixture
`reflection_constructor_return_types_11402`(12 assertions)。
tech-debt #11447 の一部。
### bundled StaticArrays が upstream 4 パラメータ SMatrix をモデル化 (Issue #11432)

`subset_julia_vm/packages/StaticArrays/src/SMatrix.jl` の `SMatrix{M,N,T}`
struct に upstream 由来の第4パラメータ `L`(`L == M*N`)を追加し、
`struct SMatrix{M,N,T,L} <: StaticMatrix{M,N,T}` とした(real StaticArraysCore の
`SMatrix{S1,S2,T,L} = SArray{Tuple{S1,S2},T,2,L}` alias 形に一致)。`W::SMatrix{2,
2,Float64,4}` のような field 注釈が `too many parameters for type
StaticArrays.SMatrix` にならなくなった(#11358 で synthetic default constructor
検証が Julia 互換になったことで露見した回帰)。`SMatrix{M,N,T}` など不完全パラメータ
化形は引き続き構築可能 — 既存の partial `UnionAll` 適用/dispatch が末尾パラメータ追加
にそのまま一般化する。`SMatrix` を表示名文字列でパースしていた2箇所の Rust 高速パス
(`try_make_static_array` / `subset_julia_vm_vm/src/vm/exec/struct_ops.rs`、
`StaticArrayInlineData`/`StaticRealValue` の型名テーブルと `elem_type_str` /
`subset_julia_vm_bytecode/src/value/static_real.rs`)を新しい4パラメータ形に対応させた。
IFS fractals サンプル3箇所すべてで `W::SMatrix{2,2,Float64,4}` を復元し、W-73
workaround を解消。fixture: `static_arrays_smatrix_four_param_11432`。
### abs2 の実数 fallback を `::Real` に型付け (Issue #10602)

`abs2("a")` が `MethodError` ではなく `"aa"` を静かに返していた —
`base/number.jl` の実数 fallback が無型の `function abs2(x)` で定義されて
おり、`String` を含む任意の引数にマッチし `x * x` が文字列連結されていた。
upstream `julia/base/number.jl:189` に合わせ `abs2(x::Real) = x*x` に型付け
し、`abs2("a")` は upstream 同様 catchable な `MethodError` を送出するよう
になった。`complex.jl` の型付き `Complex{T}`/`Complex{Float32}`/
`Complex{Float64}` メソッド(Issue #10775)は引き続き自身の具象メソッドに
dispatch する — 新 fixture は実数と Complex の両方の `abs2` を同一実行で
検証し、3 回連続の fresh-process 実行で green。fixture:
`numeric/abs2_string_methoderror_10602.jl`。

### 明示 where-parametric outer constructor の優先 (Issue #11404)

`ExplicitOuterGap{T}(x::T) where {T} = nothing` のような source 記述の明示
parametric outer が、automatic field constructor と同形の完全適用呼び出し
(`ExplicitOuterGap{Int}(1)`)でも dispatch に参加するようになった —
fully-applied fast path が `Base{T}` 形テーブルの matching-arity メソッド存在時に
static resolver を経由する。upstream 同様 user メソッドが synthetic default inner
を置換する。非一致シグネチャ・別 arity・委譲 outer は default field constructor
に到達可能なまま。fixture `struct_explicit_outer_precedence_11404`。
tech-debt #11447 の一部。
### nameof(::Module) 対応 (Issue #11171)

`nameof(m::Module)` が `MethodError` になっていたギャップを解消。
`nameof(::Type)`/`nameof(::Function)` と同じ内部 intrinsic パターンで
`nameof(m::Module) = _module_name(m)` を追加し、`_ModuleName` という
新しい `BuiltinId` がモジュール値自身の(修飾されていない)束縛名を
Symbol で返す。ネストしたモジュールの `ModuleValue.name` は内部的に
`"Owner.Name"` のように修飾されているため、`names(m::Module)` が既に
使っている「最後のパス要素を取り出す」ロジックを再利用した。ネスト
モジュール・`Main`・`Base`、および bare な動的ディスパッチ呼び出し
(`g(m) = nameof(m)`)と `::Module` 型注釈引数の両方で確認済み。
fixture: `modules/nameof_module_11171.jl`。

### concrete Complex element 型の cross-match 解消確認 (Issue #10775)

canonical MethodTable/CoreType dispatch は `Complex{Int64}` actual に対して
concrete `Complex{Float32}` / `Complex{Float64}` parameter を候補に入れない。
`Complex{T} where T<:Real` と concrete 2 method を全6通りの登録順で構築する
regression test を追加し、Int64 は常に generic row、Float32/Float64 は各 exact row を
選ぶことを固定した。元の `abs2` MWE と独立な3-method MWE は upstream と一致し、
各100 fresh sjulia process で出力が一意。後続の shared-dispatch 改修で現象自体は既に
解消していたため resolver production code は変更せず、#10784 で削除した binary
overload も復元していない。現行コード上の「#10775 は未修正」という古い comment
のみ削除した。全登録順 regression は prevention Issue #11492 も完了する。
### RegexMatch / Base.Generator の物理フィールド射影 (Issue #11382)

`RegexMatch`/`Base.Generator` の `fieldcount`/`fieldnames`/`getfield`/
`propertynames` が 0 フィールドを返していた問題を修正。`RegexMatchValue` に
`regex: RegexValue`(マッチ元の `Regex`、upstream の5番目のフィールド)を追加し、
`field_by_name`/`field_by_index` を1箇所に集約(`BindingValue::field_by_name`
と同じ形)して、dot アクセス(`exec/struct_ops.rs`)・`getfield`/`_getfield`
(`builtins_reflection/mod.rs`)・`jl_get_nth_field_checked` 相当の iterate 射影
(`value_field_projection.rs`)の3箇所が食い違わないようにした。
`CoreType::builtin_field_metadata` に `RegexMatch`(match, captures, offset,
offsets, regex)と `Generator`(f, iter)を追加(モジュール修飾子・型引数を
落とした bare 名でマッチするため、完全パラメトリックな
`Base.Generator{UnitRange{Int64}, typeof(f)}` 表記でも解決する)。issue が挙げた
他の Rust-backed 複合値(`BigInt`/`BigFloat`/RNG/`DataType`/`Core.TypeName`/
`IO`/`Core.Binding`/`Regex` 自身)は 0 フィールドを騙る代わりに明示的な
`VmError::NotImplemented`(Issue #11382 タグ付き)で fail-closed のまま。
Fixture: `reflection/regexmatch_generator_field_projection_11382.jl`。
副産物として Issue #11509(`getfield` 範囲外インデックスの `BoundsError` が
`nothing`/オフバイワンを報告する既存バグ、`Expr` でも再現)と Issue #11514
(filtered/tuple-splat generator に対する `getfield(::Base.Generator, 1)`
が既存バグとして throw する; fieldcount が2を報告するようになったことで
到達しやすくなった)を起票。

### 抽象数値フィールドの値保存 (Issue #11407)

`struct S; x::Number end; S(1)` が `1.0` を露出する問題を修正。フィールドの
静的 storage タグを `Number`/`Real`/`AbstractFloat`→F64・
`Integer`/`Signed`/`Unsigned`→I64 に潰していた写像を、フィールド境界専用の
`field_declared_value_type(_scoped)` で `Any`(boxed)に広げた — upstream は
`1 isa Number` で変換なしに Int64 を保存する。直接読み・print・関数境界
ロード・mutable setfield すべてで元の実行時値が保存される。fixture
`struct_abstract_numeric_field_11407`(20 assertions)。tech-debt #11447 の一部。

### Char を受け取る非 Int64 系整数コンストラクタ (Issue #11406)

`UInt8('b')` / `Int8('b')` / `UInt16` / `UInt32` / `UInt64` / `Int16` /
`Int32` / `Int128` / `UInt128` に `Char` を渡すと、文字の Unicode コードポイント
経由で変換するようになった。upstream `julia/base/char.jl` の
`(::Type{T})(x::AbstractChar) where {T<:Union{Number,AbstractChar}} =
T(codepoint(x))` と同じ形。従来は `Int`/`Int64` だけが Rust 境界の特別分岐で
`Char` を受け付け、他の固定幅コンストラクタは総称 `convert` の `MethodError`
に落ちていた。`subset_julia_vm/src/julia/base/strings/basic.jl` の
`Int(c::Char)` の隣に `Int8/Int16/Int32/Int128/UInt8/UInt16/UInt32/UInt64/
UInt128(c::AbstractChar)` を pure Julia で追加し、既存の `T(codepoint(x))`
という Number→Number コンストラクタへ委譲するだけで、upstream のレンジ
チェック(コードポイントが幅に収まらない場合の `InexactError`、例:
`UInt8('あ')`)がそのまま得られる。`Int64(::Char)` は既存の Rust 境界のまま
意図的に変更していない。fixture: `strings/char_integer_ctors_11406.jl`。

### colon 構文の Base 所有 dispatch (Issue #11444)

`a:b` のレンジリテラルが、bare `UnitRange` テーブルを user outer constructor
が汚染している場合に乗っ取られなくなった。upstream は `Base.:(:)` 経由で
`UnitRange{T}(start, stop)` の parametric inner constructor に直接降ろすため、
unit-range colon は Base 所有 — `base_owned_dispatch_wins` が非 Base メソッドの
勝利を検出したとき、compile は bare 名ではなく推論済みの完全適用 parametric
綴り(`UnitRange{Int64}` 等)で構築する。`a:s:b` は意図的に変更なし: upstream の
`_colon` は bare `StepRange(start, step, stop)` を呼ぶため、import した user 拡張が
step-range リテラルに正当に介入する(#11434 の回帰テストが固定)。直接の bare
呼び出し `UnitRange(3, 4)` は従来どおり user 拡張に届く。fixture
`range_colon_base_owned_dispatch_11444`。tech-debt #11447 の一部。
### kwargs の insertion-ordered 蓄積 + NamedTuple merge dispatch (Issues #11381 / #11383)

ランタイム kwargs 蓄積を `HashMap<String, Value>` から insertion-ordered
`KwargsMap<V>`(entries vec + name→slot index、重複キーは既存 slot を
value のみ上書き)へ置換。`f(; z=1, a=2)` の `kwargs...` はハッシュ順ではなく
呼び出し順(z, a)を保持し、単一 splat source 内の重複キーも先勝ちの位置で
value のみ上書きされる(#11383)。加えて keyword-splat のソース評価が
`merge(::NamedTuple, source)` の実 multiple dispatch を経由するようになり
(`Vm::merge_kwarg_splat_source` → `find_best_method_index` +
`sync_splat_callable_step`、Rust 側に型名/struct_name の文字列分岐は無し)、
ユーザー定義 `Base.merge(a::NamedTuple, ::T)`拡張や Base の
`merge(a::NamedTuple, b::Zip{I1,I2})` 重複キー検証(`ErrorException`)が
実際に発火する(#11381)。新規 `subset_julia_vm/src/julia/base/namedtuple.jl`
に退化形(空側 merge)と Zip 検証メソッドを追加。完全な非空
NamedTuple×NamedTuple 実行時 merge(呼び出し箇所で field 名が静的に
分からないケース)は runtime-parametric `NamedTuple{names}(values)`
コンストラクタ未対応に阻まれ、Issue #11494 として分離・保留
(コンパイル時に両オペランドの field 名が静的既知なケースは既存の
`try_compile_named_tuple_merge` 定数畳み込みで従来通り動作し無関係)。
fixture: `kwargs_insertion_order_11383`, `kwargs_duplicate_overwrite_in_place_11383`,
`kwargs_splat_user_merge_dispatch_11381`, `kwargs_splat_zip_duplicate_key_merge_11381`。
### DispatchFirst Base 関数の builtin フォールバックを動的呼び出しでも維持 (Issues #10786/#10871)

`compile_generic_dispatch_call` の単一 `Any` 引数フォールバック分岐が、
`DispatchFirst` な Base 関数(`isbitstype` 等)にユーザー method table がある
場合に、候補がユーザー method のみの素の `CallDynamic` を発行していた。
`Base.isbitstype(::Type{Box}) = false` を定義すると `g(T) =
Base.isbitstype(T); g(Int64)` が `BuiltinOp::Isbitstype` へフォールバックせず
`MethodError` になっていた(確認済み bytecode:
`CallDynamic(usize::MAX, 1, [Method(user)])`, builtin フォールバック無し)。
`typeof(x)` 経由の呼び出しが既に使っている `BuiltinOp -> (BuiltinId,
ValueType)` 変換 (`type_object_dispatch_builtin_fallback`) を同じ分岐で再利用し
`CallTypedDispatchOrBuiltin` を発行するよう修正。`isbits`/`ismutable` は
builtin_op 未登録(pure-Julia catch-all, Issue #6738)のため影響なし。#10786
本来の `isbits(1) == false` 症状は無関係な Base コンパイル時最適化によって
現在再現しない(`base_layout_predicates_dispatch_first_3911.jl` は変更なしで
pass のまま)。新規 fixture:
`base_isbitstype_dynamic_callsite_dispatch_first_10871.jl`。
### Int64/Int32/UInt64/Int128/UInt128 の範囲外変換 (Issue #11214)

`Int64(::Float64)` 等の checked 整数コンストラクタが、範囲外の値を静かに
saturate/誤分類していた2つの境界バグを修正。(1) 旧チェックは「値をターゲット
整数型へ cast (Rust の saturating cast) → その結果を float に戻して比較」という
round-trip 方式だったため、`typemax(Int64)` 自体が `Float64` で厳密表現不可能な点
(最近傍表現可能値は `2.0^63`)を突いて `Float64(2.0^63)` が誤って通過し
`typemax(Int64)` へ saturate していた。修正後は元の float 値に対して
`isinteger(x) && min <= x < max` という upstream (`julia/base/float.jl`) と同形の
明示的範囲述語 (`float_in_range` + `signed_int_f64_bounds`/
`unsigned_int_f64_max_exclusive`, いずれも 2 の冪境界のため f64 で厳密表現) を
直接適用する。同じ round-trip trap は `convert_to_i8/i16/i32/i64/i128/u8/u16/
u32/u64/u128` の F32/F64 全アームに存在したため、共通ヘルパーへ一括移行。
(2) `Int64(::BigInt)` の範囲外アームが `TypeError` を投げていたのを
`InexactError` に修正 (upstream 一致)。fixture
`numeric_int_range_conversion_boundary_11214`。`subset_julia_vm_vm/src/vm/
type_ops/conversion.rs::convert_to_i64` ほか。
### Complex{Float64}/Float32 具象ディスパッチの非決定性は再現せず (Issue #10775)

`abs2(Complex(2,3))` (および `Complex(2,3)/Complex(2,3)`) を現行 `main` 上で
fresh process 100 回超走らせて確認したところ、常に `13::Int64`(upstream と
一致)で決定論的だった。issue が指摘した「具象 `abs2(z::Complex{Float64})`/
`Complex{Float32}` メソッドが `Complex{Int64}` 引数に対し HashMap 順序依存で
時々勝ってしまう」不具合(commit `6d4b174a0` 時点で ~1/3 のプロセスで発生)は
再現しない。`CallDynamic`/`CallTypedDispatch` の全 applicability/tie-break
経路(`type_matches`, `nominal_type_names_compatible`,
`check_subtype_core`/`struct_params_are_subtype_with_lookup`,
`same_invariant_container_family_concrete_miss_core`)を追跡した結果、
不変パラメータの不一致は全経路で決定論的に reject されており、
HashMap 順序への依存は残っていない。#8659 のハッシュ反復決定化ラチェット、
#5915 の `CoreSubtypeEngine` 集約、#11076 のオーナー厳格化のいずれかが
副次的にこのギャップを塞いだとみられる。

コード修正は不要。回帰用に
`complex/complex_abs2_concrete_dispatch_stability_10775.jl` を追加
(upstream julia 比較 + 100 プロセス比較済み)し、
`scripts/dispatch_seed_sweep.sh` の default カテゴリ集合に `complex` を追加
(従来は `--all` の nightly 実行でのみカバー)して issue の prevention 提案
(multi-process determinism sweep)をデフォルト実行にも組み込んだ。詳細は
DONE.md 参照。#10645(#10775 でブロックされていた具象 `+`/`*`)は別途評価。

### Regex comment 終端後の recursion token 検出 (Issue #10738)

#10181 の unsupported-recursion guard は PCRE の `(?#...)` lexical rule に
合わせ、backslash 直後でも最初の `)` で comment を終える。
`(?#\)(?R)` の末尾 `(?R)` が comment 内として飲み込まれず、既存の明示的な
`regex recursion is not supported` error になる。upstream Julia/PCRE2 はこの
recursion pattern 自体を受理するが、sjulia は `fancy-regex` の silent mis-match を
避けるため recursion を意図的に未対応として拒否する。bytecode unit test が detector
と `RegexValue::new` の両経路を固定。

### 競合する import の warn-and-ignore (Issue #11426)

module 本体の代入が source order で先行する名前への selective/rename import は、
upstream 同様 stderr 警告付きで無視されるようになった。collect が module 本体を
順序走査して各値束縛の「直前の import マーカーの definition_order」を記録し
(`module_value_binding_positions`)、(1) 型 alias の eager 登録
(`register_scope_imported_type_aliases`)、(2) bare Var 読み取りの静的型解決、
(3) parametric 適用 `A{Int}(1)`(新 helper
`conflict_winning_module_value_binding` + `ApplyTypeDynamic` の module-global 変種)
の 3 点を先行値束縛が上書きする。`A = 42; import ..S: Box as A; A{Int}(1)` は
catchable TypeError。import が先行する場合の後続代入エラー(upstream の
cannot-assign-to-imported)は未実装のまま。fixture
`modules_import_conflict_ignored_11426`。
### eval の対象 module を呼び出し元 module に (Issue #11421)

bare `eval(expr)` が呼び出しの現れた module を対象とするようになった —
upstream の per-module `eval(x) = Core.eval(M, x)` に一致。compiler が
module 本体/module 関数内の `eval(expr)` に compile-time module path を
第1引数として付加し、`BuiltinId::Eval` の 2 引数形が
`eval_module_expr_value` に module 名を渡す。`Core.eval(m, expr)` /
`Base.eval(m, expr)`(従来それぞれ compile error / UndefVarError)も
明示 module 対象形として実装。`module P; module C; eval(:(x = 1)) end end`
は `P.C.x` を定義し `Main.x` を汚染しない。fixture
`modules_eval_current_module_11421` が @eval / module 関数内 eval /
既存 module global の読み取り込みで固定。eval で定義した関数の
module 帰属(静的修飾呼び出し `S.f` とグローバル漏出)は残ギャップ。
### LinRange の長さ型パラメータ L (Issues #11441 / #11449)

pure-Julia `LinRange` が upstream 署名 `struct LinRange{T,L<:Integer}`
(`len::L` / `lendiv::L`、`T<:Real` bound も撤廃) に一致。
`typeof(LinRange(0.0,1.0,5))` は `LinRange{Float64, Int64}`、
`typeof(range(big(1), big(2), length=3))` は `LinRange{BigFloat, Int64}` を返す。
部分適用 `LinRange{Float64}` の isa / `<: AbstractRange` はそのまま成立し、
IteratorSize/IteratorEltype/eltype の type-level dispatch は 2 パラメータ形へ更新。
fixture `range_linrange_length_type_param_11441` が BigFloat/Rational/Float64
形と trait pipeline を固定。tech-debt #11449 の残項目もこれで完了
(#11443/#11440 は既修正、StepRangeLen typeof は upstream 一致確認済み)。

### baremodule の builtin 型 binding authority (Issue #11419)

Core/Base 所有型の内部 registry を lexical binding の存在と混同しないよう、型注釈、
`isa`、`<:`、静的 parametric DataType literal を共通 authority query に統合。
`baremodule` は Core 型を常時可視、Base 型は `using Base` または named import 後のみ
可視とし、`import Base` 単独では不可視。hoist された module 関数の型注釈も元の
source order で builtin-only probe を実行し、失敗した定義を有効化しない。
### range(…; length) の TwicePrecision parity: Float32/Float16/narrow-int/step 形 (Issues #9509 / #11440)

`range(start, stop; length)` が Float32/Float16 端点で upstream の
`StepRangeLen{Float32, Float64, Float64, Int64}`(ref/step は Float64 スカラーに
collapse)を、narrow-int / 混在端点で promote 経由の Float64 TwicePrecision
StepRangeLen を返すようになった(twiceprecision.jl `range_start_stop_length` の
HpElement::F32 一般化 + `_linspace_range_f64` の element-tag 引数)。
`range(start; step, length)` の float 形も pure-Julia struct から upstream
`range_start_step_length`(floatrange rational 経路 + authoritative length)の
VM-native StepRangeLen に移行(新 intrinsic `_steprangelen_range_f64`、
RangeValue に `step_defined` 判別子)。`show(::StepRangeLen)` の zero-step
constructor 形 (`StepRangeLen(1.0, 0.0, 3)`) も pure-Julia 側に追加 (#11440)。
BigInt 端点は upstream 同様 LinRange のまま(第2型パラメータ欠落は #11441)。

### parametric 型の cross-module alias と const Union alias bound (Issues #11068 / #11003)

`const Y = OwnerA.X` (qualified FieldExpression RHS) が type alias として抽出される
ようになり (is_type_expression が module-qualified 型名を受理)、parametric base-name
正規化は dotted early-return の前に alias チェーンを追跡 — `OwnerB.Y{Int}()` が
所有 module の inner constructor に解決 (#11068)。#11003 (apply_type の const Union
alias bound) は main で解決済みと確認し、negative case 込みの契約固定 fixture を追加。


### String range の checkbounds (Issue #10958)

upstream strings/basic.jl:209-218 を移植: `checkbounds(s, r::AbstractRange)` は
in-bounds (空 range 含む) で nothing、範囲外で catchable BoundsError、Bool 形は
Integer と range の両方に対応。`BoundsError.i` を upstream boot.jl と同じ
untyped に変更 (range/tuple index を保持; 従来の `i::Int64` は range を
DynamicToI64 で潰して TypeError 化していた)。upstream `SubString{T}(s,i,j)`
inner constructor が依存する形。


### macro quote 内の filtered/multi-binding 内包表記 (Issue #10923)

dynamic macro 経路で、quote constructor が upstream AST 形 (`Expr(:filter, cond,
binding...)` / `Expr(:flatten, nested-generator)`) を構築し、macro 展開が IR の
filter スロットと `MultiComprehension` (comma=cartesian / whitespace=flatten) に
マップ。filtered 単一束縛の lazy generator も対応。`ExprHead::Filter`/`Flatten` を
registry 登録 (ネスト専用、standalone 能力なし)。非最内 filter の flatten は明示
拒否 (IR は最内 filter のみ表現可能)。


### module 本体の @eval 関数定義 (Issue #10874)

module 本体の `@eval f(x) = x + 1` は `extract_module_function_defs` (module 専用の
extract wrapper) で関数リストへ hoist され (module 内呼び出しと `M.f` 修飾呼び出しが
解決)、runtime `DefineEvalFunction` 文も本体に残る — top-level Program 経路と同じ
「両方起こる」挙動。


### Base const type alias の qualified 解決と ÷ の関数束縛 (Issues #10579 / #10695)

`Base.Bottom` / `Base.BitSigned` 等の qualified アクセスを、prelude 自身の alias
リスト起源チェック付きで DataType 値に解決 (フラット共有テーブル経由だとユーザー
alias が `Base.X` として漏れるため prelude 定義から target を再計算; `Bottom` は
#10304/#10578 の設計どおり qualified-only)。`÷` は `Base.:(÷)(x, y) = div(x, y)` の
forwarding メソッドで first-class 束縛になり、`@show 7 ÷ 2` / `f = ÷` / missing 混在
が動作。fixture_julia_parity.sh の upstream 表パースをパイプ相対カラムに修正
(testset 名のマルチバイト文字でバイト位置がずれていた)。


### bitwise Bool/Missing の三値論理 (Issue #10692)

upstream base/missing.jl の Kleene 論理メソッド群 (`&`/`|`/`xor`/`⊻` × Missing/Bool/
Integer) を missing.jl に移植。`false & missing === false`、`true | missing === true`、
その他の missing 混在は missing。int.jl の後にロードされるため #8197 の
dispatch-order 契約 (Int64 が mixed-type fallback) を維持。fixture 20 assertion を
25 回反復で安定確認。


### eval() の struct 構築とフィールド読み (Issue #10525)

runtime eval ミニインタプリタが、コンパイル済み struct のデフォルトコンストラクタ
呼び出し (`eval(:(Foo(1)))` — 関数メソッドを持たない struct 名を type-object callable
として `call_runtime_callable_value` の default-DataType 構築へ委譲) と、dot 構文の
フィールド読み (`Expr(:., obj, QuoteNode(:name))` を `getfield(obj, :name)` に
desugar — Julia 自身の lowering 形) に対応。module 修飾の VALUE 読みは #11073 の
スコープとして UndefVarError を伝播。


### hvncat による N 次元ブロック連結 (Issue #10381)

upstream abstractarray.jl の `hvncat` を Pure Julia 移植 (along-dim Int 形、
balanced dims 形、ragged shape 形 + `hvncat_fill!`; `Val` 非依存の sjulia 適応)。
lowering は `;;`/`;;;` 区切り + 非スカラー block のリテラルを shape 形
`hvncat(shape, row_first, xs...)` へ一様に emit する (balanced 入力でも shape 形は
同一結果になることを upstream で検証)。`[A B; C D;;; A B; C D]` が旧 hvcat 経路の
(4,4) 誤形状から upstream 一致の (2,4,2) に。trailing separator の rank パディング、
ragged 行、DimensionMismatch 検証も一致。


### sprint の sizehint キーワード受理 (Issue #10364)

`compile_sprint` が `sizehint` を upstream 同様の no-op 前確保ヒントとして受理
するようになった。context 併用時は従来の sprint_context 経路、print 除外
fast-path では positional `sprint` 呼び出しへ再入し、未知キーワードは従来通り
generic 経路でエラー。非 Float64 の context 経路 (`show(IOContext, T)` 欠落) は
Issue #11420 に分離。


### Channel の closure 内 take! と do-block producer (Issues #10352 / #10353)

`take!` builtin (VM `TakeString`) に Struct/StructRef 受け手の method-table
フォールバック (Length と同型) を追加し、closure/@async 本体の `take!(c::Channel)`
が builtin IOBuffer take! に横取りされる問題を解消 (#10352)。do-block 本体の
`lower_block_simple` に statement 系 NodeKind (for/while/global/local/const) の
一般 statement 経路を追加し、`Channel(sz) do ch; for ...; end` の producer が
lower 可能に。typed `Channel{T}(func::Function, sz::Integer)` producer
コンストラクタを channels.jl に追加 (#10353; sz デフォルトは arity-1 の inner
constructor と衝突するため持たない)。pending producer への `collect(::Channel)`
は別バグとして Issue #11417 に分離。


### Test.@test_skip の契約固定 (Issue #10350)

`@test_skip` は #10273 の統一 @test-family recorder (PR #10367) で実装済みと確認。
式を評価せず (would-throw 呼び出しが実行されない) Broken を記録し、run を失敗させず
exit 0 — upstream と同一挙動を fixture `stdlib/test_skip_broken_10350.jl` で固定。
`fixture_julia_parity.sh` の upstream サマリ表パースを「末尾2数値」からヘッダ位置
合わせのカラム読み取りに修正 (Broken/Fail/Error 列を持つ表で passed=Broken と誤読
していた) し、Total セル空の行で表を終端。


### AoT: isless / mixed-type min-max / 非 Int64 div-family (Issue #10131)

数値マトリクス拡張を塞いでいた 3 つの AoT codegen ギャップを解消。(1) `isless` を
`AotBuiltinOp::IsLess` として実装 — 整数は `<`、float は upstream `_fpint` 方式の
ビットパターン全順序 (NaN 最後尾、`-0.0 < 0.0`)。Base 本体は string builtin
(Issue #7058) と同じ call-graph 葉として遮断。(2) `min`/`max` は混合数値型を
Julia promote 形 (`Float64` 優位 → `Float32` → 広い整数幅、同幅は unsigned) で
キャストしてから比較し、推論の戻り型も promoted 型に。(3) runtime `Value` に
I8/I16/I128/U8..U128 変種と `From`/`type_name`/Display/PartialEq を追加し、
div-family (div/fld/cld/rem/mod) の native 発行が boxed slot に入るときは
`Value::from(...)` で包む。`fld`/`cld`/`mod`/`rem` 等は AoT-claimed builtin として
Base 定義の変換を回避。`Value::F32` の Display は print-form (`2.5`, f0 なし) に修正。
AoT e2e テスト 3 本 (upstream stdout 逐語比較) を追加。

## 最新対応 (2026-07-16)

### owner-scoped struct resolver と bare lookup debt 撤去 (Issue #11046)

`StructRegistry` に declaration-only `(ModuleId, local spelling) -> StructId`
索引と `resolve_scoped` を追加し、exact-qualified → current module → Main/Base
origin → lexical alias の順序を単一 authority にした。parametric instantiation は
bare 表示名と declaring owner を `insert_owned` で分離する。旧
`base_struct_table` / `base_origin_bare_names` と origin-table conversion を撤去し、
`struct_table_bare_gets_compile` を 19 → **0** にした。audit は Main-owner branch
と cache-restore の owner-aware insertion を切断する mutation を検出する。declaration
と lexical alias の登録 API を分離し、同一 layout/type-id の sibling owner を統合しない。
legacy `type_id` の field layout は first-declaration index で決定的に解決し、bare 表示の
module parametric instantiation を含む fresh/cache parity を固定した。

### 不正 UTF-8 String の iterate/インデックス/リテラルパリティ (Issue #8995 再オープン分)

`Value::CharMalformed(u32)` (Julia Char ビットパターン) と共有デコーダ
`decode_julia_char` (upstream string.jl iterate と同一セグメンテーション) を導入。
文字列 iterate は全キャリアで upstream と同じ 1-based byte-offset state になり、
不正バイト列は正確な malformed Char (`'\xff'`, 途切れ multibyte, overlong) を
線形時間で yield する。`s[i]` getindex (StrBytes) / `length` (Julia セグメンテー
ション) / splat (`f(s...)` — 文字列 splat 非展開の既存バグも修正) / 等値・isless
(ビット比較) / `repr` (`'\xff'` エスケープ) / 1引数 `isvalid` / Char 型付き slot・
Char 配列 (malformed は Any 昇格) に対応。`'\xff'` char リテラルと `"\xff"` string
リテラルはバイト指向エスケープ処理 (`\xNN`/`\NNN` は raw byte、`\u`/`\U` は
codepoint) で `Literal::CharMalformed` / `Literal::StrBytes` に降下。concat
(`*`/`string()`) と `print(io, s)` (IOBuffer sink) はバイト保存パイプライン化。
キャッシュ 3 系統 (base/prelude/compile) を bump。`UInt8(::Char)` 等の非 Int64
整数コンストラクタ欠落は Issue #11406 として分離。

### builtin type-name authority の正準 registry 化 (Issue #10954)

exact builtin spelling、nominal `JuliaType` projection、parser/compiler/reflection
visibility を types crate の単一 registry に集約した。`JuliaType::from_name` は dynamic
type grammar のみを保持し、compiler の first-class type-object emit と VM module
`isdefined` は checked projection を参照する。source-only audit は 93-entry contract
全体を fingerprint し、重複 authority と consumer delegation drift を拒否する。
negative mutation は3 consumer を個別に切断し、さらに unsampled row の削除も検出する。

### gcd/lcm の全整数幅対応 (Issue #8812)

同型総称 `lcm(a::T, b::T) where {T<:Integer}` を upstream 形
(`checked_abs(checked_mul(a, div(b, gcd(b, a))))`) で追加し、Int128/UInt8..UInt128 の
同型 lcm と checked オーバーフロー検出を実装 (同型総称 gcd は #9315 で対応済み)。
`checked_abs` / `checked_neg` を checked.jl に移植。混合符号 (`Unsigned×Signed`) と
混合幅 (`Real×Real` promote fallback) も upstream 形で追加し、同型 `T<:Real` の
MethodError terminator で promote-fallback 再帰 (#5966) を遮断。promote は
two-variable 形 (#9513)。fixture 39 assertion を upstream パリティ確認、25 回反復で
dispatch 安定性を確認 (#10775)。

### var"..." 非標準識別子構文 (Issue #8754)

パーサーが `var"name"` を、フル span を保った `Identifier` リーフへマージする
(JuliaSyntax の identifier トークン統合と同形)。名前抽出は `strip_var_quotes` で
引用内容を取り出すため、代入ターゲット・関数パラメータ・関数名・struct/abstract/
module 名・`:var"..."` Symbol・`Meta.parse` の全経路で通常識別子として扱われる。
span がフル範囲のままなので、マクロ引数の span 由来テキスト再構成 (`@test` 等) も
壊れない。補間・エスケープ・flag 付きの文字列は従来どおり string macro へフォールバック。

### module-owned constructor の dynamic call owner 保持 (Issues #11153 / #11367 / #11368 / #11371 / #11373 / #11375)

`compile_call` と qualified module-call routing は、`Dict` / `Set` の public Base 経路や
splat/keyword の早期 return より先に exact/current-module の DataType owner を確定する。
ordinary positional call は exact-qualified outer method を先に試し、該当しない場合だけ
owner-aware default/inferred constructor へ進むため #7729 の outer/default 優先順位も維持する。
local/captured runtime `DataType` の explicit parametric splat/keyword も dynamic apply-type へ
統一する。lifted closure の free-variable producer は `T{...}` の base binding を capture し、
#11375 の dynamic default path を含め Julia と同じく parametric callee を引数より先に評価する。source-only audit と
negative mutation は compiler routing、parametric DataType emission、VM の exact DataType
`MethodError` と default-constructor fallback 境界を固定する。shadow module 内の explicit typed
`Base.Dict{K,V}` / `Base.Set{T}` も #11369 で owner-preserving type-expression 経路へ統合した。

### explicit typed Base constructor の owner 保持 (Issue #11369)

`Base.Dict{K,V}` / `Base.Set{T}` の parametric call head は flat direct-call string ではなく
Base module call として lowering し、fresh/cache-restored registry の両方が Base-origin
parametric definition を source-visible table とは別の authority として保持する。Base function
body 内の bare parametric lookup へこの authority を漏らさず、explicit `Base.T` type-expression
emission と Base-origin struct field の nested parametric substitution だけが優先して参照するため、
shadow module の source-visible `Dict` / `Set` binding が public helper result や `Base.Set{T}` の
`Dict{T,Nothing}` field を奪わない。private registry は definition 選択だけを担い、concrete type は
既存の bare top-level identity を再利用して duplicate `Base.Dict{...}` / `Base.Set{...}` type ID を作らない。
constructor-owner audit は registry、lowering、concrete identity reuse、type-expression emission、
explicit collection result、nested field substitution の 6 境界を独立 negative mutation で固定する。

### typed-loop bail/effect 分類の exhaustive metadata 化 (Issue #10814)

`TypedLoopOp` の data-dependent bail と typed-state 外副作用を、個別
`matches!` denylist ではなく単一の wildcard-free `effects()` match で分類する。
recognizer は集約した 2 facts だけから従来の reject 条件を導出し、未分類の新 variant は
compile error になる。`RandF64` は現行唯一の out-of-buffer effect、`IndexStore*` は
transaction buffer 書き込みのため bail-capable だが out-of-buffer ではない。

### module-local alias target の signature 再正規化 (Issue #11029)

current main は module-local alias の target を method registration 前に owner qualification と
abstract-type reclassification へ通し、ordinary method、abstract supertype、explicit parametric
inner constructor の各 annotation を同じ nominal identity へ解決する。#11029 MWE と abstract
control を既存 #11104 fixture に追加した。struct 表示の owner 欠落は別 bug #11365 として追跡する。

### qualified parametric constructor regression の固定 (Issue #8516)

PR #11057 後の current main は、module-qualified constructor base と同じ owner の
qualified type argument を持つ `M.C{M.V}()` を inner constructor table へ正しく解決する。
再オープン時の bounded-type MWE を既存 #11034 sibling-owner fixture に追加し、owner-exact
qualified call が raw struct allocator へ戻らないことを継続検証する。

### source-order comparison の typed identity (Issue #11100)

type-alias pre-scan の definition と signature use は bare byte offset ではなく、parsed
fragment identity を内包する `SourcePosition` を受け取る。offset 比較は同一 source の
position 間だけに限定し、include/package/cache/REPL 間は従来どおり definition-order
rebase を使う。required source-only audit は typed API、唯一の比較 authority、raw
`span.start`/semantic offset comparison 禁止、registry ownership を negative mutation で固定する。

### binding provenance と runtime global key の authority (Issue #11317)

`Stmt::LocalDecl` の provenance を意味的に読む consumer を TSV へ列挙し、全
`LocalDeclKind` variant の明示 match を source-only audit で固定した。explicit global の
frame-0 load/store は compiler の module-qualified helper に集約し、bare key の再導入を
negative injection で拒否する。既存 consolidated test binary 内の table-driven matrix は
Main/module/function、function/loop/try、explicit/fresh/compiler-generated、normal/exceptional、
typed/dynamic を VM の lowered IR・qualified load/store bytecode・実行と AoT の IR conversion・typed codegen
で覆う。matrix 強化中に判明した残存 gap は #11351 と #11352 へ分離した。

### try-clause lexical ownership と strict soft-scope provenance (Issues #11305 / #11322 / #11335)

`try` / `catch` / `else` / `finally` は clause ごとの lexical owner を持つ一方、module
直下では hard scope ではなく strict soft-scope 判定を受ける。source-order inventory は
mutable global、const global、終了済み clause-local を分離し、mutable global への暗黙代入は
fresh local + upstream warning、const と過去の clause-local は silent fresh local、explicit
`global` は module binding として処理する。assignment-backed binding だけを value slot 候補に
するため、function/generic identity (#11319) はこの変換から除外される。lowering IR、runtime
value、CLI stderr の三層 regression で、後続 loop の phantom warning と compiler metadata
由来の phantom `@isdefined` も固定した。nested try の ordinary assignment は外側 clause-local
slot を再利用し、後から現れた真正な global/const は retired-local marker に source-order で
勝つ。任意 value expression 配下の try と untaken clause の explicit-global effect はそれぞれ
#11159 / #11338 に、retired provenance を経ない direct fresh loop-local の exit 後 lifetime は
#11339 に残し、この statement-level slice では誤って close しない。
### runtime struct bound alias の単一 authority (Issue #11142)

parametric struct の runtime wrapper を作る前に upper/lower bound の visible alias を
`expand_runtime_type_params` で再帰展開し、string-backed / structured wrapper の両経路が
同じ schema を `Core.apply_type` validator へ渡すよう固定した。#11092 の upper-bound
retry helper は削除し、exact-qualified、一意 bare、曖昧 bare、Union/nested alias、binder
保護を unit matrix で検証する。standalone fixture は default constructor による direct
apply と runtime `Foo{T}` allocation を upstream parity で固定し、cache-disabled
AbstractAlgebra residue-ring lane も保持する。alias bound を持つ explicit inner constructor
の dispatch gap は #11003 が継続追跡する。

### REPL definition delta の compiler snapshot 同期 (Issue #9784)

新規 generic/具体 struct の live append 成功時に、再利用 compiler snapshot も同じ
function index・type id・code offset へ transactionally 前進させる。通常式の main は毎 eval
snapshot copy せず、次の定義時に `Nop` gap で live code offset を保存する。これにより
`persistent_compile_stale` と通常の定義直後の全再コンパイルを撤去し、連続定義を live path
に保つ。既存 caller の未解決名を新定義が満たす場合だけは caller 修復のため full path を維持する。

### REPL live VM の runtime-error 回復境界 (Issue #9784)

compiler snapshot を前進させない live delta が Julia-catchable な未捕捉 runtime error で終了しても VM を
破棄せず、VM 所有の回復処理で frame 0 まで unwind する。operand stack、handler、task、
transient root、出力/error/display 状態、RNG は eval ごとに初期化し、global binding、heap/
object identity、module、定義、method world、dispatch cache は保持する。例外より前に完了した
代入・module mutation は暫定 fallback mirror にも同期するため、直後の hard-scope full path が
古い値を復元しない。`x = 41; error(...)` 後の `x == 41` と次 eval の `vm-build=0` を upstream/
differential test で固定した。called function が動的に作る global、その binding の後続 slot
化、host 側 `ans` mirror、redirect stream の LIFO 復元も同じ境界で同期する。advanced
compiler snapshot を伴う definition/type delta と、runtime `@eval` により pre/post definition
fingerprint が変わる call は安全のため VM を破棄する。これらの source-order commit は次
スライスであり、host cancellation と VM-internal invariant failure も破棄側に保つ。
Issue #9784 は継続する。

### package cache restore の nominal type registration parity (Issue #11280)

fresh lowering と `.ji.json` restore は、依存 package の load 成功後かつ loader の
commit 前に、同じ再帰的 Core-IR pass で struct・abstract type・primitive type と
nested-module owner path を thread-local nominal registry へ登録する。回帰 test の
source は空 module、cache payload だけが宣言を持つため、cache-hit wiring を省略すると
必ず失敗する。type-alias table、runtime-type binding、declared-type set、binder/quote
flag などの lowering thread-local は pass 内で snapshot/guard されるため replay 対象外で、
serialized shape は変わらないので package `CACHE_VERSION` は 22 のまま維持する。

### bytecode effect table の production consumer 着地 (Issue #9494)

`subset_julia_vm_bytecode` に conservative な instruction effect table を置き、typed-I64 の
straight-line local CSE を production peephole pass に接続した。未分類 instruction は fail-closed に
barrier となり、slot mutation・basic-block entry・REPL splice boundary を越えて式を再利用しない。
既存 fusion pass と CSE pass の compaction mapping は合成されるため、jump/function/main boundary の
index contract を保持する。現スライスは Add/Sub/Mul の typed-I64 slot 式に限定し、broader value
numbering、bounds-proven `getindex`、loop LICM は #5165 の拡張スコープとして残る。

## 最新対応 (2026-07-15)

### macro loader の cache-independent re-entry contract (Issue #11145)

stdlib と bundled package で重複していた `loaded` / `loading` 遷移を、独立 state を
受け取る共通 helper に集約した。同一 module の loading 中再入は callback を呼ばず、
macro surface の登録完了後だけ loaded を publish し、失敗・panic は loading marker を
RAII で除去して retry を許す。unit test は process-global registry や persistent/preload
cache を使わない fresh state + injected callback で、same-module 再入、success publish、
failure retry、別 module 独立性を stdlib/bundled の双方に同じ matrix で固定する。

### duplicate-branch Clippy の module-zero ratchet (Issue #10725)

workspace advisory probe は lib/test target の重複を除いて 526 件すべてが
`match_same_arms` だった。全件を機械的に統合せず、VM/compiler の高信号 module を
zero 到達時に `deny` する方針へ固定した。最初の対象
`subset_julia_vm_vm/src/vm/exec/return_ops.rs` は
`ReturnRng` / `ReturnRange` / `ReturnRef` の continuation 処理を1 helper に集約し、
module-level deny で再分岐を拒否する。再計測は workspace 525 件、対象 module 0 件、
`if_same_then_else` 0 件である。

### compile / VM の物理 crate 分割 (Issue #9090)

`compile/`、`vm/`、lowering core をそれぞれ独立 crate へ移し、main crate は loader、
macro expansion、cache、cancellation の composition root になった。compile↔VM coupling は
runtime/test とも 0 で、片側の touch 後も他方は Cargo 上 `Fresh` のままになる。cold check
中央値は分割前 47.9 秒から 34.41 秒、VM-only / compile-only の温間中央値は 2.32 / 3.19 秒。
full nextest 5,529件、AoT gate、native FFI build は green。iOS/WASM 実ビルドは Linux host に
Apple target / `wasm-pack` がないため未実施だが、Web を含む host-side suite は green。

### try clause hard scope と dynamic string budget の parity (Issues #11281 / #11301 / #11308)

`try` / `catch` / `else` / `finally` の新規 local と catch binder を clause exit で破棄し、
explicit `local` shadow は正常・例外・return/break/continue の全 exit で外側の binding を復元する。
一方、既に初期化済みの enclosing binding と lowering-generated result slot は clause local として
誤消去しないため、caught const reassignment も元の global を保持する。clause-local が `Any` に
wide 化して dynamic `*` を通る String/Char concat にも typed `StringConcat` と同じ memory budget
check を適用した。MacroTools fixture は upstream でも binding を閉じ込める `@test @capture(...)`
を避け、capture と assertion を sibling statement に分離した。
module-owned function / catch clause の explicit `global` は flat frame-0 key を module path で
qualify し、Main の同名 binding へ誤配送しない (Issue #11312)。また typed `local` が追加する
transparent `Stmt::Block` を AoT `@time` flattener が再帰し、caller assignment の side effect を保持する。

### Base export manifest の upstream parity 復旧 (Issue #11162)

ordinary module の implicit `Base` binding を Base の export metadata に混入させた残余を
除去した。`Base` 自体は language-level implicit binding であり upstream の export ではないため、
既存 `base_exports_do_not_exceed_upstream` gate が再び全 identifier の差分を検査できる。
Issue #11298 で同じ subset 関係を VM link 不要の source-only audit に移し、required
premerge/CI registry row と `Base` 注入 negative control で固定した。既存 Rust test は
別 parser による defense-in-depth として維持する。

### AoT generated Rust の borrow/clone/move 規約 (Issue #11202)

generated call site は callee signature と値表現を先に分類し、borrow を優先、owned
`Value` の clone は Julia の alias semantics を保つ場合だけ、move は rvalue または
binding-aware last-use 証明時だけ許可する規約を追加した。typed container / mutable
struct の deep clone は rustc 回避策として禁止する。#10663 の実 `count_items` 出力を
一時 Cargo crate で check する negative control は、同じ `itr` を2回の `dynamic_call`
に渡す E0382 を検出する。#10663 はこの test を positive compile gate へ反転して閉じる。

### semantic-ID 計画の as-landed verdict 同期 (Issue #11284)

Phase 2a/2b/3 の投機的計画を landed evidence に同期し、機械 inventory を
`identity-bearing` / `lexical-boundary` / `inert` に分離した。全 site は未判定を
identity-bearing とする fail-closed 分類で、6 core domain の #11078/#11095/#10460
実残余だけを Phase 4 の退役対象として集計する。#11191 で判定済みの12テーブルと
ModuleInternTable/StructRegistry/TypeVarScope の name→ID 境界は明示ルールで固定した。

### Deterministic compiler emission and nominal owner slice (Issues #10460 / #11264)

`CreateClosure.capture_names` は全 emission site で sort 済みとなり、fresh compiler process ごとの
`HashSet` seed が serialized Base bytecode を変えない。determinism subprocess は persistent
prelude/Base cache を無効化するため、実際に各 child が parse/compile した payload を比較する。
nominal parametric instantiation は bare alias が owner collision domain に入る場合、alias と同一な
qualified declaration を canonical key/name として保持する。これにより fresh package lowering と
`.ji.json` restore で `AbstractAlgebra.Integers{BigInt}` の compile-time owner が一致する。
`isa` flow narrowing も exact concrete type のみに限定され、parametric instantiation table の
random first entry への依存を除去した。loader restore の nominal registry parity は #11280、
残る type representation debt は #10460 で継続する。

### Structural source-binder rebind slice (Issue #10460)

runtime `UnionAll` constructor が、宣言済み source binder と同名の builtin/registered nominal
leaf を canonical `CoreType` graph 上で TypeVar へ rebind し、元の runtime binder ID を bodyへ
付与する。builtin-shadow binder は bare generic alias と semantic equality を保ちながら source
object identity を維持し、nested binder/bound graph は文字列 round-trip なしで保持する。
inner bound から outer binder への runtime ID 参照、bare/qualified leaf 混在時の lexical capture、
runtime Vararg/VarargLen の direct・wrapper・dispatch canonical projection も同じ構造境界で固定した。
この upstream-parity boundary の移行により `vm_type_utils/julia_name_projection` は 1 → 0。
残る semantic type-string bridge は #10460 の他 bucket で継続撤去する。

### constructor call site の owner-loss ratchet (Issue #11172)

compile-time の `short_constructor_name` probe を semantic function owner ごとの
明示 inventory に固定し、追加・移動・重複を guarded source audit で拒否する。
owner-checked projection と #10992 が退役させる legacy fallback を区別し、未完の
semantic-ID 移行を解決済みと誤表示しない。runtime DataType/apply-type 側は qualified
exact lookup、canonical parametric owner、unique-only bare default fallback を同じ audit
で固定した。negative control は direct leaf probe、runtime owner comparison 無効化、
registry row 弱体化をそれぞれ検出する。

### @testset 内 named function の lexical capture (Issue #11260)

`@testset` が展開する marker-only `let` は外側へ local を漏らさない一方、scope
内部では実 binding を持つ hard scope である。compiler の named-function capture
prescan を実行順にし、definition 時点で store 済みの testset-local binding だけを
function body compile 前に登録する。mutable array と scalar の capture を
short/full form の両方で upstream と一致させ、後続 assignment の過捕捉も避ける。

### semantic audit-selftest anchor と changed-target gate (Issue #11274)

negative-control の source edit を fail-loud exact-one literal / semantic-regex helper
へ集約し、binder-sensitive owner は local 名を capture して incidental spelling から分離した。
全 helper anchor は TSV で分類し、raw count/replace の再導入と inventory drift を
registration mode が拒否する。`--target-path` / `--changed-from` は mutation target から
bounded control set を導出し、guarded premerge が変更 target の control を自動実行するため、
#10895/#11269 の stale-anchor class を通常の source refactor で検出できる。

### cfg(test) source-audit scanner contract (Issue #11208)

type-representation string-reparse audit は repository baseline の走査前に synthetic
matrix を実行する。`#[cfg(test)]` 直後、blank line、adjacent attribute の各 test module
内 token は production debt から除外し、cfg-gated non-module item 後の production token
は必ず検出対象に残す。negative harness は pre-#11207 の trivia state 遷移を再注入して
focused diagnostic を検証し、既存 production-token injection と合わせて false positive /
false negative の両方向を固定する。同じ full harness が検出した #11269 では local-shadow
injector の `_` 固定 anchor を parameter-tolerant な exact-one arm 検出へ修正した。
### structured Type-object dispatch の parametric lower-bound 検証 (Issue #11233)

compile-time の `Type{W{T}}` structural binding が `T` を抽出した後、宣言済み
upper/lower bounds を捨てて再 binding していたため、`T>:Lower{Int}` を満たさない
concrete parameter でも bounded method を選択していた。invariant な Type-object
position では抽出 binder を元の両 bounds と登録済み struct hierarchy で検証するよう
統一した。これにより user-defined abstract supertype も `T>:Leaf` を正しく満たす。
direct/runtime call の lower-bound 不成立・exact 成立・abstract-supertype 成立と、同じ
structured path の upper-bound 成立/不成立を cold/warm cache lane の upstream parity
fixture で固定した。

### struct-new lexical authority の境界監査 (Issue #11211)

既存の LambdaContext routing audit を R1-R6 へ拡張し、`new_struct_name` の全 mutation
を root helper / lifted function / runtime eval / structural collector の owner seam に限定した。
post-hoc な slice/watermark stamp を注入する negative self-test で検出感度を固定し、
ownerless lookup 4 fixture を同一 semantic family として登録確認する。CHECKLISTS には
ordinary nested、lifted closure、macro-lifted thunk、runtime eval、eval descendant、
ownerless lookup の6境界を明記した。
### STATUS/DONE archive budget restored and locally enforced (Issue #11263)

`STATUS.md` と `DONE.md` の古い日付セクションを本文を変えずに 2026 archive
へ移し、両 live file を 3,000 行未満へ戻した。複数回の archive batch 後も
newest-to-oldest 順と section 本文を保つ self-test を追加した。固定閾値の read-only
budget check を source-only audit registry の必須 guarded-premerge default とし、
STATUS/DONE の独立超過、aggregate runner 到達、registry row の削除・弱体化を
negative control で固定する。

### Pinned Rust contract and executable Clippy lane inventory (Issues #11253/#11258/#11259)

workspace packageは `rust-version = 1.95` を継承し、local certification は
`rust-toolchain.toml` の exact 1.95.0、CI は同じ lane registry を moving stable で
再実行する。`default` は `--workspace --all-targets` で FFI を含み、feature-only
`repl` / `aot` / `aot,cranelift` と generated AoT Rust の owner を明記した。
`check_rust_toolchain_contract.sh` は pin・全 manifest・lane 列挙・premerge/AoT/CI
owner を source-only premerge で固定し、negative mutation は AoT owner が default
lane へ弱められた場合に失敗する。初回 combined lane で見つかった #11258 の
compile error 1件と Clippy error 6件、Rust 1.97 moving lane で見つかった #11259
の AoT lint 5件も解消した。target-only iOS/WASM は既存 platform build gate が
継続所有する。
### Candidate-order independent runtime resolver coverage (Issue #11252)

`RuntimeCoreCandidate` / `RuntimeCoreSliceCandidate` /
`RuntimeTypedCoreCandidate` / `CallableValueCandidate` の4 adapterを同一の
user-abstract vs builtin-generic signatureで forward/reverse 実行する matrixを追加した。
最初の row を返す seeded control は失敗するため、テスト自体の感度も固定される。
既存3 adapterは両順序で unique specific methodを選択し、残っていた callable-value
pathは同点 top setだけ structured strict dominanceへ送り、通常 pathの allocationを
増やさず registration order依存を解消した。未解決の真の ambiguity統合は #10461。

### lowered fragment の definition-order chronology 集約 (Issues #11036/#11128/#11134)

`DefinitionOrderCursor` が独立 lowering された Program/Module の順序を一元管理する。
prelude/Base/REPL は cumulative append、package source/`.ji.json` restore は package
local と caller の stamped `using`/`import` 位置へ再帰的に挿入し、後続定義だけを shift
する。stored definition と module/main/function/macro/inner-constructor block 内の
executable copy を同時に rebase し、block-local method の誤った forward-reference
probe (#11144) も防ぐ。compiler の package method precedence と type-stability frontend
も同じ chronology を消費し、Core-IR v5 は旧 chronology を拒否する。全
merge/replay/cache/AoT path は executable inventory と alias/rename を含む negative
mutation で固定される。
### Structured generic-alias parameter slice (Issues #10460 / #11241)

`VectorOf` / `MatrixOf` の UnionAll alias parameter を display `.name()` ではなく
structured `TypeVar` node から読む共通 extractor に集約し、builtin 名を shadow する
legacy binder 用 fallback だけを残して audited name projection を 2 → 1 にした。
type-representation string-reparse ratchet 自体も guarded-premerge
registry に登録し、#11236 で露呈した「scanner/self-test/ci.yml は存在するが実 merge
authority が実行しない」穴を閉じた。残る semantic bridge は #10460 で継続撤去する。

### Structured array element-type slice (Issue #11236 / #10460)

配列要素型の logical type を文字列へ落として再 parse する境界を 1 件撤去した。
`ArrayElementType::Structured(Box<JuliaType>)` が partial `UnionAll` と入れ子 runtime
`TypeVar` の identity graph を保持し、allocation・index・`eltype` / `typeof` reflection
を通して同じ構造を返す。rank 3 以上の配列型も structured に組み立て、compiler inference
では boxed storage を `Any` に widen して既存 iteration semantics を維持する。
`vm_exec/julia_name_projection` ratchet は 40 への増加から baseline 39 へ復旧した。
残る type-string bridge の inventory と撤去は #10460 で継続する。

### Lezer-compatible parser rewrite: M0/M1 着手 (Issue #11049)

- 仕様書 (全文アーカイブ: Issue #11225) のロードマップ M0 (oracle CLI +
  正規化 prototype + corpus) と M1 (Canonical CST 共通モデル
  `subset_julia_vm_parser_common`) を実装。詳細と運用手順は
  `docs/vm/LEZER_PARSER.md`。
- 次段: M2 Lexer (`subset_julia_vm_parser_lezer`)、`subset_julia_vm_parser_diff`
  (legacy/new/oracle 三者比較)、legacy parser → Canonical CST adapter。

### Structured nested-TypeVar construction slice (Issue #10861 / #10460)

runtime TypeVar を直接引数にした場合だけでなく、`Vector{A}` のような
identity-bearing `DataType` をさらに Tuple / Dict / Type / Array へ入れた場合も、
型構築・UnionAll instantiate・reflection parameters が同じ structural graph を使う。
元 MWE の whole-type identity に加え、二重 nesting、singleton `TypeOf`、builtin/user
partial UnionAll の binder/bound を upstream differential fixture で固定した。
これは #10460 の alpha-equivalence / structured conversion vertical slice であり、
残る semantic type-string bridge の inventory と撤去は同 epic で継続する。

### techdebt: 例外型 taxonomy funnel + 型不一致修正 (Issue #11146 / #10813 Phase 2a)

`VmError::exception_class()` を variant → upstream 例外クラスの唯一の写像(catch-all 無しの
網羅 match)とし、例外オブジェクトの struct 名と catchability を**そこから導出**。raise 箇所は
クラスを選べなくなり、新 variant はクラス宣言までコンパイル不可。メッセージ文字列だけは
コンパイラで縛れないため、監査 `check_exception_taxonomy_funnel.sh`(source_only_audits.tsv
登録 + negative self-test 4本)で R1-R4 を強制 — Rust 側の矛盾サイト一掃に加え、Julia 層の
`error("<Class>: ...")` 64件を `throw(<Class>(...))` に変換(表示は byte-identical、
`typeof(e)` のみ変化)、残 58件(コンストラクタ引数要)はラチェット。corpus 型不一致 2件
(`convert(Int,"a")`・非callable 呼び出し → MethodError)を修正、残 2件は in-flight PR #11163
が所有のため意図的に重複回避。Phase 1a 引継ぎ 2 バグ(eval の型付きパラメータが `String` を
catch させていた)は upstream 同形の「定義時にシグネチャ注釈を即評価」で解決し、fixture は
`@test_broken` ではなく**実検査される assertion に強化**。probe 発散 12 → 8(**corpus の型のみ不一致は 0 件に**; 残り 8 は silent/spurious/raise-layer
で Phase 2b/3 の担当)。詳細は
`docs/vm/EXCEPTION_PARITY.md` "Phase 2a outcome"。
### Structured UnionAll alias recognition slice (Issue #11013 / #10460)

runtime TypeVar の表示名ではなく identity graph を alpha projection して
Vector-style generic alias を認識する経路を、display・equality・両方向 subtype
で整合させた。通常名と macro hygiene gensym を fixture で upstream 比較し、
lower-bounded wrapper が alias へ誤 collapse しない負例も固定した。
これは #10460 の vertical slice であり、残る type-string bridge の撤去は同 epic
配下で継続する。

### main の source-only gate 復旧 (Issues #11179/#11183/#11186/#11187/#11197/#11204)

- struct body の `global` helper lowering を live `LambdaContext` authorityへ接続し、
  helper 内で lift された closure にも enclosing struct の `new` owner を伝播する。
- helper 抽出を direct/module/transparent/@kwdef で共通化し、mixed/multiple splat
  `new` も全引数・評価順を保持する。
- nested collector は `new` authority を lexical descendants にだけ伝播し、runtime
  `@eval` function を hard boundary として ownerless の通常 name lookup に戻す。
  lifted function は生成時に authority を受け取るため async/task thunk も境界を越えず、
  anonymous closure の undefined `new`、ownerless `new{T}`、keyword/splat を保持する。
- PR #11136 が増やした structural-debt 2項目を実測値と理由付きで ratchet に反映し、
  `run_source_only_audits.sh` 13/13 を復旧した。

### Root-cause quality prevention ownership (Issue #10452)

- Historical label events reconstruct the exact 403-Issue population into
  `QUALITY_ROOT_CAUSE_TRIAGE_2026_07_11.tsv`; unmatched local symptoms remain
  self-owned instead of being forced into a structural class.
- Four comparable UTC cohorts are frozen in
  `QUALITY_WEEKLY_BASELINE_2026_07_11.md`. The #10465 five-group harness now
  auto-runs for semantic-pipeline paths and covers all three AoT acceptance
  kernels. Open production work remains owned by #10460–#10464 and
  #10813–#10815.

### Phase 2b/2c: 構造体の所有者スコープ ID 化 (Issues #11078/#11046) — 完了

**完了**: `struct_table` / `base_struct_table` の re-key
(`structinfo_name_maps_compile` 61 → **0**)、`StructId`/`StructRegistry` 基盤、
fresh vs cache-restore の `StructId` 同一性テスト、#10459 ratchet の
source-only audit 登録。詳細は DONE.md 参照。

**#11046 で完了した残作業**:

1. **`struct_table_bare_gets_compile` 19 → 0**。owner/scope-aware resolver を導入し、
   Main/Base origin と current-module declaration を lexical alias から独立に解決する。
2. **`CoreType::Struct` の owner 認識(非 dispatch 経路)**。#11078 が挙げた項目 3 は
   dispatch 経路については PR #11138 が既に解決済み
   (`from_julia_name_for_dispatch` / `preserve_user_owner`、
   `has_qualified_nominal_family_collision` で gate)。残るのは
   `typejoin` / promotion / specificity など非 dispatch 経路のみで、issue 記載の
   「44 サイト」はこの意味で過大。
3. **`StructDefInfo.id` と relocation table**: 不要と判明 (上記 Pattern A)。
4. **`base_struct_table` / `base_origin_bare_names` (#10078/#10257) の撤去**:
   registry の owner-name index と canonical enumeration が置換した。
### design/prevention: 例外の型・発生層・捕捉可能性 Phase 0 棚卸し (Issue #10813)

Issue #10813(例外の型・発生層・捕捉可能性が上流と系統的に乖離し、
`@test_throws` の型無視がクラス全体を不可視にしている)の Phase 0
(棚卸し + decomposition) を実施。3 つの主張をすべて実測で検証:
(1) 例外**型**の乖離 — 型のみ不一致 4 件（`method_error_noncallable` の
TypeError vs MethodError、`convert_failure` の TypeError vs MethodError
など; `scripts/exception_parity_probe.py` の 40-case corpus で
`docs/vm/EXCEPTION_PARITY_PROBE.tsv` に記録）。(2) 発生層の乖離 — 確認
されたが縮小傾向: Evidence 記載の 3 件中 2 件(#10406/#10511)は既に
closed で今回 upstream と完全一致(regression sentinel として corpus に
常設)、残る #10593 のみ再現。(3) `@test_throws` の型無視こそが検出網の
盲点であることを実測: スローアウェイパッチ(未同梱、適用→計測→revert)で
161 fixture 中 7 file / 21 中 13 assertion が Pass→Fail に反転し、
julia 実行と突き合わせた結果 **13/13 が genuine sjulia bug**(fixture の
過剰指定はゼロ)。副産物として `regex_recursion_reject_10181.jl` の 8
assertion は `@test_throws "message"` (メッセージ部分一致形式)が
未実装なことによる計測アーティファクトと判明— 本来の修正には
`isa T` と `String`/`Regex` 部分一致の両方が必要。詳細・全データは
`docs/vm/EXCEPTION_PARITY.md`。Phase 1a は既存 #10354 を採用、Phase
2a(#11146 taxonomy funnel)/2b(#11147 raise-layer sweep)/3(#11148
generic-fallback sweep + enforcement ratchet)を新規に子 issue として
`#10813` にネイティブリンク。本 PR に Rust 本体変更なし
(`scripts/exception_parity_probe.{sh,py}` は report-only、never-gating)。

### AoT vm_aot 差分レーンの拡張 + 常時ゲート化 (Issue #10815)

両主張を実測で検証: (1) AoT が scope/型導出を独自再実装している —
`declared_locals`/`compute_hoisted_locals`(#10251/#10523)、Rust
pattern-position での `const im` 衝突(#11180 新規)の 3 サイトで確認。
(2) 差分ゲートは acceptance kernel 3 本のみ — `tests/equivalence/vm_aot.tsv`
を実測で確認、**11 ケースに拡張**(Bool/比較/単項演算子、
gcd/lcm/factorial、String 連結、再帰関数、break/continue、既存だが
未参照だった `builtin_stdout_parity_6999.jl`/`mandelbrot_scalar_aot.jl`)。
拡張作業自体が新規 AoT bug 3 件(#11180 `im` shadowing、#11181 `range()`
prelude ヘルパーが呼び出し不能、#11182 配列添字代入が無効な Rust lvalue
を生成)と `--features aot` テストビルド破壊 1 件(#11196、PR #11005 が
`Function`/`StructDef` の新規必須フィールドを 27 箇所の `#[cfg(test)]`
サイトに反映し漏れ — 誰も `test_aot.sh` を定常運用していなかったため
不可視だった、まさに本 Issue の主張そのもの)を検出。既知の乖離
(`scope_sibling_rebind_10251`, 対 #10523)は `EQUIVALENCE_KNOWN_DIVERGENCES.tsv`
に two-sided 登録。**強制**: `bash scripts/test_aot.sh`(必須 AoT ゲート、
AGENTS.md hard rule #8)が `--lane vm_aot` + `--selftest` を新規ステップ
[4/8]-[6/8] として実行(従来は lead 認証時の `premerge_gate.sh
--metamorphic` でしか回っていなかった)。`scripts/check_test_aot_vm_aot_lane.sh`
(source-only, `scripts/source_only_audits.tsv` 登録、negative selftest 2 件)
が配線と corpus 行数下限(11)を ratchet。再実装クレームは 3 分割の
decomposition issue に整理(#11195 scope/binding-identity → #11200
statement/assignment-target lowering(依存)→ #11202 ownership 規約文書化、
優先度低)。元 proposal の P0(`--pure-rust` smoke)は #10731 未修正のため
意図的に未着手(付けると即座にゲートが赤化する)。詳細:
`docs/vm/ADR_BACKEND_STRATEGY.md` §Differential verification。
### techdebt(#10459) Phase 2a continuation: module/global 12テーブルの scope judgment 完了 (Issue #11032)

#10988 が re-scope した 12 個の named module/global テーブル
(`module_functions`/`exports`/`constants`/`struct_names`/`usings`/
`abstract_names`, `module_imported_bindings`, `global_types`/
`inference_global_types`/`global_const_structs`/`global_struct_names`,
`module_aliases`) を per-table で判定。11 個は完全修飾モジュールパス
(または dot を含まない識別子と組み合わせた複合キー)で構成上衝突不可能な
canonical identity と確認、残り3個の bare 名キー (`global_types` 系) は
upstream Julia 1.12.6 との MWE 比較で実害なし(widen-to-`Any` 安全網 /
参照側の動的解決)と確認。`ModuleId` 再キー化は対象なし — `check_name_based_lookup.sh`
は module/global ドメインを一切ゲートしていないため、そもそも動かせる
baseline が存在しない。

唯一の実バグとして `module_aliases` の `imported_submodule_aliases`
(`HashSet` 反復順依存の同名サブモジュールエイリアス衝突、後勝ち)を発見・
Issue #11176 として起票し修正(source-ordered `resolved_usings` を使った
先勝ちに変更)。`docs/vm/SEMANTIC_ID_MIGRATION.md` の Phase 2a continuation
節と #11032 の phase sub-issue 表を更新。`SEMANTIC_ID_INVENTORY.tsv`
regenerate: module/global ドメインとも変化なし(55 / 63)。
### bug: `@test_throws` の型無視を修正、Phase 0 実測の 13 bug を精査 (Issue #10354 / #10813 Phase 1a)

Phase 0(直前セクション)が計測した `@test_throws` の検出網の盲点を解消。
`_test_throws_matches` を新設し upstream `do_test_throws` の全形態
(Type/例外値/String/Regex/Array/Function)を実装、Fail メッセージは
`Expected: T / Thrown: U` を明示。この修正で 161 fixture 全ての
`@test_throws` 挙動が変わるため、Phase 0 が特定した「7 file / 13
assertion が Pass→Fail に反転する」問題を同一 PR で解消: **11/13
(6 fixture)** は呼び出し箇所固有の局所修正(誤った `VmError` バリアント
選択・`length(f::Flatten)` 未実装・`Memory{T}(undef)` の `undef`
解決漏れ = Issue #10737 close・未定義関数呼び出しの `UndefVarError`
未送出 の5クラス)で upstream 一致に到達。**残り 2/13**
(`types/signature_forward_reference_11025.jl`)は `eval` のランタイム
関数定義が型付きパラメータ未実装で `VmError::NotImplemented`(Issue
#8664 の設計により Julia 例外オブジェクトを持たず生 `String` が
catch される)を送出する機能ギャップで、局所修正の scope外と判断し
`#11146`(taxonomy funnel epic、"its own distinct defect" として本ケースを
既に明記)に委譲、fixture は `@test_broken` で正しい期待値のまま
トラック(型を弱めていない)。詳細は `docs/vm/EXCEPTION_PARITY.md`
"@test_throws type-check impact" セクション。回帰カバレッジ:
`tests/testset_exit_code_8191_tests.rs`(RED half — フェイル記録は
fixture harness の gate で不可、Issue #9360)+ 新規 fixture
`stdlib/test_throws_type_check_10354.jl`(GREEN half)。`Instr`
新変種で Base cache schema fingerprint 変更 → `CACHE_VERSION` bump。

さらに main への rebase 時点で **14件目**を即座に検出(型検査は一度きりの
監査ではなく常設の検出網): 本ブランチ分岐後に landing した #11135
(PR #11155)の fixture が arrow lambda
`(y, x=2; k::Integer = "oops") -> ...` に `@test_throws MethodError` を
主張(upstream 通りで正しい)。sjulia は `UndefVarError` を投げており、
main 上では harness が型を見ないため "pass" していた。根因は arrow の
parameter collector 2箇所が named function 用 `signature.rs` の post-`;`
keyword 判定を**部分コピー**していたことで、(1) 型注釈付き keyword が
どの arm にも当たらず黙って捨てられ signature から消滅
(→ body の `k` が global load = UndefVarError、`k=` 供給は
"unsupported keyword argument"; declared type も捨てられ #11024/#11081 が
arrow に未適用)、(2) parser の rewrap が list 全体に適用され `;` 前の
位置引数デフォルトまで keyword 化(`f(1,5)` が NoMethodFound)。
`signature::parse_kwparam_node` を単一 authority として抽出し arrow 2箇所を
そこへ通す構造的修正で、arrow 全 11 形態が upstream と一致、
`annotated_kwarg_default_type_11135.jl` は 48/48 に(assertion は不変)。
発見した第3の shape(匿名 `function` 値形式の parameter 消失)は
**#11174** として起票。

## 最新対応 (2026-07-14)

### nested module の lexical module binding 可視性 (Issue #11132)

compiler/inliner が global module registry を lexical scope と誤用していたため、
`P.C` から未 binding の親名 `P` や無関係な `Q` を qualified root として使用できた。
scope-aware module path resolver を module value/call/ref/alias 作成へ共通化し、inliner
にも current module stack に基づく同じ境界を適用。不可視 path は compile error では
なく module-scope lookup を実行して catchable `UndefVarError` を出すため、`try/catch`
の upstream 挙動も一致する。Base の暗黙 module binding は export metadata と module
registry から構造的に判定し、未 import の `Random` は不可視のままにする。relative
whole-module import が module 名を binding する #11137 も同じ resolver で解決した。
AbstractAlgebra の selective relative type import が露呈した bundled macro loader の
自己再入は loading-state guard で停止し、package source cache miss 時の stack overflow
(#11141) も解消した。
### method signature の type alias 可視性を source order 化 (Issue #11086)

type alias pre-scan を leaf 名ごとの履歴へ変更し、各 entry に lexical module owner、
明示的 source identity、定義 byte offset、登録順を保持する。method signature だけは
同一 source identity の entry に限って use-site offset 以前を可視とし、include・package・
過去 REPL eval の無関係な offset は比較しない。owner-exact entry が現 source では未到達
の場合に Main/sibling の同名 alias へ fallback しない境界も固定した。通常の canonical
alias expansion は従来どおり source order 非依存。REPL は過去 eval の top-level/module
alias を originless entry として seed し、同一 eval 後方の再定義より前から利用可能にした。

Julia parity fixture 13 件、alias unit 22 件、REPL session 2 件、uncached/cache-prime/
cached lane で検証。source metadata は lowering TLS のみで serialized IR/bytecode 形状を
変えないため cache schema bump は不要。調査中に発見した bare alias chain
(`B = A` where `A` is an alias) の別 gap は Issue #11099 で追跡する。
### struct body の bounded inner ctor と global new helper (Issues #10998, #11005)

`Foo{T}(x) where {T<:Number}` の bounded where binder は binder 名 `T` +
upper bound `Number` として lower され、宣言境界は違反インスタンス化を
upstream 同様 MethodError で拒否する。実行時型引数 (`Foo{typeof(x)}(x)` /
`Foo{t}(x)`) も raw dynamic allocator に落ちず、宣言された inner constructor を
実行する: `CallStaticParametric` に `runtime_binding_names` を追加し、スタック上の
型引数値を callee frame の type binding として束縛しつつ binder 境界
(sibling binder 依存の `V<:AbstractVector{T}` を含む) を実行時に検査する。
また struct body 内の `global helper(...) = new{T}(...)` (upstream `Rational` の
`unsafe_rational` 形) を通常の global method として登録し、その body だけが
enclosing struct の `new` を保持するようにした。CACHE_VERSION 139。
### `catch e` が型付き外側ローカルと衝突するとコンパイル内部エラーになる問題の修正 (Issue #10999)

`function f(); e = "outer"; try error("boom") catch e end; return e; end` のように
`catch` 変数名が `Any` 以外の静的型を持つ外側ローカルと衝突すると、
`Type error: expected String, got "StructRef"` で落ちていた。upstream は catch 変数を
shadow せず同名ローカルを恒久的に上書きする(検証済み)ため、`stmt_try_catch.rs` の
無条件上書き自体は正しい形であり、バグは型付けだけだった。関数全体の slot 型 pre-scan
(`compile/inference.rs`) と抽象解釈の戻り値型エンジン (`compile/abstract_interp/engine/mod.rs`)
の両方で catch 変数を catch 分岐の入口で `Any` に束縛し、concrete 型と衝突する場合は
dynamic slot に落とすようにした。String/Int/Float64/引数/入れ子 try/try 後参照の
各形を fixture 化 (`scope/catch_var_overwrites_typed_local_10999.jl`)。

### `let` 内 `global function` が let ローカルを capture できない問題の修正 (Issue #11015)

`let counter = 0; global function inc() counter += 1 end; end` (Base bootstrap
パターン、`Base.include` の `SOURCE_PATH` と同型) が `UndefVarError: counter` で
落ちていた。根本原因は「`let` スコープが capture スコープとして扱われていない」ことで、
`global` なしの named function や関数内 `let` でも同じ欠落があった(後者は
silently wrong output)。対応:
(1) `lowering/closure_box.rs` で実バインディングを持つ `let` を defining scope として
    扱い、capture かつ再代入されるローカルを `Ref` に box 化、
(2) `compile/pipeline_ctx.rs` に module-level `let` スコープ内 named function の
    capture 事前解析を追加(関数本体は `compile_main` より前にコンパイルされるため)、
(3) `global function` を `Stmt::Global` マーカー付きで lower し、closure 値を
    module-level 名に束縛 (`StoreGlobalAny`)、呼び出しをその closure 値経由に routing、
(4) `activate_eval_function` が同名 closure 束縛を plain function で上書きしないようにした。
一つの `let` ローカルを共有する複数 `global` メソッド(= `Base.include` の形)と多重ディスパッチも動作する。
### kwarg default の型注釈・provenance 境界 (Issue #11135)

注釈付き keyword の不正なデフォルトが黙って受理される gap を解消。省略 keyword
は reduced-arity stub から実値でなく NOT-SUPPLIED sentinel として転送し、default
所有側の full method が literal/call-valued default を実体化する。user body 前の
first-store check が final bound value を declared type に照合し、upstream と同じく
不正 default は `MethodError`、caller-supplied mismatch は `TypeError`、required
omission は `UndefKeywordError` のまま。optional kw slot は default 式の型で固定せず
`Any` とし、named/arrow/full-form anonymous value/IIFE の入口を同じ prologue 経路へ
統合した。heap-backed struct の supplied 値も Julia 型名で検査する(Issue #11024)。
`where` annotation は supplied/default の両境界で同じ frame binding を構造置換し、
全 default guard を左から実行した後に validation store を行う二相 prologue とした。
body-evaluated default の guard store は per-frame skip count で materialization として
通し、その後の self-store だけを検査する(supplied/literal は skip 0)。
package loader cache は同形の古い `Function.body` を識別できないため version 19→20
で無効化した(Issue #11154)。fixture matrix と再発防止 checklist(Issue #11140)で固定した。
### macro quote の signature binding 位置での esc/補間識別子 (Issue #11014)

macro の quote 内で `function f($(esc(pname)))` のように **束縛位置** —
引数名・引数の型注釈・`where` 束縛の型変数名 — に esc された識別子を補間すると、
`macro expansion returned unsupported function parameter (type) Expr` で lowering
が失敗していた。`function_param_from_value` / `julia_type_from_value` /
`struct_type_param_from_macro_value` が bare `Symbol` 形しか認識せず、
`Expr(:escape, ...)` / `Expr(:hygienic-scope, ...)` を剥がしていなかったのが原因。
関数自身の名前 (#8066) と同じ `macro_assignment_target` で unwrap するように修正。
esc された識別子は呼び出し側で解決され hygiene rename されない (#10925) という
不変条件はそのまま。名前衝突ケース (esc 引数名と同名の quote-local) の upstream
差分は Issue #11107 に分離。

### Unicode 演算子の文字集合を upstream の優先順位表から導出 (Issue #11083 / #11110)

lexer が認識する Unicode 演算子が ad-hoc な allowlist だったため、`⊛` `⊞` `⊠` `⋆`
などは演算子として lex されず (Identifier に落ちて) パースエラーになり、`⊗ᵢ` のような
suffix 付き演算子名も使えなかった。`julia/src/julia-parser.scm` の優先順位表
(`prec-arrow` / `prec-comparison` / `prec-plus` / `prec-times` / `prec-power` /
`prec-colon`) と `julia_opsuffs.h` (`jl_op_suffix_char`) から機械的に文字クラスを
導出し、単一の catch-all トークンを **upstream の優先順位クラスごとの** トークンに
分割。dotted (broadcast) 形も base 演算子のクラスを引き継ぐようにして、
`xs .⊛ ys .+ 1` の誤結合 (Issue #11110) を解消。Identifier の start クラスからは
演算子文字を差し引いたので `a⊛b` のような空白なし中置も upstream 通りに lex される
(`∞` `∇` などの非演算子記号は従来どおり識別子)。parser corpus は
`julia/test/bitarray.jl` が新たに clean になり allowlist から除去。
### const 型エイリアスの引数注釈が dispatch するようになった (Issue #11104)

`const AE = E; f(x::AE) = 6` は method を nominal placeholder `Struct("AE")` に
登録してしまい、`f(E())` が MethodError になっていた。原因は lowering の型
エイリアス判定ゲート (`is_likely_type_name`) が RHS 識別子について **builtin 名の
静的リストしか知らなかった**こと — ユーザ宣言型を指すエイリアスは alias table に
入らず、signature 注釈に展開すべき対象が存在しなかった。lowering の pre-scan が
プログラムの宣言型名 (`struct` / `mutable struct` / `abstract type` /
`primitive type`、module 内含む) を先に収集し、エイリアス連鎖 (`const BE = AE`) は
fixpoint まで反復登録するようにした。source order にも Base cache の有無にも
依存しない。残ケース: Base/stdlib 宣言型を指すエイリアス (#11113)、前方参照
エイリアス注釈の UndefVarError (#11114)。
### compile-context cache 復元の設計境界 (Issue #10438)

`COMPILE_CONTEXT_REHYDRATION.md` を accepted design とし、fresh/cache の correctness
invariant、persisted snapshot / structural projection / runtime-only の3分類、typed
event、versioned envelope、全 lane 共通 post-hit hook、fail-closed relocation、
fresh-vs-cache test matrix を定義した。既に landed 済みの #10462 Phase 0
`CompileContextSnapshot` と scoreboard を baseline にし、production 実装は #10462、
inference globals (#10333) と specialization policy (#10334) は現在 exact parity。
残る scoreboard mismatch は #10339、seeded `PROGRAM_CACHE` の context hydration は
#10335 が追跡する。

### where-binder scope routing の構造負債解消 (Issue #10436)

lowering 内で別々だった signature/alias 用と function-body 用の binder stack を
`type_binder_env` に統合し、完全な `TypeParam` frame と lexical nearest lookup を
全 surface で共有するようにした。semantic substitution は可能な限り
`CoreTypeVarId` で binder を照合し、未解決の lowering leaf だけ name fallback を
使う。既存の signature/body/closure/same-name/dependent-bound/reflection fixtures と
unit tests が受入行列を担い、audit は旧 stack の再導入を拒否する。
`CoreType::from_julia_name` / `rebind_where_binders` など表現 bridge の残件は
#10460 の structured-type migration として分離済み。


### runtime alias bound と current-input source order を整合 (Issues #11092, #11117)

runtime parametric-struct bound は visible alias を exact-qualified / unique-bare の順で
展開してから subtype 検査するよう統一した。signature definition probe は Base/cache と
current input の source coordinate を混ぜず、current-input 宣言同士にだけ ordinal/offset
比較を適用する。AbstractAlgebra residue-ring と #11025 の Base/nested annotation 行列を
full suite で固定。alias-bound validator の横断予防は #11142。


### regression: マクロ展開定義の struct 注釈が UndefVarError になっていた (Issue #11119)

#11025 の前方参照プローブが、`@testset` 内など**マクロ展開された定義**(span が
合成で `definition_order == 0`)を「順序 0 に定義された = 全ての型が前方参照」と
誤読し、struct 型注釈のたびに誤った `UndefVarError` を出していた(full suite の
5 chunk が RED)。前方参照と判定するのは**両方の序数が既知で、型の定義が厳密に後**
のときのみに限定し、それ以外は #11025 以前と同じくスキップする。fixture
`types/signature_forward_reference_11025.jl` に @testset 内定義の回帰ケースを追加
(julia/sjulia とも 3 testset green)。

### bug: sibling-module 同名 struct パラメータの dispatch ambiguous 化を修正 (Issue #11076)

`vm/dispatch.rs::type_matches` の `JuliaType::Struct` 汎用アームに owner-aware
guard を追加(`subset_julia_vm_types::types::struct_owners_compatible` を
`pub` 化して再利用)。sibling module の同名 generic struct を BARE 注釈で
method パラメータに使うケースの誤 ambiguous を解消。調査中に発見した
別バグ(non-generic struct や explicit parametric 注釈が `add_method` の
`core_signature` dedup で登録時に静かに collapse する)は Issue #11094 として
別途起票、Issue #11078 に折り込み。詳細は DONE.md 参照。
### main red: 注釈付き kwarg の call デフォルトが #undef になる (Issue #11124)

PR #11082(#11024)由来の 5 件目の main red(`packages::chunk_003`)。
`Value::Undef` は keyword の NOT-SUPPLIED センチネル(required マーカー兼
body-evaluated デフォルトの実体化指示、#5121)だが、デフォルト値付き位置引数が
生成する reduced-arity 転送スタブ(`g(y, x=2; ...)` → `g(y) = g(y, 2)`)には
プロローグが無く、生の `k` スロット(センチネル)を `CallWithKwargs` でそのまま
転送する。その結果センチネルが「供給された keyword」として callee に届き、
#11024 のアサーションが
`TypeError: ... expected Integer, got a value of type #undef` を送出していた。

発火条件は (デフォルト付き位置引数) × (型注釈付き kwarg) × (kwarg デフォルトが
呼び出し式) の 3 つすべて。`check_supplied_kwarg_types` がセンチネルをスキップ
するよう修正(= 「**供給された** keyword のみ検査」という関数自身の契約に復帰)。
required keyword の `UndefKeywordError` と #11024 の供給値アサーションは
いずれも従来どおり動作することを fixture
`kwargs/annotated_kwarg_call_default_positional_stub_11124.jl` で固定。

kwarg の**デフォルト値自体**が注釈に違反する場合に upstream(`MethodError`)と
乖離して黙って受理する件は、本件とは独立の既存ギャップとして Issue #11135。

### main red: #11025 の定義順プローブがネスト関数/abstract type で誤爆 (Issue #11111)

PR #11082(#11025 の #10396/#10582 フォワード参照検出)が main を red にした。
`definition_order`(0 = 未スタンプ、`span.rs` のコメント通り)を「最も早い順序」
と誤読していたため、(1) `@testset`/`if`/`for` などの block 内で定義される
LOCAL/nested 関数(`lower_function_defs_to_stmt` は `stamp_function_definitions`
を呼ばない)と (2) `abstract type` 宣言(`StructDef` の `stamp_struct_definition`
に相当する stamp 呼び出しが存在しない)が常に order 0 になり、既に定義済みの
`AbstractDict`/`Rational`/ユーザー struct を参照するネスト関数が前方参照と誤判定
され `UndefVarError` で probe-fail していた(`dispatch_agg_misc_9671` /
`rational_identity_ctor_float_dispatch_9363` / `types_agg_misc_10238` /
`type_inference_agg_struct_field_10238`)。

`emit_signature_definition_probes` の比較を、定義側 order が 0(未知)なら常に
スキップ、型側 order が `None`/`Some(0)`(未知)なら常にスキップに変更し、両者が
既知かつ型が真に早い場合のみ前方参照と結論する。ネスト関数内の真の前方参照検出は
`definition_order` を nested/abstract type にも配線する追加作業として#11025 拡張の
follow-up に委譲(fixture: `types/signature_definition_order_nested_11111.jl`、
julia 1.12.6/sjulia とも green)。

### dynamic fixture helper coverage の契約 self-test (Issue #11041)

`check_unregistered_fixtures.sh` に一時 fixture root/allowlist を注入できる test-only
override を追加し、実リポジトリの fixture tree を変更しない self-test を source-only
gate と CI に登録。computed path の未登録 helper は失敗、理由付き allowlist は成功、
literal include/evalfile は executable call のみ自動検出し、comment/string/prefixed
identifier は除外。理由なし・duplicate・covered・missing-file の stale row は失敗する
両側の契約を固定した(#11112)。registry row の削除も mutation test で拒否する。

### source-audit Python helper の 3.9 floor gate (Issue #11102)

`check_*.sh` / `audit_*.sh` が ambient `python3` で起動する外部 helper を自動発見し、
Python 3.9 grammar、stdlib/typing import availability、eager PEP 604 annotation、
isolated import-time execution を検査する source-only gate を追加。dynamic/option
path は fail-loud とし、CI interpreter を 3.9 に固定。新 helper の discovery、
Python 3.10 syntax、3.11 stdlib、#11093 と同型の annotation 欠落を負テストで固定した。

### signature 注釈の前方参照が定義時に受理されていた問題 (Issue #11025)

`f(x::S) = 1` を `struct S end` より前に書くと、upstream は定義時に
`UndefVarError` を出すのに対し sjulia は黙って受理していた。原因は
#10396/#10582 の定義時プローブが「コンパイラが型オブジェクトとして解決できる名前」
を無条件にスキップしていた点で、struct table はソース順に関係なくプログラム全体で
構築されるため、前方参照も「静的に解決可能」と判定されていた。

`SharedCompileContext.type_definition_orders`(`StructDef`/`AbstractTypeDef` の
`span.definition_order`、モジュール内も qualified/bare 両名で登録)を追加し、
プローブは**その型自身の定義がこの定義より前にある場合のみ**スキップする。
前方参照は従来どおり `LoadAny` でプローブされ、実行時に upstream と同じ
`UndefVarError` になる。builtin と type alias(実行時束縛を持たない)は従来どおり
短絡。fixture: `types/signature_forward_reference_11025.jl`(julia/sjulia とも green)。
なお `const` 型 alias を注釈に使うとディスパッチしない件は独立の既存バグで #11104。

### カスタム Unicode 演算子の中置呼び出し (Issue #11023)

`⊗(a, b) = a * b + 1; 1 ⊗ 2` が `UnsupportedOperator("⊗")` で lowering に失敗
していた。upstream Julia には演算子専用の名前空間は無く、**syntactic でない
演算子はすべて通常の関数名**であるため、定義したとおり中置でも呼べる。
`lower_binary_expr` / `lower_binary_expr_with_ctx` の両方で、`map_binary_op` が
知らない演算子は前置形 `⊗(1, 2)` と同一の `Expr::Call` に落とすようにし
(#10933 の `is_syntactic_operator` を単一の authority として共用)、両綴りが
同じメソッド identity とディスパッチ経路を共有する。`&&`/`::`/`=`/`<:` などの
syntactic 演算子は従来どおりエラー。fixture:
`operators/custom_unicode_operator_call_10933.jl` に中置・ディスパッチ・
Base Unicode 演算子のケースを追加(julia/sjulia とも 7/7)。
なお lexer が認識する Unicode 演算子の集合が upstream より狭い件は別途 #11083。

### keyword 引数の型注釈が lowering で捨てられていた問題 (Issues #11024, #11081)

`KwParam::new(name, default, None, span)` — 3 つの kwparam lowering パス全てが
型注釈を破棄していたため、宣言型が定義時検証(#10582 のプローブは
`KwParam.type_annotation` を読む)にも呼び出し時にも効いていなかった。

- **#11024**: 注釈を lowering で運ぶようにし、`KwParamInfo.declared_type`
  (serde-default、CACHE_VERSION bump)として bind 時まで伝播。供給された
  keyword 値は upstream と同じく**変換ではなくアサーション**として検査し、
  不一致なら `TypeError: in keyword argument x, expected Int64, got a value of
  type Float64` を catchable に送出する。抽象注釈(`x::Real`)は `isa` 判定なので
  `h(x = 2.5)` を通す upstream 挙動と一致。オプショナル kwarg のスロット型は
  従来どおり `Any`(`Real` に忠実な `ValueType` は無く、デフォルト値からの
  推論で固めると正当な値を弾くため)。
- **#11081**: 注釈付きでデフォルトの無い keyword(`f(; x::Int64)`)は、型式が
  デフォルト値として lower され optional 化していた(`f()` が `0` を返した)。
  required として lower し、`f()` は upstream 同様 `UndefKeywordError`。

fixture: `types/kwparam_type_annotation_11024.jl`(julia/sjulia ともに 25 passed)。

### orphaned-Rust audit の Python 3.9 compatibility 修復 (Issue #11093)

`check_orphaned_rs_files.py` の PEP 604 annotation が Python 3.9 で import-time
TypeError となり、audit negative self-test が invariant 到達前に停止していた。
annotation evaluation を postpone し、macOS の Python 3.9 lane でも seeded orphan
検出まで実行できるようにした。

### constructor identity authority を単一化 (Issue #11043)

`MethodSig` の投影済み value signature と独立した inner-constructor boolean が
再導入されないよう、serialized `MethodTable::constructor_self_families` を唯一の
authority とする source audit を追加。constructor selection、type-stability
analyzer の canonical signature 構築、Base-cache round trip を同じ gate で監査し、
owner/alias/binder/cache identity matrix を実装 checklist に固定した。

### guarded certification を GitHub merge boundary で強制 (Issue #11087)

PR #11077 は exact-base full suite が3回 green 後に main 進行で拒否されたにも
かかわらず、4回目の gate 中に外部から ready/merge された。local script は
authorization boundary ではないため、`premerge_gate.sh --pr` が exact head に
`sjulia/guarded-certification` status を publish/revoke し、GitHub の active
`protect main` ruleset が strict up-to-date required status として強制する構成へ
移行。`scripts/github_merge_ruleset.sh` で live check/apply と offline negative
self-test を行う。

### current main の typed-loop Clippy 回帰を修復 (Issue #11074)

`StoreSlotArray` の identity-rebind provenance 判定を instruction match の guard
へ移し、cross-slot store は従来どおり generic VM fallback に reject する。挙動を
変えずに `cargo clippy --all-targets -- -D warnings` の
`collapsible_match` を解消し、#11056 の guarded certification を再開可能にした。

### draft PR の ready 化を guarded certification に集約 (Issue #11056)

agent-created implementation PR は review と必須ローカルゲートが完了する
まで draft を維持し、implementation agent は ready 化・merge を行わない。
`scripts/premerge_gate.sh --pr <N>` は exact current `origin/main`、clean HEAD、
PR の draft/base/head をゲート前後に検証し、既定 source-only audits + clippy
を外せないまま追加ゲートを実行する。成功後だけ ready 化して certified SHA
固定の regular merge を行い、ready 後の base 進行/API/merge 失敗では EXIT
trap が draft に戻す。隔離 Git repo + fake `gh` の負の自己テスト 10 ケースで、
未認証経路が ready/merge に到達しないことを固定した。
### techdebt(#10459): Phase 2b investigation — StructId 導入せず #11021 を修正、続きは #11078 (Issue #10989, part of #10459)

`docs/vm/SEMANTIC_ID_MIGRATION.md`の Phase 2b(`StructId` + module-owned
struct identity, 351 サイト)を調査。Phase 2a の `macro_bindings` のような
「1 PR で re-key 完結できる自己完結テーブル」が struct ドメインには存在せ
ず、`struct_table`/`base_struct_table` の re-key は 61+20 サイト、
`StructInfo`/`StructDefInfo` の構築サイトは ~55、`CoreType::Struct` の
construction-time module-stripping(`inference_core::type_core::
from_julia_name_uncached` の `base_type_name` 呼び出し)まで含めると波及
範囲がさらに広い ── どれも「誰も読まない `id` フィールドだけの `StructId`」
を作ると解決するものが何も無く、issue 本文が明示的に禁止する「並行する
`StructId` パスを追加するだけ」になってしまうため、`StructId` 型は今回導
入しなかった(advisor レビューで確認)。

代わりに、調査中に発見した本物のバグ Issue #11021(sibling module の同名
struct が `==`/`===` で collapse する)を `StructId` 抜きで修正 —
module-prefix stripping を「片側だけ bare なら安全、両側 qualified なら
owner 一致必須」という非対称な形に直した。詳細と upstream 照合済み
identity matrix は DONE.md 参照。`check_name_based_lookup.sh` の
`structinfo_name_maps_compile`(61)/`struct_table_bare_gets_compile`(20)
baseline はどちらのテーブルも retire していないため変更なし(意図的 —
何も retire していないのに baseline を下げるのは ratchet を欺くことに
なる)。

継続 Issue #11078(`struct_table` re-key・construction-site 移行・
`CoreType` module-aware 化・Issue #11076 の推奨着地順を含む)と、調査中に
見つかった副次バグ Issue #11076(sibling module 同名 struct パラメータの
メソッド dispatch が誤って ambiguous になる)を新規起票。

### lezer-julia を第二の文法クロスチェック参照として文書化 (Issue #10985)

`docs/vm/PARSER_GRAMMAR_REFERENCES.md` を新設。`julia/src/julia-parser.scm` を
正 (normative)、JuliaPluto/lezer-julia の `src/julia.grammar` を副
(structural cross-check) とする authority 順序を明文化し、Milestone 73 の
パーサ修正 (#10932/#10940 syntactic operator role, #10937/#10945 scoped
declarations, #10915 `::`, #10644 double bounds, #10951 precedence matrix)
と lezer-julia の該当ルールの対応表、および今後パーサを拡張する際の
参照手順 (lezer で仮説 → upstream `julia` で確証) を記録。

### Base マクロの quote-local hygiene を復活 — module 所有と gensym rename の分離 (Issue #10977)

`hygiene.is_some()`(#9619 で全 Base マクロに付与)が「quote-local gensym
rename も全部スキップ」を意味していたため、`@elapsed` 自身の `t0` などが
caller 変数を clobber していた問題を修正。`maybe_apply_quote_hygiene` は
module 所有マクロに対して新設の `apply_module_macro_quote_local_hygiene` を
適用: rename 対象名を展開後の値ではなく**マクロ本体の quote constructor から
静的に収集**(static エンジンの Pass-1 collector を
`collect_quote_constructor_introduced_names` として共用)するため、
`$ex` で splice された caller 名(`@time grid = ...` の `grid`、#9619)は
構造的に rename 集合に入らない。`global` 宣言名と、展開が `esc(...)` 内から
も参照する名前(Plots `@animate`/`@gif` の `_anim` ブリッジ、#6355 機構)は
除外して従来動作を維持。fixture:
`macros_base_macro_internal_local_hygiene_10977.jl`(@time/@elapsed/@timev/
@timed/@showtime/@allocated/@allocations/@show/@lock + #9619 回帰 + MWE、
julia parity 一致)。詳細は LOWERING.md の "Resolved (Issue #10977)" 段落。

### static Pass-1 の Assign ロールが destructuring target を再帰展開 (Issue #10980)

`collect_introduced_vars` の `Assign` アームが bare `Symbol` target しか
登録しなかった非対称を解消: 新設 `register_assignment_target_names` が
dynamic path の `collect_assignment_target_names` と同じ再帰で
`Tuple`/`TypeAssert` target(`(a, b) = f()`, `x::Int = 1`)内の全 bare 名を
登録。現状どの shipped stdlib/Base マクロの quote body にも destructuring
assignment target は存在しない(到達不能)ため、`handlers.rs` の既存
unit-test module に直接テスト 5 本(tuple / nested tuple / type-assert /
esc 除外 / 収集集合の完全一致)を追加してカバー。この collector は #10977 の
dynamic 側 rename 集合の供給源にもなったため、両エンジンの Assign ロールが
完全対称になった。
### syntactic operator の role inventory と bare `&&`/`||` の拒否 (Issues #10932 / #10940)

`Token::is_syntactic_operator()`(upstream `syntactic-operators` の operator-lexed
メンバー = `->`, `&&`, `||`, `.&&`, `.||`)を単一の authority として導入し、
`is_operator_identifier` と `reject_invalid_operator_identifier` を経由する全
unquoted-name shortcut がこれを共有する。bare/paren/call-arg/method-def/const/
import/export の各形は upstream と同じ `invalid identifier` / `expected identifier`
と exact span で拒否され、infix 参加・quoted forms(`:(&&)`, `Base.:(&&)`)・
qualified quoted import(`import Base.:(&&)`, `import Base.:(->)` — #10939 での
退行を修正)は green。checked-in role inventory は
`corpus_operators.rs::test_syntactic_operator_role_inventory_issue_10940`、
token contract(mutation で RED)は
`token/tests.rs::test_syntactic_operator_role_split_is_exhaustive_issue_10940`。

### `::::` を premature-end-of-input として拒否 (Issue #10915)

`::` は upstream の syntactic-*unary* operator: operator value にはならないが
invalid identifier としても拒否されず、unary typed 文法が再帰的に消費する。
`is_operator_identifier` から `DoubleColon` を除外し、`::::` は末尾 `::` の型式
欠落として `UnexpectedEof`(incomplete input, upstream と同じ end-of-input span)
になる。`:::: Int` / `::(::Int)` は upstream 同様 nested UnaryTypedExpression と
してパースされる。malformed-input corpus の `::::` と `->` は "reports a typed
error" に強化済み。

### struct/abstract の double-bounded 型パラメータ `Lo<:T<:Hi` (Issue #10644)

`struct DB{Int8<:T<:Signed}`(と mirrored `Signed>:T>:Int8` chain、where 節の
`>:` chain も)が upstream の comparison-chain 文法どおりにパースされ、
`SubtypeConstraint [name, upper, lower]`(#5051 の where 形と同一 shape)として
lowering が両 bound を `TypeParam::with_both_bounds` に束縛。範囲外の literal
type application(`DB{Int16}`: lower violation / `DB{Integer}`: upper violation)
は upstream と同じ TypeError。fixture:
`types/struct_double_bounded_param_10644.jl`(julia/sjulia parity 11 passed)。
corpus allowlist から `julia/test/errorshow.jl` が ratchet で除去(本修正で
clean parse 化。stale だった `replcompletions.jl` / `ccall.jl` も同時に除去)。

### identifier continuation × operator boundary の table-driven lexer 網羅 (Issue #10848)

lexer の identifier-continuation 文字クラスごとに代表 1 文字を `!=`/`!==`/
`.!=`/`.!==`/dotted unary `.!` の境界と組にした table-driven テスト
(`lexer.rs::test_identifier_continuation_operator_boundary_table_issue_10848`、
全行 upstream 1.12.6 で検証、`f!!=g` の single-`!` rewind も固定)を追加。
CHECKLISTS.md に「identifier continuation 拡張は paired operator-boundary lexer
テスト必須」のチェックリスト節を追加。
### techdebt(#10459): Phase 2a — ModuleId foundation (Issue #10988, part of #10459)

Issue #10459(bare-name identity table 撤廃 epic)の Phase 0 マイグレーション
プラン(`docs/vm/SEMANTIC_ID_MIGRATION.md`)が定義した Phase 2a を実装。
`ModuleId(u32)` + `ModuleInternTable` を新設(`subset_julia_vm_bytecode::
module_intern`)し、モジュール登録順(`Program` モジュール木の決定的な
深さ優先走査)で ID を割り当てる。Phase 2b(`StructId`)/Phase 3
(`FunctionId`)が `ModuleId` を型として埋め込む前提の基盤。

cache-relocation パターンを2種類設計し `docs/vm/CACHE_ARCHITECTURE.md` に
文書化(Pattern A: `RuntimeCompileContext::module_registry` のように
`#[serde(skip)]` で毎回 IR から再導出するテーブル向け、Pattern B:
`CompiledProgram::macro_bindings`(`String`→`ModuleId` 移行)+ 新設の
`CompiledProgram::module_registry` のように実際に bincode で永続化される
テーブル向け、`CACHE_VERSION` 136→137)。同名別モジュールの識別子が
fresh compile と cache restore の双方で一致することをテストで固定。

Issue 本文が挙げた 12 の module/global テーブル(`module_functions` 等)は
実査の結果いずれも「それ自体が bincode シリアライズされる struct のフィー
ルドではない」(`CorePipeline`/`CoreCompiler`/`SharedCompileContext` の一時
状態であり、唯一 `inference_global_types` だけが `#[serde(skip)]` の
`RuntimeCompileContext` へ clone されるが、それすらワイヤは越えない)と判明
したため今回は未移行 — 別 issue へ委譲。詳細は `docs/vm/DONE.md` および
`docs/vm/SEMANTIC_ID_MIGRATION.md`「Phase 2a status」参照。
### closure body の builtin 同名 `where` binder を lexical に保持 (Issue #11031)

#10934 の active type-parameter context を assigned arrow と full-/short-form
nested function の body まで継承。direct arrow は nested IR を使い、closure
environment は runtime type binding も snapshot するため、enclosing method が
`Float64 = Int64` を束縛した場合の `Vector{Float64}` は upstream 同様
`Vector{Int64}` になる。unit 3件 + upstream parity fixture 6件で固定。

### bug: constructor last-definition-wins を独立 lowering fragment 間で保持 (Issue #11028)

PR #11035 を regular merge。bare inner/ordinary outer の同一 `Type{Foo}`
signature は serialized `definition_order` で評価順を比較し、prelude/Base/
include/REPL/package module の独立 lowering fragment は累積 rebase してから
統合する。same-file/include/REPL/package-cache/method-table 回帰と upstream
fixture を追加し、guarded full suite 5,351/5,351 green。今後の direct append
再発防止は #11036、汎用 cache-context replay は #10462 で継続する。

### techdebt(#10459) Phase 1 完了 — TypeVar projection identity の表示文字列キー残余を撤去 (Issue #10987)

`runtime_typevar_projection_identities` のキーから描画済み
`String`/`Option<String>` 成分を撤去し、完全構造化の
`TypeVarProjectionKey { owner, binder_depth, declared_lower,
declared_upper }` に置換。宣言時 bounds はパース済み `JuliaType` として
キーに残す(body 由来の owner は binder bounds を符号化しないため、
落とすと同一 body・別 bounds のラッパーが衝突 — targeted fixture
`where_binder_shadow_scope_10100.jl` で検出した実回帰により bounds-less
`(CoreType, usize)` 案は棄却)。codex 敵対的レビューで2件追加修正:
bounds キーを `JuliaType` 化(CoreType はモジュール修飾を剥がす)、
owner 正規化のネスト `UnionAll` binder 保存(bound 内のみに出現する
外側 binder の識別)。interval 分割のブラケット深度対応化で既存バグ
#11020 も修正。既存の同名構造体クロスモジュール型等価ギャップは
#11021 起票(#10989 スコープ)。表示名は値側メタデータへ降格。#10459
Phase 1 は残余ゼロで完了、`EXTRA_ANCHOR_ROWS` 空化(インベントリ
873→872、typevar 15→14)。構造化 UnionAll body で owner 自体が bounds
を持てるようにするのは #10460 のスコープ。
### literal `T{...}` 型適用の残余ドリフト3件を解消 (Issues #10654, #10643, #10642)

#10556 parity matrix の literal `{...}` 経路の残余レグを解消。#10654:
非パラメトリック builtin base への literal 適用(`Int64{Float64}` 等)を
compile 時に `PushDataType(base)+args+ApplyTypeDynamic` へ差し替え、
`Core.apply_type` と同じ runtime validator が upstream 形 TypeError を送出
(variant ベースの許可判定なので既存 parametric family の static fast path は
不変)。#10643: applied-target type alias(`w = Plain{Int64}`)への追加適用
`w{Float64}` が static 展開で外側引数を DROP していたのを、alias base の
dynamic 化+展開ターゲット全体の base_expr 保持で runtime append に統一。
#10642(lower bound `S>:Int32` の方向)は main 上で解消済みと確認し
regression fixture を追加。fixtures 3本(37 assertion)全て
`fixture_julia_parity.sh` green。詳細は DONE.md 同日エントリ。
### milestone-73 misc: #10933 / #10582 / #10630 (fix/milestone73-misc)

- **#10933**: カスタム Unicode 演算子(`⊗` 等)が定義位置に加えて呼び出し
  位置(`⊗(1)`)でも通常関数として解決されるように
  `resolve_call_target` を修正(構文形式のみ拒否する
  `is_syntactic_operator` を新設)。infix 形 `1 ⊗ 2` は別経路の残ギャップ
  → Issue #11023。
- **#10582**: 未定義名の引数型注釈 `f(x::SomeUndefName) = 1` を定義実行時
  UndefVarError に(#10396 のプローブを
  `emit_signature_definition_probes` に一般化)。残余ギャップ: kwparam
  注釈は lowering で欠落(→ Issue #11024)、後方定義型への前方参照は受理
  されたまま(→ Issue #11025)。
- **#10630**: マクロ statement/value アダプタ契約の再発防止(ユニット
  テスト+統合マトリクス fixture/8191 テスト+LOWERING.md 文書化)。

### techdebt(#10984) CoreCompiler の for/foreach 誘導変数シャドーイング (Issue #10984, fixes #10903)

`CoreCompiler` のローカル変数追跡がフラット `HashMap<String, ValueType>`
でレキシカルスコープを持たなかった問題(cluster A 第3の原因、#10925/
#10936/#10965 の姉妹issue)を、完全なスコープスタック再設計ではなく
既存 `Expr::LetBlock` シャドーイング機構を一般化した
`CoreCompiler::shadow_local_enter`/`shadow_local_exit` で解決。
`Stmt::For`/`ForEach`/`ForEachTuple`/単変数コンプリヘンション/タプル分割
コンプリヘンション(`[expr for (a,b) in iter]`、advisor レビューで発見した
独立した衝突箇所)の誘導変数が衝突するケースのみ値と型状態を退避・復元
(無衝突時は完全 no-op)。
静的型推論側(`compile/inference.rs` の事前スキャン、
`compile/abstract_interp/engine/mod.rs` の戻り値型推論エンジン)も同様に
修正しないと `return` 文が誤った型の Return 命令を選び実行時クラッシュ
していたため、あわせて修正。マージ前レビューで初版の自己回帰
(兄弟空ループの `initialized_locals` 残滓を本物の外側値と誤認 →
`channels.jl` `_wake_all_channel_waiters` で `UndefVarError`)を発見し、
5構造の対称スナップショット/復元 `restore_shadow_bookkeeping` へ修正。
さらに codex 敵対的レビューで、条件付き初期化ローカル(初版導入の
クラッシュ回帰)とネスト同名 const-step カウンタ破壊(main 由来の既存
乖離)の2件を検出 → save/restore を `IsDefined` ランタイムガードへ強化し
両方修正。詳細・fixture 結果・残課題(#10999/#11000/#11001)は
`docs/vm/DONE.md` の同日エントリを参照。
### techdebt: constructor self-family identity を MethodTable の serialized carrier へ (Issues #10962, #10974)

#10959/#10967 の修正は inner/outer constructor 識別を
`SharedCompileContext` の 2 つの transient `HashSet<usize>` に持たせていた
(cached Base tables はこの集合を再構築しないため `has_where_params()` への
fallback が残っていた)。この tech-debt を `subset_julia_vm_bytecode::method_table`
の `MethodTable` 自身が持つ serialized・deterministic な
`BTreeMap<usize, ConstructorSelfFamily>` (`BareInner` / `ExplicitParametricInner`)
へ移設。`add_inner_constructor_method`/`add_method` は置換されたメソッド行の
古い origin エントリを同一トランザクションで除去し、`clone_with_methods_for_compile`
は filtered method 集合に残らない global index の origin を破棄する。
`constructors.rs`/`dispatch.rs` の3箇所の呼び出し元は全て
`table.is_explicit_parametric_inner_constructor(...)` /
`table.has_explicit_parametric_inner_constructors()` /
`table.dispatch_among_explicit_parametric_inner_constructors(...)` を
参照するのみとなり、`has_where_params() == inner` ヒューリスティックは compile
crate から完全に除去(#10962 DoD 完了)。`precompile.rs` の `CACHE_VERSION` を
131→132 に bump しスキーマ fingerprint を更新。

投資範囲の判断: 調査の過程で #10969 (`Rational{T}(x, x+x)` が cached Base で
非正規化 `2//4` を返す) は cache identity loss が原因ではないことを実証
(uncached でも同一症状を再現) — 真因は `Rational{T}(::Integer, ::Integer)
where {T<:Integer}` が別の method table (`"Rational{T}"`) に登録され、
dynamic forwarding path (`dynamic_parametric_inner_constructor_method`) が
struct 本体名の table しか探索しない構造的ギャップ (#10968/#10971 と同系統)。
このため #10969 は close せず open のまま維持し、当該関数の user-source
gate (W-67, `docs/vm/WORKAROUNDS.md`) も除去せず(除去すると Base 自身の
StepRange 構築が同じ dynamic path を通り native carrier 破損を再現)。詳細は
`docs/vm/DONE.md` の同日エントリを参照。

### bug: package loader の永続キャッシュが struct/inner-constructor 形状変化を検知できず stale 再利用 (Issue #11004)

#10962 の DoD 4 (`has_where_params()` フォールバック全除去) を実装した後、
AoT gate (`bash scripts/test_aot.sh`) の `packages::chunk_003` →
`packages_data_structures_binary_max_heap_8509` が
`Struct constructor expects 1 arguments, got 0` で赤化。標準の
`julia`・sjulia の standalone 再現スクリプトはどちらも正しく動作するが、
実際の `using DataStructures; BinaryMaxHeap{Int64}()` パスのみ再現し、
AoT/非AoT 双方で発生、`SUBSET_JULIA_VM_DISABLE_CACHE` 系(Base cache)を
無効化しても再現 — Base cache とは別の第三のキャッシュ層の疑いから
`subset_julia_vm/src/loader.rs` の永続 package cache
(`$TMPDIR/subset_julia_vm_cache/*.ji.json`) を精査。実機に残っていた
stale なキャッシュ JSON を直接読むと `BinaryMaxHeap` の
`inner_constructors` に `is_explicit_parametric` キー自体が存在せず
(`#[serde(default)]` で `false` に暗黙補完)、当該ファイルを削除すると
即座に正しい結果を返すことを確認 — cache-identity loss ではなく
「古いキャッシュが新しいフィールドの意味を反映していない」という
#7921 と同系統の schema drift。

根本原因: `module_schema_fingerprint()` が検証用に組む probe `Module` の
`structs` フィールドが常に空 (`Vec::new()`) で、`StructDef`/
`InnerConstructor` の形状・値変化に一切反応しない。`CACHE_VERSION`
(17→18) を bump してこの class の stale entry を強制再構築すると同時に、
probe に `is_explicit_parametric: true` を持つ代表 `InnerConstructor` を
含む代表 `StructDef` を追加 — 既存の `type_aliases` probe entry と同じ
パターンで、以後 `StructDef`/`InnerConstructor` の形状変化がすべて
fingerprint に反映されるようにした。`test_schema_fingerprint_covers_
struct_inner_constructor_shape_10962`(旧 probe 形状とのハッシュ差分を
assert)を追加。`packages::chunk_003` を含む `packages::` 全 chunk
green を確認。

pre-existing 性の実証: merge-base コミットを隔離 worktree
(独立 `CARGO_TARGET_DIR`/`SUBSETJULIA_CACHE_DIR`) にチェックアウトし、
実機で見つかった stale cache と同一の破損(`is_explicit_parametric` 欠落)
を持つ cache entry を手動で複製して読ませたところ、merge-base では
`has_where_params()` フォールバックが暗黙にカバーして正しく動作した一方、
このブランチ(フォールバック除去後)では同じ壊れた entry で再現した —
このバグ自体は #10962 以前から存在する潜在バグであり、#10962 の DoD 4 が
それを可視化しただけであることを確認。詳細は `docs/vm/DONE.md` の
同日エントリを参照。
### `global function` / `local function` 長形式の残課題解消 (Issue #10937)

PR #10944 の deferred 残課題のうち 2 件を解消。(1) 関数本体内の `global`
メソッド定義(長形式・短形式とも)は upstream と同じ
`syntax: Global method definition around line N needs to be placed at the top
level, or use "eval".` の typed lowering error になる(`lambda_ctx` では
トップレベル `let` と関数本体を区別できないため、`lower_function_impl` /
`lower_short_body_expr` に CST pre-scan
`reject_global_method_definitions_in_body` を追加。quote / 未展開 macro 引数は
スキップ = upstream の `@eval` 逃げ道を保存)。(2) `global macro` は upstream
1.12.6 で lowering error(`invalid syntax in "global" declaration`)であることを
実測確認し、同じ typed error を出す(以前は UnsupportedExpression("macro_definition"))。
残る deferred は let-local capture(`let counter = 0; global function f();
counter += 1; end; end`)のみで、フォローアップ Issue に切り出し。

### `const global` / `global const` の束縛が消える問題 (Issue #10943)

`lower_const_statement` が GlobalStatement 子を `_ => {}` で無視して no-op に
していたのが根本原因。GlobalStatement 子は `lower_global_statement` に委譲して
Global marker + 代入を得た上で `wrap_const_assignment_deep` で const 化。
構造的 type alias(`const global A = Vector{Int}`)は `try_extract_type_alias`
を scoped wrapper に降下させて従来どおり alias 登録+no-op。`const local` /
`local const` は upstream 実測どおり
`syntax: expected assignment after "const"` の typed error(以前は silent drop
→ UndefVarError)。fixture: `scope/scoped_const_global_10943.jl`。

### `global`/`local` 後の全式パース (Issue #10945)

upstream の `(global local)` arm は `parse-eq` 経由で完全な式をパースする。
`parse_var_declaration_item` に予約語 construct(module/baremodule/struct/
mutable/abstract/primitive/if/for/while/try/begin/let/quote/return/break/
continue/using/import/export/global/local)の delegation を追加し、
`GlobalDeclaration(構文ノード)` の upstream 形状 CST を生成。incomplete 分類は
construct パーサの UnexpectedEof がそのまま伝播(`global module` = incomplete)。
lowering 側は宣言可能な名前形状以外を upstream 実測の
`syntax: invalid syntax in "global"/"local" declaration` typed error で拒否し、
silent drop を全廃(`ensure_scoped_declaration_name_children`)。付随修正:
`global f(x) = 2x` 短形式メソッド定義の委譲 (Issue #11008)、
`global x, y = 1, 2` の comma-list `= rhs` 分配 (Issue #11009)、`local x += 1`
の compound assignment、演算子 tail(`global c + 1`)の statement 分裂防止。
corpus ratchet: `replcompletions.jl` / `ccall.jl` が green 化し allowlist を
2 件縮小。fixture: `scope/scoped_declaration_forms_10945.jl`。

### 予防: scoped 宣言の差分文法マトリクス (Issue #10951)

`corpus_statements.rs` に table-driven の差分文法マトリクスを追加:
delegated construct 行列(25 形)、incomplete prefix 行列(unprefixed
construct との分類一致を等式で強制)、式 item の statement 非分裂、comma 分配
形状、RHS precedence tier(pair/arrow/ternary/nested assignment/chained pair)、
modifier 順×改行位置(11 形)。mutation contract(6 authority それぞれを
バイパスするとどの行が赤くなるか)をコメントで明文化。lowering 側は
`regression_misc_tests.rs` の `scoped_declaration_lowering_10943_10945` mod が
全 normalized CST shape の typed error / 正常 lowering を固定。

### static quote Pass-2 codegen (#10916) は DEFER — 到達可能性を実証し文書化 (Issue #10916)

Function/Where/Comprehension/Generator の static Pass-2 codegen は実装せず
deferral(Issue open のまま)。static エンジンの唯一の入口は stdlib マクロの
statement position 呼び出しのみ(全 Base マクロは `ef83266a2e` 2026-06-25 以降
dynamic; expression position の stdlib マクロ・user 定義・bundled も dynamic)
と実証され、fixture では新 codegen を行使不能 = #10916 本文の「real consumer
なしに speculative codegen を追加しない」に抵触するため。LOWERING.md の stale
記述(#10627 "Correction" 段落が旧 routing を記載)を修正し、"Static Pass-2
reachability" 節に routing 表・再開基準・static Pass-2 を dynamic へ統合して
gap クラスごと削除する構造的代替案を記録。production Rust 変更なし。詳細は
DONE.md 同日エントリと Issue #10916 のコメント参照。

### techdebt(#10459) Phase 0: semantic-ID inventory + migration分解 (Issue #10459)

owner-scoped semantic ID 導入 epic (#10459) の Phase 0(棚卸し+分解)のみ納品。
report-only generator `scripts/semantic_id_inventory.sh` が production コード中の
裸名 identity テーブルを機械的に分類(identity domain × layer × migration難度)し、
`docs/vm/SEMANTIC_ID_INVENTORY.tsv` へ committed snapshot。873サイト中 6-domain
合計607件(struct 351 が最大、function 94、module 54、global 63、method-sig 30、
typevar 15 は #10049/#10261 でほぼ完了済の残余のみ)。`check_name_based_lookup.sh`
6パターンとの照合は完全一致。依存順マイグレーション計画は
`docs/vm/SEMANTIC_ID_MIGRATION.md`(新規)。詳細と sub-issue 一覧は同ドキュメント・
`docs/vm/DONE.md` の同日エントリを参照。production Rust 変更なし。
### techdebt(#10869) Phase 3: enforcement endpoint (Issue #10908)

panic debt 退治 epic (#10869) の最終フェーズ。`PANIC_FREE_DENY_MODULES.tsv` を
production front door 全体(parser/lowering/compile の crate-subtree root
カスケード含む)へ拡張(125行)、`user-input-reachable` バケット専用の新規
production-lane gate(`check_panic_free_production_baseline.sh` +
`PANIC_FREE_PRODUCTION_BASELINE.tsv`、target 0 modulo issue 紐付き許可リスト)、
FFI/CLI/REPL の境界プロセス生存テストを追加。詳細は
`docs/vm/PANIC_DEBT_RETIREMENT.md`「Enforcement endpoint — Phase 3」・
`docs/vm/DONE.md` の同日エントリを参照。
### parametric constructor の inner/outer 分離 (Issues #10959 / #10967)

同じ value signature と `where` clause を持つ constructor を暗黙 self family
(`Type{Foo}` / `Type{Foo{T}}`) で分離する。explicit `Foo{T}(...)` は explicit-inner subset
内で dispatch し、裸呼び出しからはその subset を除く。outer body の statically-bound
runtime `T` は caller の concrete binding を保って inner body を実行し、任意の runtime
type expression は従来の dynamic path に残す。詳細と 11-assertion parity fixture は DONE.md。
### techdebt: stdlib/dynamic macro-expansion engine の Pass-1 hygiene 判定を単一 registry へ収束 (Issue #10627)

stdlib/Base macro の静的 quote 展開(`lowering/expr/quote/`)と VM 実行時の
user/package macro 展開(`macro_runtime.rs`)が、どの `ExprHead` がローカル
束縛を導入するかを独立した2つの手書き match で判定していた drift を解消。
`expr_heads.rs`(既存の4-path 共有 registry)へ `quote_binding_role(head) ->
QuoteBindingRole`(`LocalDecl`/`Assign`/`TryCatchVar`/`FunctionName`/`None`)
を追加し、`collect_introduced_vars`(static)と `collect_quote_local_names`
(dynamic)の両方がこの1関数を通して判定するよう書き換え(木の走査・登録方法
自体は #10626 の結論どおり両エンジンで別実装のまま維持)。あわせて
`static_quote_top_level: bool` 列を registry に追加し、static Pass-2 の外側
dispatch カバレッジも `macro_return_to_stmt`/`macro_return_to_expr` と同じ
`debug_assert_eq!` drift 検知パターンで保護。既知の未対応 head
(`Function`/`Where`/`Comprehension`/`Generator`, #10916)には
`tracked_static_quote_gap_issue` でエラーメッセージに Issue 番号を明記。

差分テスト: `Test.@test` の実 quote body(Block/Local×4/Try-Catch/If/ElseIf/
Call/String をすべて含む)を model にした user-defined macro `@my_test` を
`quote_engine_convergence_test_10627.jl` で比較し、pass/fail/error 分類と
hygiene 非漏洩が両エンジンで一致することを確認。作業中に発見した既存バグ2件
(`#10977`: Base macro の quote-local 変数が hygiene rename されず caller
scope を汚染、`#10627` 差分テスト構築中に発見; `#10978`: トップレベル
`@testset` の失敗が `try`/`catch` で捕捉できない)と、tuple/type-assert
destructuring assignment target が static Pass-1 で未対応な既知ギャップ
(`#10980`, 現状到達不能)を別 Issue として起票。詳細は LOWERING.md
「Converging the Two Engines' Pass-1 Decision Table」節、DONE.md 参照。

### techdebt(#10869) Phase 2: runtime/optimization paths zero-deny (Issue #10907)

`vm/specialize`(4 files)・`vm/type_ops`(6 files)・`vm/formatting`(4
files)・`register_vm.rs` の計15ファイルへ `#![deny(clippy::unwrap_used)]` /
`#![deny(clippy::expect_used)]` を付与し `docs/vm/PANIC_FREE_DENY_MODULES.tsv`
へ登録した(Phase 0 の予測どおり実質ヒット0、`#[cfg(test)]` ブロックのみ)。
`vm/executable.rs` の実コード側 `unwrap()` 1件(guarded invariant)は
`?` 演算子へ変換。`src/aot` の実22件のうち16件を `AotResult`/`ok_or_else` へ
変換(2件は `HashMap::entry()` ベースの `ensure_enqueued` や `if let` 束縛の
一本化で panic 経路そのものを型で排除)、残り6件(`aot_codegen/expressions.rs`
の生成コードテンプレート文字列リテラル)は別種の是正として Issue #10955 へ
分離。`subset_julia_vm_runtime::error::aot_throw` の `panic!` は Issue
#5658/#7018 で既にレビュー済みの意図的な設計(AoT コンパイル済みネイティブ
バイナリの uncaught throw 境界)と確認し、変換対象外として明記した。
`cargo clippy --all-targets -- -D warnings` / `--features aot --all-targets`
green、cranelift 機能組み合わせは事前・事後で同一の無関係な既存 warning のみ
(regression なし)。`bash scripts/test_aot.sh` green。詳細は
`docs/vm/PANIC_DEBT_RETIREMENT.md` の Phase 2 節、DONE.md 参照。

### Prevention #10983 の敵対的検証と分解 (Issue #10983)

Parser/Lowering/Syntax milestone の5根本原因主張を、各 cluster の元 bug/PR
と実装コードに照らして検証した。参照先だった
`memory/project/parser-lowering-syntax-root-causes-2026-07.md` は未コミット
(dangling)と判明。5 cluster 中 4 つ(A の macro hygiene/`LambdaContext`
ルーティング、B、C、D)は既存 issue/epic(#10925、#10936+#10965、#10940、
#10951、#10627+#10916+#10980、#10436)で既に owned — #10983 自身が
cluster B→#10627、cluster E→#10436 の cross-link を欠いていた点を指摘。
唯一 unowned だったのは `CoreCompiler.locals: HashMap<String, ValueType>`
がレキシカルスコープスタックを持たない点(for/comprehension induction
variable が同名 outer local を上書きする #10903 の根本原因)で、新規
Issue #10984 を起票した。詳細は DONE.md、`docs/vm/PREVENTION_MAP.md`。

### value parameter と builtin type の parametric-base shadowing (Issue #10948)

function body の `T{...}` base 判定は、global builtin/alias より先に active lexical
value parameter を参照する。short/full、positional/keyword、assigned/inline arrow、
destructuring を同じ frame authority で扱い、`f(Vector::Type)=Vector{Int64}` に
`Set` を渡すと upstream 同様 `Set{Int64}` を構築する。#10934 の builtin 同名
where-binder と組み合わせた元 MWE も固定済み。詳細と mutation proof は DONE.md。
### Base `SubString` 型オブジェクトの識別復旧 (Issue #10953)

bare `SubString`、`Base.SubString`、`isdefined(Base, :SubString)` が同じ builtin
型 binding を解決するよう、type parser/compiler/reflection の3 authority を同期した。
Regex `split` の返り値は upstream と同じ
`Vector{SubString{String}}` として直接比較できる。authority の正準リスト化は
Issue #10954。詳細は DONE.md 参照。
### explicit parametric constructor self の残存 parity を修正 (Issue #11019)

#10973 の serialized constructor-origin carrier に完全な self pattern と owner/bound 照合を重ね、
cross-binder correlation、組み込み・lexical/qualified-same-leaf user alias、self binder ごとの qualified-owner と lexical-spelling bound identity、parameterized/lower
bound、module-local self argument、runtime miss、short default、同名 module owner の定義順依存を解消した。
Base/package cache version は 139/19。runtime-`Any` overload dispatch は
W-71 / #10971 で引き続き追跡する。詳細は DONE.md。
module body top-level call の bare type-argument scope は別 bug #11034、再発防止は #11043。
Base 完全 self metadata の再帰は W-72 / #11062。

### global/local と const の modifier 正規化 (Issue #10938)

global/local declaration の共通 parser が改行をまたぐ後置 `const` を消費し、
`global const` / `const global` と `local const` / `const local` を同一の
`Const(Scope(...))` CST へ正規化する。scoped const の assignment 必須化、tuple shape、
重複 modifier、node span も固定した。name path は Identifier/operator に限定し、
literal expression・予約語・punctuation・EOF を unchecked Identifier として読む経路を
declaration 境界で閉じた。operator-keyword identifier は維持する。scoped
const の RHS は pair/arrow/nested assignment まで right-associative に保持する
(Issue #10947)。function/macro は Issue #10937、module/control-flow は Issue #10945、nested
const/global CST の lowering は Issue #10943。詳細は DONE.md を参照。

### `global function` 長形式宣言の corpus RED 修正 (Issue #10935 / #10937)

`julia/base` 6 ファイルの corpus ratchet RED を root-cause: `global`/`local`
の次の `function`/`macro` が識別子として誤パースされる長年の silent gap が、
#10927 の bare-`end` 拒否で顕在化したもの (#10930 は無実)。parser は upstream
`(global (function ...))` 形に修正、lowering は定義子ノードの silent drop を
委譲/typed error に置換。deferred 分 (let-local capture / 関数内エラー同等 /
`global macro` 登録) は Issue #10937、`global const` 誤パースは Issue #10938。
詳細は DONE.md 参照。
### operator token と operator identifier の分離 (Issue #10917)

`->` は precedence/arrow-function/quote のため operator token のまま保ち、unquoted
identifier/value として許可するかは `is_operator_identifier` で分離した。拒否入口は
`reject_invalid_operator_identifier` に集約され、expression・definition・declaration・
import の各 shortcut が upstream と同じ `invalid identifier` / exact arrow span を返す。
parenthesized import 名は `expected identifier` と括弧全体の span を返す。
short-circuit syntactic operator の残りは Issue #10932。詳細・mutation proof は
DONE.md 参照。

### lowering/compile front door の panic-debt 退役: Phase 1b (Issue #10905, epic #10869)

`lowering`(21)・`compile`(21、doc-comment 誤検出2件除く real 19)・
`macro_runtime.rs`(9)の real user-input-reachable `unwrap_call`/`expect_call`
49件を typed error / 型で不変条件を吸収する再構成へ全件変換 (parser crate
Phase 1a の `internal_parser_error` に倣う `internal_lowering_error`/
`internal_compile_error` ヘルパー導入)。17ファイルに zero-deny を追加、
`catch_unwind` ベースの malformed-input fuzz corpus
(`regression_misc_tests.rs::lowering_compile_malformed_input_10905_tests`)
を追加。`compile/abstract_interp/engine/mod.rs` は base cache schema
manifest 対象のため `CACHE_VERSION` を 129 に bump (wire shape 変更なし)。
epic #10869 の Phase 1b 完了。詳細: DONE.md。

### techdebt(#10925) 動的マクロ衛生リネームのスコープ対応化 (Issue #10925)

`macro_runtime.rs` の動的パス衛生リネームをフラット全木置換から
`RenameEnv` スコープスタック(function 定義 / where 節がフレーム push/pop)
へ再設計し、#10626 が兄弟の無関係なグローバル参照を巻き込むため revert して
いた関数パラメータ・`where` 型パラメータの登録を安全に有効化。upstream
`@macroexpand` 検証 8ケース全一致、regression guard グリーン維持。副産物で
UnionAll 等価性バグ(#11013)と escaped signature-binding position 未対応
(#11014)を発見・起票(修正はスコープ外)。詳細: DONE.md。

### techdebt(#10459) Phase 3 調査結果: `FunctionId`/`MethodId` 見送り、bug #11088 修正 (Issue #10990, part of #10459)

Phase 2b(#10989)と同じ判断: function/method-sig ドメイン(94+30サイト)に
`macro_bindings` 相当の bounded なテーブルは存在しない
(`function_indices`/`source_ordered_method_sigs`/`method_tables`/
`imported_functions` は全て `CompiledProgram` 非シリアライズの一時状態、
唯一永続化される `functions: Vec<Rc<FunctionInfo>>` は既に index-keyed)。
`FunctionId`/`MethodId` は導入せず、調査中に発見した実バグ Issue #11088
(sibling module 同名関数の `===`/`==`/`typeof` collapse、struct 版
#11021 の関数アナログ)を `core_compiler.rs::emit_function_value_named` の
qualified-key 存在チェックで修正。着地前の adversarial review が
「無関係かつ未 `using` の sibling が既に `using` 済みの宣言の identity を
誤って分断する」regression を検出→両アクセス経路を `unique_using_owner`
ヘルパーで対称化して解消(MWE を fixture の永続 regression guard として
追加)。2回目の advisor 相談でさらに、`unique_using_owner` が
`module_functions`(定義)のみを見て `module_exports`(export 済みか)を
見ていなかったため、export していない同名関数を持つ別の `using` 済み
モジュールが誤って owner 解決を曖昧にする2件目の regression を発見・
修正(既存ヘルパー `imported_submodule_aliases` と同じ export-aware
ルールを `unique_using_owner` にも適用)。最終 18件 identity matrix が
upstream と完全一致、既存回帰 fixture 3件 green 維持、
dispatch/modules/functions/closures/hof/macros/packages 全カテゴリと
clippy/fmt green。より深刻な副産物バグ Issue
#11089(`using` が bare-name method table の可視性をモジュール単位で
スコープしていない — 一度も `using` されていないモジュールが dispatch に
勝つ)は `compile/expr/call/dispatch.rs` の大規模変更を要し、同ファイルを
並行編集中の Issue #11076 と設計が重複するため継続 Issue #11095 へ委譲。
詳細: DONE.md、`docs/vm/SEMANTIC_ID_MIGRATION.md`「Phase 3 status」。
