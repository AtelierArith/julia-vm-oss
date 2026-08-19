# 実装済み一覧 (DONE)

**最終更新**: 2026-07-20. 実装済み項目は下の日付別「最新対応」セクションを正とし、先頭メタデータには長い issue 要約を重複させない。

> 更新方針 (Issue #3760): 新しい項目は日付ごとの共有 `## ...YYYY-MM-DD...` 見出しの下に、Issue ごとの `### ... (Issue #NNNN)` 小見出しとして追加する。同日の見出しが既にある場合は、その下に小見出しを追加し、先頭に新しい独立セクションを増やさない。
>
> 3,000 行の live-file budget を超えた過去分は [archive/DONE-2026.md](./archive/DONE-2026.md) にアーカイブ済み (Issues #6341/#11263)。年が変わったら前年分を `archive/DONE-<YYYY>.md` へ移す。

---

## 最新対応 (2026-08-18)

### General Wasm AoT first slice

Added `AotBackend::Wasm`, exact-pinned `wasm-encoder`, a typed Core IR → AoT →
`IrModule` → Wasm path, structured CFG dispatch, direct calls, scalar operations,
and generated-module descriptor ABI v2 with inline arbitrary-rank UInt8 metadata.
Consolidated AoT tests execute five
unrelated Julia sources and RGBA alpha-preserving mutation in Node. No transform
registry, JS kernel, imports, dynamic dispatch, or fallback is present.

## 最新対応 (2026-07-20)

### Windows default LOAD_PATH includes embedded stdlib (Issue #11800)

The loader previously used the Unix-only literal `@stdlib:@packages` as its
default, then parsed it with Windows' `;` separator. Windows therefore treated
the whole literal as a filesystem path, so the REPL's automatic
`InteractiveUtils` import failed on the first evaluation. An unset environment
now produces structured `Stdlib` and `Packages` entries directly; explicit
`SUBSETJULIA_LOAD_PATH` / `JULIA_LOAD_PATH` values retain platform-native
parsing. A unit test fixes the platform-independent default invariant.

### Exception parity Phase 3 ratchet and numeric fallback sweep (Issues #10813/#11148)

The upstream-vs-sjulia exception corpus is now a two-sided, issue-linked gate:
new type/catchability divergences fail, and resolved allowlist rows must shrink.
`premerge_gate.sh --full-suite` runs the release differential lane. Splitting
probe setup from its caught expression removes the false #10593 result and
records 44 comparable cases: 41 exact, three tracked gaps (#11559/#11794/#11390).
Interpreter absence, timeout, signal, non-zero sentinel exits, and missing
sentinels are explicit fatal health states rather than apparent matches.
The Base sibling sweep also restores upstream's `::Real` signatures for
`conj`/`isreal`/`flipsign` (#11522/#11525) and `real`/`signbit`/`abs`
(#11797), with one Julia-parity fixture and six permanent corpus sentinels.
The remaining untyped math-fallback inventory is filed as #11799.

### control-flow struct の inner constructor activation (Issue #11679)

- control-flow 内の non-parametric `struct` が explicit inner constructor を持つ場合も、宣言到達前は型と constructor method を非公開に保ち、`DefineRuntimeNominal` 到達時に予約済み type ID と constructor world を同時 publish する。raw/default constructor は upstream 同様に生成せず、未到達 branch の型・method は漏洩しない。
- fresh VM、REPL live append、catchable error 後の snapshot adoption で constructor function index と到達済み prefix を共有し、後続 eval および error recovery 後も同じ inner constructor semantics を維持する。parametric runtime struct は #11678 の fail-closed rejection を継続する。
- fixture `types/runtime_nominal_control_flow_edges_11654.jl` と consolidated REPL regression が、変換 constructor、default suppression、未到達 branch、後続 eval、error recovery を upstream parity 付きで固定する。serialized bytecode 変更に伴い Base cache schema を 191 へ更新した。
### Typed array literals always apply convert (Issue #10835)

The typed-literal builders previously chose whether to emit `convert(T, x)`
through a primitive-tag predicate plus a hard-coded boxed-type prefix allowlist.
That duplicated Base method capability and drifted when a new conversion became
supported (#10750). Both the main compiler and runtime specializer now mirror
upstream `setindex!` semantics directly: every element of a non-empty `T[...]`
passes through `CallBuiltin(Convert, 2)` before `MemorySet`. The two predicates
and their public bytecode export are removed.

The expanded fixture covers recursive `Vector{Int}` conversion, `Char[97]`
(unsupported-feature #11779), and an exact-type boxed Regex control. The
specializer unit test asserts even `Any[...]` emits Convert, and CHECKLISTS.md
records the unconditional invariant. Existing Regex object-identity divergence
was isolated and filed as #11780. Base cache version 188→189 invalidates
programs compiled with the former allowlist. Full-suite coverage exposed that a
numeric-only Union target was intercepted by the generic numeric method before
the VM's identity conversion; both Convert execution paths now return an
already-member value before dispatch while preserving user-defined Union
conversion methods, with #9842's fixture extended for #11781. The literal
builder evaluates the original target expression once and reloads that value,
rather than round-tripping `ArrayElementType` through a rendered `PushDataType`
name, so nested-parametric Union members retain their structured identity
(#11783) and element-side binding mutations cannot change later conversions.
The separate dynamic-target logical `eltype` metadata gap is tracked by #11787.
### chomp の multibyte 対応 (Issue #11642)

- `chomp` が `length(s)`(文字数)を `codeunit` のバイト index に誤用しており、multibyte 文字を含む文字列で末尾改行が残っていた。upstream 形の `lastindex`/`prevind` 走査へ書き換え(lone `\r` を chomp しない upstream 挙動にも一致)。fixture: `strings/chomp_multibyte_11642.jl`(parity 済)。

### AoT inline 残骸の bare path statement 解消 (Issue #10796)

- statement 位置(値未使用)の top-level call を AoT inliner が inline した際、inlined body の accumulator 変数(戻り型注釈由来の `Convert` ラッパー含む)を bare Rust path statement として残し `path_statements` 警告を出していた。effect-free な inline 結果(Var/リテラル/その Convert 包み)を statement 位置では drop する(`inline_result_is_effect_free`)。
- unit test `statement_position_inline_drops_effect_free_result_issue_10796`; MWE の `--emit-binary` は警告 0 で `done` 出力を確認。`scripts/test_aot.sh` gate 済。

### broadcast の Any-eltype per-element 型保存 (Issue #10787)

- `Any[1, 2.5]` のような non-concrete eltype 配列への broadcast が、先頭要素サンプルで確定した storage に後続要素を coerce していた(`2.5 + 2.5` が silent に `5`)。upstream `copyto_nonleaf!`/`promote_typejoin` と同形の widening 経路を `copy(bc)` に追加: operand に non-concrete eltype がある場合は Any storage に materialize してから実結果型の typejoin へ narrow(`Vector{Real}` 等)。concrete eltype の既存 fast path は不変。
- fixture: `array/broadcast_any_eltype_widening_10787.jl`(1D/2D/演算子/同種/非数値/空、parity 済)。発見 gap: #11776(`vec(::Matrix)` が flatten しない)。

### Phase 4 classifies every TypeVar/CoreType name map (Issue #10992)

The 14 direct `HashMap<String, CoreType>` spellings are now represented by two
private, purpose-named authorities: `LexicalTypeBindings` for one dispatch
candidate's `where` substitutions and `RenderedTypeParseCache` for pure
rendered-name parsing. `check_name_based_lookup.sh` excludes only those exact
declarations and holds every unclassified raw site at zero. A same-count
negative mutation replaces a classified lexical use with a raw map and proves
the audit fails, closing the substitution hole in the former baseline-14
ratchet. The semantic inventory mirrors the exclusions and reconciles at zero.
Issue #10992 remains open for #11095/#11089's function/method work.
### runtime specializer の typed array literal codegen (Issue #10746)

- `Any[x]` / `Float64[...]` など type-object-prefixed array literal を含む関数が、specialization 全体を放棄せず specialize 可能になった。main compiler の getindex literal arm と同一の build(`NewMemory` → 要素ごと `PushI64`/値/`MemorySet` → `FinalizeArray`、`convert` ルーティング込み)を specializer が emit する。
- 要素型マップ(`bare_type_name_array_element_type` / `typed_literal_abstract_element_needs_convert`)は #8192 の `typed_scalar_binary_instr` と同じパターンで `subset_julia_vm_bytecode` に移して両 codegen path で共有(乖離防止)。マップ外の要素型(user struct 等)は従来どおり generic body へ fallback。
- unit test: `compile_typed_array_literal_emits_literal_build_issue_10746`(specialize module)。fixture: `array/specializer_typed_literal_10746.jl`(Any/Float64/Int+hex/ComplexF64/String/Real、parity 済)。

### regex match/findnext の範囲外 index エラー型 parity (Issue #10736)

- `match(re, s, start)` の `start > ncodeunits(s)+1` は upstream 同様 `ErrorException("PCRE.exec error: bad offset value")` を送出(RegexMatch builtin handler に上限チェック追加)。silent `nothing` を廃止。
- `findnext(re, s, i)` の `i < 1` は upstream の `convert(UInt, i-1)` 経路と同じ `InexactError` を、pure Julia wrapper 内で同じ `UInt64(ii-1)` 変換を実行して再現(`base/strings/search.jl`)。
- fixture: `regex/match_findnext_index_errors_10736.jl`(境界 `start == ncodeunits+1` の合法性・既存 BoundsError も含め upstream parity 済)。

### SubstitutionString の AbstractString surface (Issue #10735)

- `s"..."` (SubstitutionString) が `replace` の外でも AbstractString として振る舞うようになった。upstream base/regex.jl と同形に `ncodeunits`/`codeunit(s,i)`/`isvalid`/`iterate` をラップ先の string へ転送し、sjulia では builtin 実装のため generic fallback が無い `==`(String 両方向 + SubstitutionString 同士)/`length`/`getindex`/`hash`/`String`/`eltype` を狭い具象メソッドで追加(`subset_julia_vm/src/julia/base/strings/util.jl`)。
- VM 側の構造修正: builtin 裏付けの名前(`ncodeunits` 等)に Pure Julia メソッドが 1 つでも付くと全呼び出しが CallDynamic 化し、String 引数が dispatch miss で即 MethodError になっていた。CallDynamic の miss パスに CallFunctionVariable と同じ builtin fallback(`BuiltinId::from_name` → `execute_runtime_builtin_immediate`)を追加(`subset_julia_vm_vm/src/vm/exec/call_dynamic.rs`)。
- fixture: `regex/substitution_string_abstractstring_10735.jl`(upstream parity 済)。
- 発見した gap/bug を起票: #11751(1-arg `codeunit` が compile 時 arity check で拒否)、#11753(macro 引数内の `r"..."` が lowering error)、#11754(1-arg `hash(x)` が 2-arg dispatch へ転送されない)、#11755(`string(...)` 呼び出し結果の直接 `==` が Str 型付け fast path で誤 false)、#11756(macro 引数内の `s"..."` が silent に plain String 化)、#11757(macro 引数内のエスケープ引用符文字列が壊れる)。Workarounds W-78〜W-80 登録。

### Reached selective imports survive catchable REPL errors (Issue #11748)

The compiler previously emitted runtime binding-metadata stores for
`Stmt::Using` but no statement-level completion event, so catchable-error
recovery could neither retain a reached selective import nor distinguish it
from a source-later import. `Instr::ActivateUsing` now records the owning module
path and local `usings` index after the statement's metadata stores complete.
The VM keeps a distinct, source-ordered activation trace per appended main;
session recovery validates Main-owned indices and stores only those exact
`UsingImport` rows. If any import remains unreached, the errored VM is dropped
because its compiler-reserved binding surface can expose the dormant suffix;
the next eval rebuilds from the sanitized session state. Regression coverage
pins immediate and post-barrier reachability, source-later non-callability,
owner-index isolation, trace dedup/reset, and fail-closed index validation.
Serialized bytecode changed, so the Base cache schema is bumped to version 186.

## 最新対応 (2026-07-19)

### No-suffix conditional method survives full-compile error (Issue #11745)

Fresh REPL full-compile recovery used the hoisted-only
`current_input_function_count` as its method-publication eligibility gate. A
conditional method represented by an inline `Stmt::FunctionDef` therefore
opened a recovery plan only when a source-later hoisted method happened to be
present (#11742), and disappeared at the next full-rebuild barrier otherwise.
The gate now consumes the existing complete
`current_input_stored_function_count` authority (hoisted source methods plus
inline named methods). A regression proves the final reached method is callable
immediately after the error and after a module rebuild barrier.

### Fresh full-compile error recovery keeps reached methods (Issue #11742)

A fresh `REPLSession` full compile that reached a conditional method definition
before an uncaught error discarded the entire VM when the input contained no
runtime nominal definition. The recovery-plan builder was incorrectly gated on
`runtime_nominal_templates` being non-empty even though it also records
`DefineEvalFunction` activations. It now admits plans when the current input has
Julia-visible source methods or runtime-nominal templates; compiler-generated
constructor/helper markers alone do not opt a normal input into recovery
validation. On success, method-only full compiles keep the ordinary definition
commit path; the new plan is consumed only by the error arm. A two-eval regression test
proves the reached method survives, an unreached later method remains absent,
and the recovered method still survives the next full-rebuild barrier.

### Plain-String regex replacement keeps $ literal (Issue #10721)

`replace(s, re => "plain \$1")` が plain String replacement でも `$1` を capture
展開していた(fancy-regex の replacement syntax が `_regex_replace` builtin を
素通し)。upstream は plain String を literal 扱いし、展開は SubstitutionString
(`s"..."`, `\N` 形式)のみ。builtin 側で `$` → `$$` エスケープを挿入。
SubstitutionString 経路 (`_replace_general`/`ExpandSubstitution`) は不変。
fixture: `regex/replace_literal_string_10721.jl`(parity 6/6)。

### HOF/callable sqrt keeps BigFloat (Issue #10604)

`map(sqrt, [big"2.0"])` が MethodError になっていた(直接呼び出しの
`BuiltinId::Sqrt` には BigFloat arm があるが、function-value 経由の
`try_call_intrinsic` sqrt arm が f64 変換失敗で decline)。callable lane に
builtin と同じ BigFloat arm(full precision sqrt + 負数 DomainError)を追加。
非数値の MethodError decline(#10481)は不変。fixture:
`bigfloat/hof_sqrt_bigfloat_10604.jl`(parity 5/5)。dot-broadcast lane の
BigFloat eltype 縮退は別バグとして #11727 に起票。

### Undefined typed-empty-array head raises UndefVarError (Issue #10583)

`SomeUndefName[]` が permissive な `ArrayElementType::Any` catch-all で silent に
`Any[]` へ落ちていた。identifier 形の未解決 head は upstream の lowering
(`T[]` → `getindex(T)`)同様 `getindex(head)` へルーティングし、undefined なら
runtime UndefVarError、runtime-only binding なら正しい `getindex` dispatch に
到達する。value binding(#6839)・既知型 head・compound head(`Foo{Int}` /
`Union{...}`)の挙動は不変。fixture:
`array/typed_empty_array_undef_head_10583.jl`(parity 5/5)。

### UnionAll trailing unbounded binder elision (Issue #10505)

`Array{T,N} where {T<:Real,N}` を upstream (`show_can_elide`, base/show.jl)
同様に `Array{T} where T<:Real` へ正規化。`format_unionall_name` に反復
trailing-elision を追加: 最内 binder が unbounded かつ base の最終パラメータと
一致し、他のパラメータ・残 binder の bounds に現れない場合のみ pop(#10635 の
same-name binder ガードを bounds へ拡張)。全 unbounded の `Array` 収束や
bounded-innermost の非省略は従来どおり。8 形が julia 1.12 と一致。fixture:
`types/unionall_trailing_elision_10505.jl`、unit:
`trailing_unbounded_where_var_is_elided_issue_10505`。

### Rank>=3 Array display renders upstream's ;;-literal form (Issue #10385)

rank>=3 の Array を println/string すると内部表現
(`Array{Float64, 3}(MemoryRef{...}, dims)`)、show すると独自 summary
(`Array{T, N} with size (...)`) が出ていた。compact renderer に再帰 N-d 形式を
実装: dim-1 は `"; "`、dim-2 は space、dim-k (k>=3) は `k` 個のセミコロン + space
で結合(`[0.0 0.0; 0.0 0.0;;; 0.0 0.0; 0.0 0.0]`)。rank>=3 の空配列は
`Array{T, N}(undef, dims...)` 形式。io.jl の show summary 分岐は 2-d と同じ
`print(io, arr)` 委譲に統一。zeros/reshape/fill/Bool/4-d/空 の 7 形が julia 1.12
と byte 一致。fixture: `array/ndim_array_display_10385.jl`(parity 9/9)。

### Typed exception payloads share one keyed one-shot carrier (Issue #11647)

The independent pending slots for `MethodError`, `DomainError`, `TypeError`,
`StringIndexError`, `ParseError`, and field-index `BoundsError` are replaced by
one `PendingExceptionPayloadCarrier`. Producers atomically derive the matching
`VmError` from the payload key, while the exception funnel consumes the carrier
once before classifying catchability. A six-kind lifecycle matrix covers exact
matches, mismatches, internal/stale errors, nested replacement, unhandled
clear, and same-session recovery; four existing fixtures add real nested
catch/rethrow coverage. Registered source-audit mutations reject direct and
disguised carrier fields, late consumption, and registry weakening.

### Module-scope Ref index-assign preserves value type (Issue #10363)

module スコープの global `Ref` への `R[] = v` が Int64 を Float64 に silent
coerce していた。index-assign lowering は target の静的型が不明なとき legacy の
unboxed-F64-array 前提で `ToF64` を挿入するが、zero-index store
(`IndexStore(0)`) の対象は Ref cell で、runtime は値を逐語格納するため coercion
がそのまま型破壊になる(Main/関数ローカルは型既知で無事)。`indices` が空の
store では coercion を行わないよう修正。unboxed 配列 store の upstream 互換
conversion(Int → Float64 配列で 3.0)は不変。fixture:
`modules/module_ref_index_assign_10363.jl`(parity 4/4)。

### print/println evaluate all arguments before writing (Issue #10351)

多引数 `print`/`println` の stdout fast path と io-first split path は、引数を
1 つ評価するごとに書き出していたため、後続引数の評価副作用(それ自身の出力)が
本呼び出しの先行書き込みの後に出る order divergence があった
(`println("x: ", f())` が `x: side` → `1` になる)。後続引数に副作用があり得る
場合(リテラル/変数以外)は全引数を temp へ左から評価してから書き出す
(`spill_print_args` / `compile_stdout_print_args`)。副作用フリーの多引数は従来の
直接ループのまま。IOPrint 一括経路(全引数評価後に単一書き込み)は元々正しい。
fixture: `io/println_arg_eval_order_10351.jl`(stdout ログ順 + IOBuffer
position probe; `copy(::IOBuffer)` 未対応は Issue #11714 として起票)。

### Script/gate maintenance: loc_report crate-split, corpus require, seed-sweep normalization (Issues #11592 / #10946 / #11474)

- **#11592**: `scripts/loc_report.sh` が crate split 前の
  `subset_julia_vm/src/vm/executable.rs` を読んで FileNotFoundError で落ち、
  compile/vm/lowering の area 行も silent 0 になっていた。area 行を移設先
  crate (`subset_julia_vm_compile/src`, `subset_julia_vm_vm/src`,
  `subset_julia_vm_lowering/src`) に解決し、#10817 baseline 比較の同一性を維持
  (crate total も split 分を合算)。
- **#10946**: julia/ submodule 不在の worktree で corpus 依存テスト
  (`parser_corpus_base_ratchet`, `base_exports_do_not_exceed_upstream`) が
  silent PASS し、full-suite gate を false-green にしていた (#10935 incident)。
  `SJULIA_REQUIRE_CORPUS=1` (premerge_gate.sh が export) で skip を FAIL に
  変換。worktree setup を CHECKLISTS.md に追記。
- **#11474**: dispatch seed sweep が `@time` fixture の elapsed 秒差分を
  seed-variant と誤検出。宣言的 normalization 登録簿
  `docs/vm/DISPATCH_SEED_SWEEP_NORMALIZATION.tsv` (stale entry は FAIL) を
  導入し、`elapsed-time` 種別で "  <float> seconds" 行のみマスク。

### Nested @testset summaries aggregate into the enclosing set (Issue #10338)

VM の testset 簿記は flat なスカラーカウンタ + `current_testset` 1 本で、
`_testset_begin!` が外側のカウントをクリアするため、nested `@testset` を含む
外側 testset の summary 行が「最後の内側 set のカウントの重複」になっていた。
`testset_stack: Vec<TestSetFrame>` を導入し、begin で外側カウント+名前を push、
end で終了 set のカウントを summary 表示してから親へ fold(upstream
`DefaultTestSet` の `record(parent, child)` 集約を count frame に圧縮した形)。
builtin 経路 (`_testset_begin!`/`_testset_end!`) と legacy `Instr::TestSet*`
経路の両方が同じ helper (`testset_begin_frame`/`testset_end_frame`) を通る。
非 nested の出力は不変。回帰: `testset_exit_code_8191_tests.rs` に集計 3 本
(2段/mixed outcomes/3段)。upstream julia 1.12 は同 MWE を `3/3` に集計する
ことを確認済み。

### .sjvmbc load replays promotion rules (Issue #10339)

`.sjvmbc` ロード (`vm_bytecode_file.rs::load`) は `compile_context` 再構築のみ
行い、Base cache ヒット経路 (`cached_base_from_serialized`) が行う promotion
registry replay を行わなかったため、`.sjvmbc` 実行中の runtime reflection
(`Base.promote_type` 系) は空 registry を参照していた (#10265 codex review 指摘)。
ファイル形式 v7 で payload にコンパイル時 registry のルール一覧 (sorted) を追加し、
deserialize 直後に `register_promotion_rule` replay + `mark_registry_initialized`
を実施 (iOS 向け `load_from_bytes` も同経路)。旧バージョンファイルは exact-match
version check で従来どおり再生成へ。回帰: `load_replays_promotion_rules_10339`
(save → registry clear → load → replay 検証)。`main_scope_names` (#9182) は
REPL-only consumer で `.sjvmbc` CLI に消費者がないため documented-latent のまま。

### Seeded PROGRAM_CACHE hits restore compile context (Issue #10335)

seeded PROGRAM_CACHE (Issue #10120) のヒット経路は postcard decode した
`CompiledProgram` を `compile_context: None` のまま返していた (`#[serde(skip)]`
なので decode で必ず None になるが、この経路だけ restore を呼んでいなかった)。
`seeded_program_cache_lookup` に呼び出し元の live `Program` を渡し、Base cache /
`.sjvmbc` ロードと同一の `restore_compile_context_from_program` を decode 直後に
通すことで、seeded ヒットも fresh compile と同じ compile context を持つようにした
(cache-restore parity invariant, Issue #10265)。回帰テスト
`seeded_program_cache_hit_restores_compile_context_10335` は serialize 済み entry を
thread-local に注入して実 lookup を駆動し、fresh context との parity を検証する。

### User Array-named structs keep nominal identity vs Base.Array (Issue #11388)

#11395 クラスタ第三弾で isa/`<:` リークを解消。(1) builtin 専用 JuliaType variant
にパースされる shadowing 注釈 (`x::Array`) を、module がその名前の struct を宣言
している場合に qualified `Struct("Faux.Array")` へ署名側で書き換え (module 内の
`Base.iterate(::Array)` が match-time collapse に依存しなくなる)。(2) その上で
native array family (Array/Vector/Matrix) の bare-vs-qualified collapse を
builtin owner のみに制限 (`canonical_core_nominal_family` /
`nominal_family_names_compatible`)。(3) isa の正規化名比較 (owner 剥がし後の
等値/subtype) を family 非互換時は raw 名で engine に委ねる。#11388 MWE の
isa/`<:`/splat/typeof が julia 1.12 と一致、shadowing fixtures (#11094/#11021/
#8861) は全緑。fixture: `modules/module_array_shadow_identity_11388.jl`。
### Module-owned type display follows Main visibility (Issue #11365)

instance 表示は owner を全て剥がし (`M.B()` → `B()`)、typeof 表示は `Main.` を
付けない、という二重の乖離を upstream の可視性規則に揃えた: 型の leaf 名が Main
から unqualified で到達可能 (トップレベル宣言 / `using` import) なら bare、そうで
なければ `Main.M.B` の完全パス。`using` の import emission が Main スコープに残す
bare-leaf `DataType` global (cache restore lane でも再構築) を authority とし、
`Vm::run()` 冒頭で frame-0 から thread-local レジストリへシード
(`set_struct_name_registry` と同型)、global store の choke point が実行中を
インクリメンタル更新する。display マトリクス 8 ケースが julia 1.12 と一致。
fixture: `modules/module_struct_display_owner_11365.jl`。

### REPL HOF helpers install on the held VM (Issue #9784)

Ordinary top-level lambda/HOF, do-block, generator-body, and filtered-generator
predicate helpers now join the relocatable live append instead of forcing a
fresh full compile. `ReplDefinitionActivation` primary/refresh membership, not
a contiguous source-function count, controls publication: source methods and
refresh bodies remain world-gated until their marker, while marker-less helpers
are visible immediately at world 1.

Compiler and VM error recovery share the same final aligned primary indices and
retain helper bodies for function-index stability. Only reached source primaries
advance Julia-visible generic and method-source snapshots; generated helper
names never leak into dispatch/reflection registries. Consolidated regressions
cover named methods mixed with HOFs, helpers before/after a thrown error,
do-blocks, generators, and filtered predicates with zero fresh VM builds.

### Sibling-module type alias no longer leaks bare (Issue #11452)

lowering の bare-name unique-leaf alias fallback を「見える owner」(トップレベル・
lexical に包含する module・`using`/`import` 済み module) に制限した。pre-scan が
using/import エッジを thread-local `IMPORT_EDGES` に記録し (clear/snapshot/restore
対応)、never-imported な sibling module の builtin 同名 alias
(`baremodule AliasOwner; const BigInt = Int64`) が別 module の `f(x::BigInt)`
注釈を汚染しなくなった。#11086 の imported-alias 挙動は fixture ごと維持。
fixture: `types/type_alias_sibling_module_no_bare_leak_11452.jl`。

### Owner-aware array-wrapper recognition (Issue #11388, partial)

共有述語 `is_array_wrapper_struct_name` と `struct_instance.rs` の同類述語が
module qualifier を Base/Core/Main 所有の場合のみ許可するようになり、user module の
`Faux.Array` が native array 高速経路 (~50 呼び出し元: display の `[1, 2]` 化、
`_mem`/`_size` field alias、compile routing、specializer) に入らなくなった。
`isa(a, Base.Array)` リークは match-time の bare-vs-qualified family collapse
由来で、signature 側 owner 解決 (#11395/#11078) が必要なため残存 (Issue コメント参照)。

### Enum constants follow upstream Dict expansion order (Issues #11656 / #11666)

Enum member metadata and `instances(Enum)` remain source-ordered, while member
constant stores now follow the slot iteration order produced by upstream
`Dict{basetype,Symbol}`. One bytecode-owned authority reproduces Julia 1.12's
64-bit integer hash, linear probing, load-factor resize, probe-limit resize,
and slot-order rehash; both compiler emission and the VM collision guard consume
that order. Catchable member collisions therefore retain and replay the exact
same published subset as upstream, including source-later members visited first
by the Dict.

Duplicate values and duplicate names are rejected during lowering before the
enum type or any member is published. Full base-type range conversion and wide
unsigned/128-bit enum values remain tracked by #11667.

## 最新対応 (2026-07-18)

### throw(value) preserves any non-Exception value's identity (Issue #11554)

`throw(value)` coerced any argument whose static compile-time type was not
`Struct`/`Any`/`Str` (a bare `DataType`, a number, a `Symbol`, a `Tuple`, an
`Array`, …) into `ErrorException(string(value))` via `ToStr` + `ThrowError`,
losing the original value's identity. Upstream Julia allows throwing ANY
value and `catch` binds the exact thrown value — e.g. `throw(Int32)` binds
the `Int32` `DataType` itself (`typeof(T) == DataType`), not a stringified
`ErrorException`. The compile-time `throw` dispatch (`compile_builtin_io`)
now always emits `Instr::ThrowValue` regardless of the argument's static
type, which already preserved struct-typed exceptions verbatim via
`pending_exception_value`; this also retires the special-cased `Str` branch,
so `throw("msg")` now preserves the raw `String` too (matching upstream)
instead of wrapping it into `ErrorException("msg")`. Normal `Exception`
throwing (`throw(ErrorException(...))`, `error(msg)`) is unchanged. Fixture:
`exceptions/exceptions_throw_type_value_preserve_11554.jl` (DataType
identity, downstream `Vector{T}`/`isa` construction from the caught value,
several other non-Type value kinds, and unchanged normal-exception
behavior). Discovered a separate, pre-existing bug while adding the
downstream fixture — a *local-scope* function definition whose signature
references a local `Type`-valued variable fails to dispatch even without
`throw`/`catch` involved — filed as Issue #11574 (out of scope here).

### Static dispatch on a literal Tuple{T,T} where-param now honors the concrete argument (Issue #11490)

`same_tup(::Type{Tuple{T,T}}) where T = true; same_tup(::Type) = false` used to
resolve `same_tup(Tuple{Int,Int})` and `same_tup(Tuple{Int,String))` to the SAME
`CallResolved` candidate at a STATIC call site (runtime dispatch through a
variable-bound function value was already correct). Root cause was a different
bug class from Issue #11231: the single-argument compile-time static-dispatch
fast path `core_static_datatype_exact_match`
(`subset_julia_vm_compile/src/compile/expr/call/dispatch.rs`) checked each
element of a candidate's `Tuple{...}`/`Struct{...}` `where`-bound type
parameters independently, with no shared state between sibling comparisons, so
a repeated type variable (`T` in `Tuple{T,T}`) matched any combination of
concrete element types instead of requiring the same binding (upstream's
diagonal rule) — #11231 fixed the general CoreType typemap matcher's
struct-to-struct binding extractor, but this narrower fast path bypasses that
matcher entirely once it finds a `.find()` match, so it never benefited from
that fix. Fixed by threading a `HashMap<String, CoreType>` of type-variable
bindings through the recursive match; a mismatch now falls through to the
already-correct general `table.dispatch` path. The anonymous
covariant/contravariant placeholder name `"_"` is exempted. Extended the
`repeated_where_param_conflict_11231.jl` fixture with a `same_tup` case,
verified against upstream `julia` 1.12.6.

### Third `sqrt` router closes the exact-or-Any invariant (Issue #11486)

`BuiltinOp::Sqrt` (`subset_julia_vm_compile/src/compile/expr/builtin.rs`) was
the one sqrt router #11526 (Issues #11436/#11468/#11469/#11481/#11510/#11511)
left unguarded: its "not statically known to be Complex" branch still
unconditionally emitted `Instr::SqrtF64`, treating an `Any`/`Union` static
type (or an unresolved non-Complex `Struct`) as proven real. It now mirrors
`compile_builtin_math`'s guard and `compile_sqrt`'s already-correct `Any`
branch — only a proven exact primitive numeric type reaches `SqrtF64`;
everything else defers to `CallTypedDispatchOrBuiltin` / the `BigFloat`
builtin. Traced and confirmed via `--dump-bytecode` that this branch is
currently unreachable through source `sqrt(...)` calls (scalar, generic, and
broadcast) — `compile_sqrt` always intercepts first — so this closes #11486
as a consistency/defense-in-depth fix rather than a new observable bug; the
existing `constructor_return_exact_or_any_11436.jl` fixture remains the
regression coverage for the two live bugs #11526 already fixed. The
lattice provenance-bit redesign and full single-routing-authority
consolidation (tracked under #10461) remain follow-ups.

### Bundled StaticArraysCore models the upstream four-parameter SMatrix (Issue #11542)

`struct SMatrix{M,N,T} <: StaticMatrix{M,N,T}` in
`subset_julia_vm/packages/StaticArraysCore/src/types.jl` now declares the
upstream fourth length parameter (`struct SMatrix{M,N,T,L} <:
StaticMatrix{M,N,T}`, `L == M*N`), mirroring the #11432 fix applied to the
separate, independent bundled StaticArrays package's own `SMatrix` struct. A
field annotated `W::SMatrix{2,2,Float64,4}` under `using StaticArraysCore`
directly no longer raises `too many parameters for type
StaticArraysCore.SMatrix`. `SMatrix{M,N,T}` (and narrower spellings) remain
constructible via incomplete parameterization — sjulia's existing partial
`UnionAll` application/dispatch generalizes to the added trailing parameter
with no other dispatch-site signature changes needed. No Rust-side change
was required: the small-array inline-representation fast path
(`try_make_static_array`, `subset_julia_vm_vm/src/vm/exec/struct_ops.rs`)
only recognizes the `"StaticArrays."`-prefixed display name, so it never
matched `StaticArraysCore.SMatrix` before or after this change. Fixture:
`static_arrays/static_arrays_core_smatrix_four_param_11542.jl`. Filed
Issue #11573 (deferred, not fixed here) for a discovered, pre-existing gap:
constructing `SMatrix{M,N,T,L}` via a single flat-Tuple argument (rather than
separate positional arguments) bypasses the `check_array_parameters` length
validation, because sjulia's dispatcher resolves to the struct's
auto-generated default `(data::Tuple)` inner constructor for that call shape
(matching real Julia's own specificity rules, verified with a standalone toy
struct) — real upstream `StaticArraysCore.SArray`/`SMatrix` avoids this by
declaring an explicit `new`-calling inner constructor, which bundled
`SArray`/`SMatrix` does not have.

### UTF-8 string index validation is shared across all indexing routes (Issue #11621)

A VM-local structural validator now classifies every one-based code-unit index
as a character start, numeric out-of-bounds index, or in-bounds continuation
byte. Scalar `String`/`StrBytes`, index vectors, and unit/step range endpoints
share it, and range slicing derives Rust's exclusive byte end only after Julia's
inclusive endpoint is validated. The fixture matrix covers ASCII, multibyte,
and malformed byte-backed strings.

### Base cache schema fingerprint audit has mandatory premerge ownership (Issue #10688)

The source-only registry sync checker now requires the fingerprint audit's
default-gate row. Negative controls prove that weakening that registration or
changing a real manifest-listed schema source without its version/snapshot
update fails with the targeted fingerprint diagnostic. The maintainer checklist
requires the version bump, changelog entry, and snapshot refresh in one PR.

### Shadow-audit guard grammar is conformance-tested (Issue #11604)

The compile-expression local-shadow audit now runs production-independent
accepted/rejected grammar cases before scanning Rust source. Direct `name`,
`name.as_str()`, and whitespace variants must match, while unguarded comparisons,
positive lookups, and unrelated identifiers must not. A registered sandbox
mutation removes `.as_str()` support and requires the projection-specific
diagnostic, complementing the existing unguarded-source mutation.

### Declared invoke signatures are matrix-tested across callable lanes (Issue #11619)

The invoke fixture now covers direct and stored callables across static and
runtime-held signature tuples, positional and keyword calls, and declared
`Any` versus `Integer` (16 upstream-parity cells). A bytecode regression proves
the stored-callable cases emit all four `InvokeFunctionVariable*` forms. The
shared call-routing audit now requires every form to enter the common
declared-signature helper and rejects mutation to value-based runtime
refinement, so literal `Any` remains authoritative as required by Julia.

### REPL nominal types activate at their source position on the held VM (Issues #11635 / #9784)

Brand-new Main-owned abstract types, non-parametric primitive types, and enums
now join functions and concrete structs in one live-VM definition transaction.
Compilation appends aligned metadata tails, but the VM keeps every current-input
type binding private until `DefineEvalAbstractType`,
`DefineEvalPrimitiveType`, or `RegisterEnum` reaches the declaration's exact
source position. Later evaluations retain subtype/dispatch, `sizeof`, enum
construction, `instances`, and display behavior without rebuilding the VM.

Catchable top-level errors commit one validated interleaved prefix across
functions, concrete structs, abstracts, primitives, and enums, while every
unreached binding and enum member remains undefined. Rejected setup is now
preflighted before the live VM is taken; activation maps are built
transactionally, and an enum-registry guard restores thread-local display state
unless the same compiler/runtime prefix commits. Cache schema version 172
persists ordered enum metadata. Parametric or inner-constructor structs,
redefined types, modules/imports/macros/type aliases, Base/preload-owned methods,
and opaque runtime `eval` remain later Issue #9784 slices.

### ParseError and StringIndexError now carry their upstream payloads (Issues #11572/#11615/#11618)

Parser-originated `ParseError` values now carry a structured
`JuliaSyntax.ParseError` detail instead of `nothing`: `SourceFile` preserves the
parsed substring, absolute byte offset, filename/first-line metadata, and
one-based line starts; each `Diagnostic` preserves absolute one-based byte
bounds, severity, and structural parser text; `incomplete_tag` is derived from
parser recovery state. Both ordinary and start-offset `Meta.parse` paths are
covered against upstream. Exact `Base.JuliaSyntax` binding remains tracked by
#11614.

VM-raised `StringIndexError` now carries the exact offending `String` value,
including byte-backed `StrBytes`, rather than the funnel's empty-string
placeholder. A one-shot key-matched carrier is created atomically at each
scalar/vector/range raise site and consumed unconditionally at funnel entry.
Vector indices inside a multibyte character now retain the correct
`StringIndexError` class (#11615), and range indexing validates Julia's
inclusive code-unit endpoints before computing Rust's exclusive byte end
(#11618). Numeric out-of-bounds remains `BoundsError`; restoring that error's
receiver is separately tracked by #11616. Prevention follow-up: #11621.

Strict range validation also exposed Base helpers that had relied on the old
permissive byte endpoint. `split`, `rsplit`, `chopprefix`, `chopsuffix`,
predicate `strip`, multi-pattern `replace`, and the registered Irrational
display workaround now navigate character starts. `lastindex`, `thisind`,
`nextind`, `prevind`, and `isvalid` share malformed UTF-8 segmentation, so a
standalone continuation byte remains its own character start (#11624/#11628).
String scalar/range/vector/colon indexing now shares one byte-aware classifier
for `Str`, `StrBytes`, and captured `Any` values; non-integer vectors/ranges are
rejected with upstream exception classes and maximal ranges fail without host
overflow (#11627/#11629/#11630/#11640/#11643/#11644).

`Meta.parse` now returns appendably incomplete expressions, isolates diagnostic
segments, groups same-line semicolon expressions as `Expr(:toplevel, ...)`,
consumes one newline separator, validates signed start bounds, and rejects
UTF-8-interior starts without a host panic (#11633/#11634/#11636/#11637/#11639/
#11641). Nested catches freeze the payload at handler capture so later catches
cannot overwrite it (#11632).

The nested detail type exposed the #10445 same-leaf constructor-owner bug.
Concrete inner allocations now resolve their declaration owner directly, with
a regression test for a `Main` type shadowed by a module leaf. The remaining
cached synthetic-default collision is registered as workaround W-75 on
`Base.ParseError`.

Verification completed with parser tests, targeted VM/compiler/integration and
fixture lanes, both workaround audits, source/fixture audits, clippy/fmt,
independent adversarial re-review, and the full release suite: 5,838/5,838 tests
passed (2 slow, 4 skipped).

### Direct dynamic calls use the shared CallRequest resolver (Issue #10461 Phase 1b)

`CallDynamic` now carries the compiler-resolved `callee_name` in boxed
`DynamicCallOperands` instead of an anonymous fallback/arity/candidate tuple.
Bare and qualified direct-call cache misses and callable-value calls build the
same semantic request and invoke the same structured runtime scorer; runtime no
longer needs to infer call identity from candidate order. The source audit and
its mutation control require carried-identity consumption, request-based
routing, and the single compiler emission hub. The serialized operand change
bumps the Base cache version to 172. Complete result bindings/keywords and
resolved-ID intercepts remain open under #10461. Qualified single-argument
`Any` overloads that are rejected before this runtime path remain tracked
separately in #11622.

### Qualified REPL methods activate on the held VM (Issue #9784)

Main-owned source methods no longer rebuild the REPL VM merely because they use
`where` parameters or keywords. Brand-new definitions, new-signature extensions,
and same-signature replacements for ordinary, bounded/repeated-TypeVar,
multi-keyword/default, keyword-splat, positional-vararg, and combined
`where`+typed-keyword forms compile against the retained snapshot. The primary
method and its marker-specific transitive caller refresh slice install dormant
and publish atomically at `DefineEvalFunction`; catchable errors retain exactly
the reached source prefix and discard later methods. Every covered definition
asserts `last_vm_build_nanos() == Some(0)`.

Source-ordered keyword calls now use the existing world-aware function-value
dispatch when a later method is dormant. A caller compiled before a replacement
therefore selects the visible prior method until the callee marker runs, while
its refresh body selects the replacement afterward. Eligibility is semantic
(Main ownership); relocatable extraction independently verifies function indices,
specialization rows, closure targets, global slots, and activation membership.
Base/preload-owned methods plus the remaining type/module/import/macro/opaque-eval
families stay fail-closed on the full fallback under Issue #9784.

### Runtime callable paths use one production resolver (Issue #10461 Phase 1a)

Stored functions, callable structs, `DataType` constructors, HOF callbacks,
qualified runtime calls, and splat opcodes now select methods through
`dispatch_function_variable_for_values`. The shared resolver tries concrete
value-aware scoring first, distinguishes no-match from ambiguity, and consults
the legacy string scorer only after a miss. Parametric constructors enter the
same boundary but retain an explicit legacy bridge until the full callable
`Type{...}` head participates in one signature comparison (#11610). `invoke`
remains a separate declared-signature mode; function-variable and keyword forms
now preserve a declared `Any` instead of refining it to the runtime type
(#11609). The three `CallFunctionVariable*` opcodes no longer contain private
scorer ordering, and the source audit plus two mutation controls enforce shared
routing, real call detection, and value-before-legacy ownership. Compile-time
direct calls, complete bindings/keywords, and resolved-ID intercepts remain
open under #10461.

### AoT gate binary lookup now follows Cargo's target directory (Issue #11598)

`scripts/test_aot.sh` and the AoT, metamorphic, and fixture-parity helpers now
preserve explicit `SJULIA_BIN` / `JULIARS_BIN` overrides and otherwise consume
`CARGO_TARGET_DIR/release/{sjulia,juliars}`. Relative target directories resolve
from the repository root, while the unset default remains `target/release`.
This keeps Cargo producers and downstream consumers aligned in shared-target
worktrees. The registered source-only audit executes default, external absolute,
relative, and explicit-override cases, and its negative self-test restores a
fixed `$ROOT/target/release` lookup to prove the regression is rejected. After
adversarial review, the audit discovers all `aot_*.sh` and fixture-parity
wrappers (11 consumers), requires one authoritative assignment per binary, and
also rejects a later `${ROOT}` fixed-path reassignment that leaves the expected
assignment token intact.

### Structured call-resolution comparison boundary (Issue #10461 Phase 0)

Added the shared `CallRequest -> ResolvedCall` vocabulary with structured
callee, argument, keyword-origin, lexical scope, world, source-span, candidate,
target, and TypeVar-binding identities. The default-off
`SJULIA_CALL_RESOLVER_COMPARE=1` adapter runs at the callable-value/HOF dispatch
boundary and compares the production callable scorer with the VM runtime
selector on the same request. It emits only differences and always returns the
already-computed production result, so it cannot change selection. The
qualified/stored/HOF/specialized/parametric-constructor comparison corpus is
clean. `CALL_RESOLUTION.md` inventories all call entry points and connects the
specializer, constructor-owner, and compile-time shadowing audits. This is the
completed comparison/inventory foundation; production routing and complete
binding capture remain open under #10461.

### Compile-expression shadow audit recognizes InternedStr guards (Issue #11602)

The bare-name shadow audit now accepts the canonical
`contains_key(name.as_str())` and `contains(name.as_str())` guard forms used
after `Expr::Var` migrated to `InternedStr`. Previously it recognized only a
direct `name` argument and falsely rejected 20 guarded sites on clean `main`.
Brace-region tracking, explicit annotations, fail-loud zero-match behavior,
and the injected unguarded negative control remain unchanged.

### Structured type semantics no longer depend on display reparsing (Issue #10460)

`CoreType` is now the canonical semantic carrier across inference tuple
`typejoin`, runtime `_typeintersect`, type-object reflection, and method-cache
serialization. Structural conversions preserve qualified nominal owners,
ordered `UnionAll` binders, dependent bounds, and bound-versus-free `TypeVar`
identity; diagonal intersection substitutes only the intended binder identity,
and context-free nominal `typejoin` follows upstream by widening unrelated user
families to `Any`. The generated upstream parity corpus covers nested and
same-name binders, dependent bounds, partial application, value parameters,
and alpha-equivalent wrappers. The type-representation audit now pins the exact
normalized semantic-site inventory with a same-count-substitution negative
control, preventing textual parse sites from being exchanged invisibly.

### Splatted vararg forward into a runtime type-application curly now resolves (Issue #11539)

`T{A,B,C,expr}(xs...)` — splatting the same vararg collection forward from a
lower-arity parametric outer constructor into the fully-parameterized one,
where `expr` is a runtime `where`-bound type variable or an inline call such
as `length(xs)` — raised `UndefVarError: T{A,B,C,expr} not defined`; the
identical call without the splat (`T{A,B,C,expr}(xs)`) already worked. Two
gaps compounded: (1) `compile_call` branched into `compile_splat_call` before
ever reaching `try_compile_parametric_constructor_call` whenever the call had
a splat argument, so the whole curly text was mis-resolved as an undefined
variable name; (2) the VM's `CallFunctionVariableWithSplat` handler's callee
match only covered `Function`/`Closure`, missing the `DataType`/`Struct`/
`StructRef` arms its non-splat sibling `CallFunctionVariable` already had (the
vararg-binding code further down already assumed a `DataType` callee could
reach it — it just never did). Added
`try_compile_splat_parametric_constructor_call` (builds the runtime `DataType`
via `emit_parametric_type_arg_value` + `ApplyTypeDynamic`, then invokes it
through the splat-aware call convention) and completed the VM callee match to
reuse the existing `collect_runtime_callable_candidates` dispatcher shared
with the non-splat path. The new check runs BEFORE
`owned_constructor_name_in_scope` in `compile_call` (it returns `Ok(None)` for
any non-curly name, so a plain `Owner.Foo(xs...)` forward still falls through
unchanged) — `owned_constructor_name_in_scope`'s own target,
`compile_runtime_datatype_value_call`, eagerly resolves type arguments through
`resolve_instantiation_with_type_expr`, which rejects a `where`-bound runtime
type variable outright, so a module-qualified struct (e.g. `M.Foo{M,N,T,n}
(xs...)`) hit the same bug the top-level case did. Fixture
`dispatch_splat_vararg_forward_type_apply_11539` asserts the resulting type
and the untouched field content (flat, not nested) against upstream for a
local-variable trailing value parameter, an inline-call one, and a
module-qualified struct.

### apply-type TypeError carries its typed payload (Issue #11399)

`x{T}` where `x` is a value (not a type) now raises a TypeError whose
`.func` is `Symbol("Type{...} expression")`, `.expected` is `UnionAll`,
and `.got` is the value itself — matching upstream — instead of the
`:unknown`/`nothing` placeholders the VM funnel emitted. Same
message-keyed side-channel as the DomainError/MethodError work: a new
`pending_type_error_payload` field + `type_error_with_payload` helper,
consumed once by the exception funnel. Other TypeError paths (typeassert,
raised as pure-Julia struct throws) keep their own real fields. Fixture:
`exceptions/exceptions_typeerror_applytype_11399.jl`. Completes the
VM-raised error-payload restoration for tech-debt #11399
(MethodError/BoundsError via #11374, DomainError.val, and now
TypeError-apply-type).
### A splat/kwargs call to a conditionally-defined function now honors runtime visibility (Issue #11320)

A method defined inside an untaken top-level branch (`if cond; f(x)=x; end`
with `cond == false`) must stay undefined at runtime, matching a direct call
to the same name. Root cause was two-fold: `compile_main`'s eager top-level-
definition activation drain (`top_level_definition_activations`) treated any
function found by scanning statements — including one nested inside an
untaken `if`/zero-iteration loop branch — as unconditionally reached by
source position, activating it (`DefineEvalFunction`) regardless of whether
its own enclosing branch ever executed; and the positional-splat dynamic
call path manufactured a callable token via `PushFunction` with no
visibility check at all, evaluating the splat argument before ever
consulting the callee's existence. Fixed by excluding statement-nested
function definitions found inside `if`/`while`/`for`/`try` bodies from the
eager drain (`compile_stmt`'s own branch-gated `Stmt::FunctionDef` handling
already activates them correctly), and by adding a new
`Instr::RaiseUndefVarErrorIfFunctionInvisible` guard emitted before a
keyword-free splat/positional dynamic call evaluates its arguments — mirrors
upstream's callee-before-arguments evaluation order (verified via
`Meta.lower`; a kwargs-bearing call legitimately evaluates its arguments
before the callee upstream and is unaffected). The `CallFunctionVariable`/
`CallFunctionVariableWithSplat`/`CallFunctionVariableWithKwargsSplat`/
`call_runtime_callable_value`/`invoke_runtime_callable_value_with_signature_and_kwargs`
dispatch paths now share one `Vm::function_name_exists_but_invisible`
decision, raising `UndefVarError` instead of a `MethodError`/"not found"
`TypeError` when every `FunctionInfo` under a bare callee name is currently
outside the dispatch world. Residual gaps in the same visibility-
centralization family (siblings #11286/#10461) — `CallSpecialize`'s direct-
call path never checking world visibility, a stored function VALUE's
compile-time `candidate_indices` hint bypassing the world filter, and the
identical drain bug on the struct/`DefineEvalStruct` side — are filed as
Issue #11581. Fixture:
`exceptions/conditional_branch_splat_call_visibility_11320.jl`.

### getfield BoundsError carries the real receiver and 1-based index (Issue #11509)

`getfield`/`_getfield` out-of-range index on a Rust-backed composite carrier
(`Expr`, `Base.Generator`, `RegexMatch`, ...) previously produced
`BoundsError(nothing, <off-by-one index>)`: the shared
`VmError::FieldIndexOutOfBounds -> BoundsError` conversion had no receiver
`Value` and re-derived the message from the internal 0-based `field_idx`
instead of the caller's 1-based index. Fixed at the shared conversion: the
`Getfield`/`_Getfield` builtin dispatch parks the receiver via
`Vm::field_index_out_of_bounds_with_receiver` atomically with each raise —
keyed by the exact `(index, field_count)` pair — and the funnel consumes it
unconditionally (mirroring `pending_domain_error_val`, Issue #11399) to
build `.a`/`.i`; the raise sites report the caller's original index instead
of `field_idx`. `Base.Generator`'s own `generator_projected_field_by_index`
no longer raises inline with no receiver — it returns `Option<Value>` like
`RegexMatchValue::field_by_index` (Issue #11382) so out-of-range indices flow
through the same shared arm. Fixture
`exceptions_getfield_boundserror_payload_11509` covers `Expr` and
`Base.Generator`, verified against upstream `julia` 1.12.6.

An adversarial re-review of this same fix caught a cross-contamination
regression before merge (the non-transactional pending side-channel bug
class from Issue #9787): an earlier draft parked the receiver
unconditionally *before* the field lookup, including on a getfield that
succeeds, so a stale receiver from a successful getfield could attach to a
later, unrelated `FieldIndexOutOfBounds` raised through a path that never
parks one (e.g. `setfield!` with an out-of-range index), misreporting the
wrong object. Fixed by moving the parking to the exact raise site (the
`field_index_out_of_bounds_with_receiver` helper above) instead of ahead of
the lookup, and keying it by `(index, field_count)`. The fixture's third
`@testset` reproduces the contamination (successful `getfield` on one
object, then an out-of-range `setfield!` on a different one) and fails on
the pre-fix code; while writing it, `setfield!`'s own `BoundsError` was
found to not thread a receiver at all — a separate, narrower, pre-existing
gap outside this issue's `getfield`-only scope, tracked as Issue #11596.

### REPL concrete types activate in source order and retain only the reached prefix (Issues #9784 / #11546)

Live-appended concrete structs are now reserved in a private type-registry tail
during compilation and exposed to Julia/runtime lookup only when their
source-ordered `DefineEvalStruct` marker executes. `PushDataType`, `NewStruct`,
`NewStructSplat`, and the typed fused-return path reject an unreached type with a
catchable `UndefVarError`, so called function bodies and forward references cannot
observe a dormant suffix. If a later top-level operation raises a runtime error,
the VM, reusable compiler snapshot, and session mirrors commit exactly the reached
prefix from one interleaved function/type activation trace and discard unreached
reservations. Runtime `eval(:(struct ... end))` activates at its marker and remains
immediately constructible. Base cache version 165. Abstract, primitive, enum,
parametric/inner-constructor, and module forms plus the remaining fallback
retirement remain follow-up slices of Issue #9784.
### DomainError carries its out-of-domain .val (Issue #11399)

A DomainError raised by a VM-internal numeric op (`sqrt` of a negative
real, f64 and BigFloat) now exposes upstream's `.val` — the actual
out-of-domain value — instead of a `nothing` placeholder. Same
message-keyed side-channel pattern as the MethodError/BoundsError payload
work (#11374): a new `pending_domain_error_val` field + `domain_error_with_val`
helper, consumed exactly once by the exception funnel. The four sqrt
DomainError raise sites (builtins_math.rs / arithmetic.rs, f64 and
BigFloat) carry the value; user-thrown `DomainError(val, msg)` /
`DomainError(val)` keep their explicit val (struct throw, unaffected).
Fixture: `exceptions/exceptions_domainerror_val_11399.jl`. Continues
tech-debt #11399 (the same payload-restoration line as #11374).
### Finalized specialization-disable flags survive cache restore (Issue #10334)

The fresh compiler now captures its final method-table decisions for array
`getindex`, array `setindex!`, and field-access specialization in a single
`CompiledProgram::specialization_disable_flags` snapshot. In-memory clones,
whole-program serde (`.sjvmbc` and manual restore), and the sectioned Base-cache
format carry that same snapshot; context restoration copies the flags directly
instead of approximating them with a top-level IR scan. The regression corpus
defines overrides inside a module and uses an alias-typed `Vector` receiver, so
all three flags must be enabled and must match exactly across fresh, manual, and
`.sjvmbc` contexts. This resolves #10334 without changing the compile-context
activation predicate. Seeded `PROGRAM_CACHE` context hydration remains #10335,
and promotion-registry/`main_scope_names` hydration remains #10339.

### Base/stdlib 型の const alias をパラメータ注釈で dispatch (Issue #11113)

`const MyPair = Pair; f(x::MyPair) = 1` は `Struct("MyPair")` という誰も
instance にならない placeholder へ method 登録され `MethodError` になっていた。
#11104 の lowering alias gate (`is_likely_type_name`) はプログラムが宣言した型名
と固定 builtin リストしか認識せず、`Pair` は Base 内で宣言される (Base は隔離
された lowering pass、または Base cache 使用時はソースから lowering されない)
ため gate に届かない。compile 層で `struct_table` (Pure-Julia struct: `Pair`,
`VersionNumber`, ...) と compiler-visible builtin-type registry (`struct_table`
に entry を持たないネイティブ VM 型: `Regex`, `UnitRange`, ...) の両方を使い、
lowering 後も未登録なバインディングを解決するようにした。両者とも両方の
cache mode (cold-compile / Base-cache-restore) で利用可能。fixture は
`types/const_type_alias_of_base_struct_annotation_11113.jl`
(Pair/Regex/UnitRange/VersionNumber、module-local alias、parametric use、
#11104 の builtin/program-declared alias control を再確認)。

### Multi-type-param outer-ctor arity miss no longer reproduces for #10592, new sibling gap filed (Issue #10592, #11549)

Verified against up-to-date `main`: PR #11476's fix for Issue #11404
(explicit where-parametric outer constructors participating in dispatch
without suppressing the default field constructor) also fully resolves
Issue #10592's default-ctor-after-miss class for single-type-param structs
— all three MWE shapes from the issue (direct call, bound-callable `f =
B{Int64}; f(7)`, self-referential outer body) now construct correctly
instead of crashing. The `parametric_ctor_callable_parity_10502.jl`
prevention fixture's previously-deferred negative guards
(`CtorAuditBox10502{Int64}(7)`, `CtorAuditSib10502{Int64}(7)`, direct and
bound-callable) are now asserted instead of documented-but-untested.
Multi-type-param structs (2+ fields) with a concrete outer constructor
whose arity differs from the default field constructor's arity have a
*different*, still-open crash (uncatchable compile error or runtime
`InternalError`, filed as Issue #11549) — out of scope for this fixture
strengthening.

### `ConstructParametricType` validates parameter values, raising TypeError instead of silently degrading to `Any` (Issue #11555)

sjulia had two divergent parametric-type-construction paths: the dynamic-base
path (`T{x}`, `Core.apply_type`, `apply_type_to_runtime_base`) already
validated each argument via `type_arg_value_to_julia_type` and correctly
raised `TypeError` for an invalid value (an `ErrorException` instance, a
`String`, ...), but the literal/compile-time-known-base path
(`Vector{e}`, `Instr::ConstructParametricType`) called `build_parametric_type`
directly with no such check, so an unrecognized parameter value silently
became the `Any` placeholder (`Vector{Any}`) instead of erroring. Added
`is_valid_type_param_value`, mirroring upstream `jl_valid_type_param`
(`julia/src/builtins.c`): a `Type`/`TypeVar`/`Symbol`/`Module`, an isbits
scalar, an all-isbits `Tuple`, or a struct instance whose definition
`is_isbits_with_struct_defs` — this keeps `Complex`/`Rational`/plain isbits
user structs and `nothing`/`missing`/`Module` accepted exactly as before
(their rendering already fell back to the pre-existing `Any` placeholder;
a struct-value type-param *renderer* is a separate, out-of-scope concern),
while rejecting a genuinely invalid value like `ErrorException` or `String`.
A non-isbits `Number` (`BigInt`/`BigFloat`) raises with `expected Int64`,
matching upstream's `jl_isa(pi, Number)` branch. A bare named `Function`
(no captures) and an `@enum` value are always isbits upstream; a `Closure`/
`ComposedFunction` is isbits iff every capture / wrapped function is —
checked directly here (not through the shared `isbitstype` builtin
machinery, which does not yet classify these kinds at all, a pre-existing
gap filed as Issue #11589) to avoid a false-positive regression
(`Vector{sin}` must keep NOT raising). `build_parametric_type` now returns
`Result`, shared by `ConstructParametricType`, `ConstructParametricTypeSplat`,
and `apply_type_to_runtime_base`'s own concrete-base fallthrough. Fixture:
`types/construct_parametric_type_invalid_param_typeerror_11555.jl`
(`Vector{e}` raises `TypeError`; `Vector{7}` keeps working, Issue #4644
regression guard; `Complex`/`Rational`/`nothing`/`missing`/`Module`/a bare
`Function`/an `@enum` value/a `ComposedFunction` keep NOT raising; `BigInt`/
`BigFloat` raise with `expected Int64`).

## 最新対応 (2026-07-17)

### AoT reduced numeric matrix: div-family extended to non-Int64 widths (Issue #9687 slice 3)

`scripts/aot_numeric_matrix_reduced.sh`'s supported slice grows from 85 to
105 rows. Issue #10131 added the missing I8/I16/I128/U8..U128 `Value`
variants to the AoT runtime and made the div-family (div/fld/cld/rem/mod)
codegen box through `Value::from(...)` only when the recorded return type is
an actually-boxed slot, keeping native results native otherwise; this
comparator now exercises the same signed-integer tower already covered for
arithmetic/comparison (`Int8`/`Int16`/`Int32`/`Int128`), adding 20 rows.
`docs/vm/NUMERIC_MATRIX_AOT_REDUCED_SKIPLIST.tsv`'s catch-all count shrinks
5117 → 5097 by the same amount. UInt8/16/32/64/128 div-family cells compile
in AoT now too (Issue #10131), but stay skiplisted here because the oracle's
`repr()` value (hex) still diverges from the probe's `string()` value — a
separate, still-open gap from the div-family codegen fix. isless / mixed-type
min-max also compile in AoT after Issue #10131, but are not yet wired into
this comparator's `supported()`/`key_for()`; left for a future slice.

### A catch binder now shadows a type alias inside composite signature annotations (Issue #11321)

`catch T` introduces a fresh lexical binding for `T`, but lowering's
whole-program type-alias pre-scan (Issue #5055, deliberately source-order
independent) never descends into `try`/`catch` bodies, so a composite
annotation (`Vector{T}`) referencing the shadowed name still froze to an
outer `const T = Int64` alias — the method definition silently succeeded
with a non-Type runtime binder value instead of raising `TypeError` like
upstream. A same-clause reassignment to a resolvable type before a
definition (`catch T; T = Int64; f(x::T) = 99`) was likewise invisible to
signature lowering, so the valid case incorrectly raised `MethodError`.

Fixed with two coupled parts. Lowering (`control_try.rs`) now registers a
lexically scoped shadow when lowering a `catch NAME` clause, using the
existing alias-table registration/visibility primitives
(`register_prescanned_non_alias`, `register_prescanned`,
`AliasScope::snapshot`/`.restore`) — a non-alias tombstone for `NAME` at the
clause's own start position, plus a real alias entry at its own position for
each direct-child same-clause reassignment, discarded the moment the clause
finishes lowering (every exit path, including error) so nothing leaks past
the clause's `end`. Compile-time (`emit_signature_definition_probes`) gained
a companion pass: recurse into composite annotations collecting
bare-identifier leaf names, and for a name that is a CURRENT runtime local
(`self.locals` AND `self.initialized_locals`, e.g. an active `catch` binder)
validate it the same way upstream validates ANY parametric-type argument — a
Type/TypeVar, a `Symbol`, or an `isbits` value are all legal (`Vector{7}` is
a real upstream `DataType`, not a `TypeError`) — by routing through
`ApplyTypeDynamic`'s existing `type_arg_value_to_julia_type` classification
(the same authority a dynamic-base application `T{x}` already uses) with a
fixed `Vector` head, discarding the result. An earlier revision of this probe
emitted a bare `name <: Any`, which demands the value literally BE a Type;
that wrongly raised `TypeError` for an upstream-legal isbits type parameter
like `x = 7; q2(v::Vector{x}) = 1` — verified as a regression against both
upstream and pre-existing sjulia behavior for that shape (caught during
review, before merge). The `initialized_locals` gate keeps this out of
#11114/#11118's separate forward-reference probe territory. Fixture:
`exceptions/catch_binder_signature_shadow_11321.jl`
(`exceptions_catch_binder_signature_shadow_11321`): both original MWEs, a
no-leak control, and the isbits-parameter non-regression case.

### Caught exceptions carry typed payloads (Issue #11374)

Caught exceptions now expose upstream's payload fields. `BoundsError`
carries the actual container and the complete index tuple (`A[10]` →
`.i == (10,)`, `M[9, 9]` → `(9, 9)`, with upstream `at index [9, 9]`
rendering; pure-Julia raise sites tuple-ize and the VM funnel converts
`IndexOutOfBounds` indices to a tuple). `MethodError` carries the real
callable and argument values through a message-keyed
`pending_method_error_payload` side-channel consumed exactly once by
the exception funnel: the compile-time dispatch-miss sites emit the new
`ThrowMethodErrorWithArgs` builtin (wire ID 317, CACHE_VERSION 161)
keeping the argument values on the stack instead of Pop+ThrowMethodError,
the named numeric fast paths (sqrt, both handlers) park the payload at
their remap sites, and four runtime dispatch-miss sites build the error
through `method_error_with_payload`. Rendered messages are unchanged
(`showerror` renders a Function payload by `nameof`). Remaining
string-only lanes (const-propagated alias calls and other builtin fast
paths) stay tracked under tech-debt #11399. Fixture:
`exceptions/exceptions_payload_fields_11374.jl`.
### finally-scoped rethrow() no longer swallows the unwinding exception (Issue #11306)

A `finally` block whose own body catches an explicit `rethrow()` with a
nested `try`/`catch` used to prevent the exception that unwound into the
`finally` from ever reaching the enclosing `catch`. Root cause: the
"must re-propagate at the end of this finally" state was a single scalar
(`rethrow_on_finally`) that any nested handler routing -- including the
nested catch's own `ClearError` -- unconditionally clobbered. Replaced
with a depth-aware stack, `Vm::pending_finally_rethrows`
(`subset_julia_vm_vm/src/vm/mod.rs`), pushed by `handle_error`
(`subset_julia_vm_vm/src/vm/state.rs`) only when routing into a
finally-only handler, and truncated to each popped `Handler`'s recorded
`finally_pending_len` so a scope nested inside a finally can never see,
let alone clear, an enclosing finally's marker. The compiler-emitted
trailing `Instr::Rethrow` (`subset_julia_vm_vm/src/vm/exec/error_handling.rs`)
pops its own marker to resume propagation, restoring the original
exception value/backtrace so the outer `catch` binds the correct
exception. Fixture: `exceptions/finally_rethrow_swallow_11306.jl` --
the MWE, a nested catch that re-throws again, a double-nested finally,
and an unrelated exception fully handled inside the finally (verified
against Julia 1.12.6; Issue #11281's catch-clause-cleanup fixture stays
green).
### Exact-or-Any constructor identity audit (Issue #11436)

Class-level prevention for #11434 (a latent same-base first-match over
the hash-backed `StructRegistry` sharpened an `Any` constructor return
HashMap-seed-dependently). The invariant — constructor return identity
is either EXACT (owner plus complete type parameters) or stays `Any`;
same-base scans may enumerate but never establish identity — is
documented in docs/vm/CODE_AUDITS.md, and the new
`check_struct_registry_first_match.sh` audit discovers every `.iter()` +
find/find_map/position/next chain over the hash-backed struct
registries and requires a reviewed classification (unique-guarded /
exact-key-equivalent / enumeration) in its inventory; unclassified new
sites fail. Registered in CI, source_only_audits.tsv, and the negative
self-test framework (injected same-base scan must trip the
inventory-drift failure). The last order-dependent site (the two-key
`iter().find` in `try_struct_field_count_default_ctor_fallback`) is
rewritten as ordered exact-key probes (`struct_table.get`). Part of
tech-debt #11447.
### Inference-global types survive cache restore (Issue #10333)

The finalized fresh-compile map—precise for const bindings and widened to
`Any` for mutable globals—is now persisted as the name-sorted
`CompiledProgram::inference_global_types_snapshot`. Whole-program serde
(`.sjvmbc` and manual restore) and the sectioned Base-cache format carry the
same snapshot, and restore rebuilds the transient runtime compile context from
it instead of installing an empty map. A reflection regression pins
`Base.infer_return_type` / `Base.return_types` parity for const and mutable
globals, and the #10462 scoreboard now requires exact fresh/manual/`.sjvmbc`
equality. Base cache version 162; `.sjvmbc` version 5.

### Preserve the reached REPL function-definition prefix (Issues #9784 / #11477)

Live-appended function bodies now remain dormant until their source-ordered
`DefineEvalFunction` marker executes. If a later top-level operation raises a
catchable runtime error, the VM method world, reusable compiler snapshot, and
fallback replay state commit exactly the reached definition prefix. Reflection,
direct and dynamic calls, forward references, and IR inlining cannot expose the
dormant suffix; overloads of one generic retain only the methods actually
reached. Thus `f() = 1; error(...); g() = 2` leaves `f` callable, keeps `g`
undefined, and reuses the same live VM on the next evaluation. Type-definition
deltas and indirect definition-world changes through runtime `@eval` retain the
conservative drop boundary for the next Issue #9784 slice.
### Comprehensions dispatch on their runtime element type (Issue #10315)

An assigned one-dimensional comprehension with a statically unresolved body
no longer becomes a falsely proven `Vector{Any}`. When collection uses runtime
type-join, the compiler now preserves known rank with an unresolved element
(`ArrayOf(_, Some(1))`) across both the indexed and iteration-protocol paths;
tuple-destructuring also projects its internal empty-`Union{}` collector
sentinel to that unknown-element representation. The existing dispatch policy
therefore defers competing `Vector{Any}` / `Vector{T}` methods until the
concrete runtime vector exists, without changing the matcher. The parity
fixture covers range, Set, tuple, heterogeneous, and explicit-`Any` forms, and
a consolidated bytecode regression requires `CallDynamic` over both vector
overloads (prevention Issue #11513).
### Slot-backing dominance verifier (Issue #10820, prevention for #10819/#7556)

Added `subset_julia_vm_compile/src/compile/slot_backing_verifier.rs`, a
test-only dominance/dataflow bytecode verifier for the exact invariant
#10819 broke: every local read (`LoadSlot`/`LoadAny` and the paired
name-keyed/index-keyed Load/Store instruction family) must be dominated
by a store on every reachable predecessor path from function entry — a
forward "must" dataflow over the CFG (meet = intersection, transfer =
union with the block's own stores). It carries a negative self-test that
reproduces the pre-fix #10819 shape (a `Nothing` local compiled to a bare
`Pop`, widened to `Any` in only one `if` branch) and confirms the
verifier flags the non-assigning merge path for exactly that reason, plus
a positive test proving the landed fix satisfies the invariant. Extended
`cfg.rs` to model `PushHandler(catch_ip, finally_ip)` as real CFG edges
(the block containing it now splits and gains edges to `catch_ip`/
`finally_ip` in addition to fallthrough) so the verifier can correctly
distinguish a store made *before* a `try` (dominates the `catch`/
`finally` block) from one made only *inside* the protected region (does
not dominate, since an exception can fire before it runs) — this closes
the #10820 DoD item to extend the control-flow matrix to `try`/`catch`
and zero-iteration-loop widening, both covered by dedicated unit tests
(8 total, all green) and by a new upstream-verified fixture
`control_flow/nothing_initialized_trycatch_loop_widen_10820.jl` (8/8
assertions, parity with `julia`). The verifier is not wired into the
production compile/VM pipeline — it costs nothing at runtime.

### Constructor reflection resolves structurally (Issue #11402)

`Base.infer_return_type` / `Base.return_types` on a DataType callee now
resolve structurally instead of widening through function-name
reflection: an applied parametric spelling (`S{Int64}`) constructs
exactly itself (field layout resolved through the runtime type registry,
materialization-independent), a bare family infers its type parameters
from the concrete argument types (reusing the runtime dynamic
constructor's `infer_parametric_type_args` unifier), and unmatched
arities report `Union{}`. Explicit inner constructors' applied
spellings resolve too. Ordinary function reflection is untouched.
Fixture: `reflection/reflection_constructor_return_types_11402.jl`
(12 assertions). Part of tech-debt #11447.
### Bundled StaticArrays models the upstream four-parameter SMatrix (Issue #11432)

`struct SMatrix{M,N,T} <: StaticMatrix{M,N,T}` in
`subset_julia_vm/packages/StaticArrays/src/SMatrix.jl` now declares the
upstream fourth length parameter (`struct SMatrix{M,N,T,L} <:
StaticMatrix{M,N,T}`, `L == M*N`), matching the canonical
`SMatrix{S1,S2,T,L} = SArray{Tuple{S1,S2},T,2,L}` alias shape from real
StaticArraysCore. A field annotated `W::SMatrix{2,2,Float64,4}` (the synced
IFS fractals sample's affine map matrix) no longer raises `too many
parameters for type StaticArrays.SMatrix`, a regression exposed once
synthetic default-constructor validation became Julia-compatible (#11358).
`SMatrix{M,N,T}` (and narrower spellings) remain constructible via
incomplete parameterization — sjulia's existing partial-`UnionAll`
application/dispatch generalizes to the added trailing parameter with no
other `StaticArrays`/`Rotations` dispatch-site signature changes needed.
Two Rust-side fast paths that recognize `SMatrix` by parsing its
display-name string needed updating for the new `"SMatrix{M, N, T, L}"`
shape: `try_make_static_array`
(`subset_julia_vm_vm/src/vm/exec/struct_ops.rs`, the small-array
inline-representation intercept) and the `StaticArrayInlineData` /
`StaticRealValue` type-name tables plus the `elem_type_str` element-type
extractor (`subset_julia_vm_bytecode/src/value/static_real.rs`), both of
which previously assumed exactly three curly parameters. `W::SMatrix{2,2,
Float64,4}` restored in all three synchronized IFS sample copies (`mobile/`,
`SubsetJuliaVMApp/.../Resources/Samples/`,
`CodeSamples+Intermediate.swift`); the W-73 workaround entry retired.
Fixture: `static_arrays/static_arrays_smatrix_four_param_11432.jl`.
### IterateDynamic family-fallback resolvers gain nominal-origin fencing (Issue #10879)

Prevention follow-up of #10295/PR #10877: auditing the "duplicated
applicability checks across dispatch selectors" blast radius against a live
MWE found that the `IterateDynamic` family-fallback resolvers
(`resolve_iterate_struct_family_fallback`,
`resolve_runtime_iterate_struct_family_fallback`, and the `scored` candidate
list feeding `resolve_scored_family_fallback`) never called the shared
origin-aware predicate (`function_candidate_has_nominal_origin_conflict`,
Issue #10295) that the metadata-backed runtime scorer, `CallTypedDispatch`
replay, legacy function-value dispatch, and compile-time `MethodTable`
dispatch already consult. A same-named external struct with no `iterate`
method of its own could reach Base's `iterate(p::Base.Iterators.Partition)`
body through the looser `runtime_core_family_fallback_matches` matcher and
then crash with a `BoundsError` reading a field the external struct's layout
does not have. A new `origin_safe_iterate_candidates` helper fences the
candidate set once, before any of the three resolvers see it, routing
`IterateDynamic` through the same shared API as the other selectors.

Added a table-driven selector-parity fixture
(`modules/dispatch_selector_origin_parity_10879.jl`) that feeds the same
Base/user same-name struct pair through direct, function-value, Any-boxed
dynamic, `iterate()`-protocol, splat, kwargs, and repeated-call-site
(inline-cache replay) dispatch, plus positive rows for a same-ID Base
submodule alias and an external subtype of `Base.AbstractDisplay`; and two
Rust unit tests (`dispatch_selector_origin_parity_table_issue_10879`,
`nominal_origin_conflict_core_api_abstract_and_union_rows_issue_10879`)
exercising the shared runtime predicate and the core API's abstract/Union
rows directly. Reverting the `call_dynamic.rs` fence reproduces exactly the
two `BoundsError` assertions failing in the new fixture (negative self-test).
Documented the type-only `IteratorSize`/`IteratorEltype` generator-trait
selector's deliberate non-fencing (it dispatches on function name only,
against a VM-synthesized, non-user-instantiable `Generator` wrapper with no
nominal identity to erase) in `docs/vm/GENERATOR_REPRESENTATION.md`.
Incidentally found (and filed, not fixed here) two unrelated pre-existing
gaps: `IterateDynamic`'s "no candidate" fallback raises `TypeError` instead
of upstream's `MethodError` (#11527), and a differently-named external
subtype of `Base.AbstractDisplay` fails `pushdisplay` dispatch entirely
unless its own bare name happens to equal `"AbstractDisplay"` (#11528).
### abs2's real-number fallback is now typed `::Real` (Issue #10602)

`abs2("a")` silently returned `"aa"` instead of raising a `MethodError`:
the real-number fallback in `base/number.jl` was an untyped
`function abs2(x)`, so any argument — including `String` — matched it
and `x * x` string-concatenated. Retyped to `abs2(x::Real) = x*x`
matching upstream `julia/base/number.jl:189`; `abs2("a")` now raises a
catchable `MethodError` like upstream. The typed `Complex{T}`/
`Complex{Float32}`/`Complex{Float64}` methods in `complex.jl` (Issue
#10775) keep dispatching to their own concrete methods unaffected — the
new fixture checks both real and Complex `abs2` in the same run and
passed three consecutive fresh-process runs. Fixture:
`numeric/abs2_string_methoderror_10602.jl`.

### Explicit where-parametric outer constructors take precedence (Issue #11404)

A source-written explicit parametric outer constructor
(`ExplicitOuterGap{T}(x::T) where {T} = nothing`) now participates in
dispatch even when the call is also shaped like the automatic field
constructor: the fully-applied fast path consults the static parametric
resolver whenever a `Base{T}`-shaped method table has a matching-arity
method, so the user method replaces the synthetic default inner as
upstream does. Non-matching signatures, different arities, and
delegating outers keep the default field constructor reachable. Fixture:
`struct/struct_explicit_outer_precedence_11404.jl`. Part of tech-debt
#11447.
### nameof(::Module) reflection support (Issue #11171)

`nameof(m::Module)` no longer raises `MethodError` — a new
`nameof(m::Module) = _module_name(m)` pure-Julia method (mirroring the
existing `nameof(::Type)`/`nameof(::Function)` internal-intrinsic
pattern) backs it with a `_ModuleName` `BuiltinId` that returns the
module's own unqualified binding name as a Symbol, not the qualified
`Owner.Name` path a nested module's `ModuleValue.name` carries
internally (the same last-path-component extraction `names(m::Module)`
already used). Verified for a nested user module, `Main`, `Base`, and
through both a bare dynamic-dispatch call (`g(m) = nameof(m)`) and a
`::Module`-annotated argument. Fixture:
`modules/nameof_module_11171.jl`.

### Concrete Complex element types no longer cross-match (Issue #10775)

The canonical MethodTable/CoreType dispatch path now rejects concrete
`Complex{Float32}` and `Complex{Float64}` parameters for a
`Complex{Int64}` actual argument. A regression table contains the bounded
`Complex{T} where T<:Real` method plus both concrete methods in all six
registration orders: Int64 always selects the generic row, while exact
Float32/Float64 actuals select their concrete rows. The original `abs2` MWE
and an independent three-method MWE each produced one correct output across
100 fresh sjulia processes, matching upstream Julia. Later shared-dispatch
work had already repaired the observed behavior, so closure required no new
resolver production change and does not restore the concrete binary overloads
removed by #10784; obsolete comments claiming the bug remained active were
removed. The all-order regression also completes prevention Issue #11492.
### RegexMatch / Base.Generator physical field projection (Issue #11382)

`fieldcount`/`fieldnames`/`getfield`/`propertynames` on `RegexMatch` and
`Base.Generator` no longer report zero fields. `RegexMatchValue` gained a
`regex: RegexValue` field (the match's originating `Regex`, upstream's 5th
physical field) plus a centralized `field_by_name`/`field_by_index`
authority (mirrors `BindingValue::field_by_name`) shared by dot-property
access (`exec/struct_ops.rs`), `getfield`/`_getfield`
(`builtins_reflection/mod.rs`), and the `jl_get_nth_field_checked`-style
iterate projection (`value_field_projection.rs`) so the three call sites
cannot drift apart. `CoreType::builtin_field_metadata` (matched on the
module/type-param-stripped bare name, so a fully parametric
`Base.Generator{UnitRange{Int64}, typeof(f)}` spelling resolves the same as
the bare name) now reports `RegexMatch`'s 5 fields (`match`, `captures`,
`offset`, `offsets`, `regex`) and `Generator`'s 2 fields (`f`, `iter`),
wiring `fieldnames`/`fieldcount`/`propertynames` (which delegates to
`fieldnames(typeof(x))`). Other Rust-backed composites named in the issue
(`BigInt`, `BigFloat`, RNG state, `DataType`, `Core.TypeName`, `IO`,
`Core.Binding`, `Regex` itself) remain explicitly fail-closed
(`VmError::NotImplemented`, tagged Issue #11382) rather than silently
reporting zero fields. Fixture:
`reflection/regexmatch_generator_field_projection_11382.jl` (36
assertions). Follow-ups: Issue #11509 (pre-existing, unrelated
`BoundsError` object/index-formatting bug discovered on `Expr` while
verifying the `getfield` out-of-range path) and Issue #11514
(pre-existing: `getfield(::Base.Generator, 1)` throws instead of
returning a callable for filtered/tuple-splat generators specifically —
`fieldcount` now reports 2 for those too, widening the practical
exposure).

### Abstract numeric fields preserve the runtime value (Issue #11407)

`struct S; x::Number end; S(1)` no longer exposes `1.0`: the field
storage tag mapping that collapsed `Number`/`Real`/`AbstractFloat` to
`ValueType::F64` (and `Integer`/`Signed`/`Unsigned` to `I64`) is widened
to `Any` at the field boundary by the new
`field_declared_value_type(_scoped)` helpers — upstream stores the
original `Int64` because `1 isa Number` requires no conversion. Direct
reads, prints, function-boundary loads, and mutable `setfield` all
preserve the original runtime value now. Fixture:
`struct/struct_abstract_numeric_field_11407.jl` (20 assertions,
Int64/Float64/Complex/Int8/BigInt/Float32 lanes). Part of tech-debt
#11447.

### Non-Int64 integer constructors accept Char (Issue #11406)

`UInt8('b')`, `Int8('b')`, `UInt16`/`UInt32`/`UInt64`/`Int16`/`Int32`/`Int128`/
`UInt128` of a `Char` now convert via the character's Unicode codepoint,
mirroring upstream `julia/base/char.jl`'s
`(::Type{T})(x::AbstractChar) where {T<:Union{Number,AbstractChar}} =
T(codepoint(x))`. Previously only `Int`/`Int64` had a Rust-boundary special
case for `Char`; every other fixed-width constructor fell through to a
generic `convert` `MethodError`. Added
`Int8/Int16/Int32/Int128/UInt8/UInt16/UInt32/UInt64/UInt128(c::AbstractChar)`
pure-Julia methods next to `Int(c::Char)` in
`subset_julia_vm/src/julia/base/strings/basic.jl`; since these route through
the existing `T(codepoint(x))` Number->Number constructors, the upstream
range check (`InexactError` for out-of-range codepoints, e.g. `UInt8('あ')`)
falls out for free with no new logic. `Int64(::Char)` is intentionally left
on its existing Rust boundary. Fixture:
`strings/char_integer_ctors_11406.jl`.

### Colon syntax keeps Base-owned dispatch (Issue #11444)

Range literals `a:b` are no longer hijacked when a user outer
constructor contaminates the bare `UnitRange` method table. Upstream
lowers unit-range colon syntax through `Base.:(:)` straight into the
parametric `UnitRange{T}(start, stop)` inner constructor, so it is
Base-owned: when `base_owned_dispatch_wins` detects that a non-Base
method would win the bare-table dispatch, the compiler now builds the
range through the inferred fully applied parametric spelling
(`UnitRange{Int64}`, ...) instead of the hijackable bare name.
`a:s:b` is deliberately unchanged — upstream's `_colon` calls the BARE
`StepRange(start, step, stop)`, so an imported user extension
legitimately intercepts step-range literals (pinned by the #11434
recovery regression test). Direct bare calls (`UnitRange(3, 4)`) still
reach the user extension, matching upstream 1.12's
unqualified-extension behavior. Fixture:
`range/range_colon_base_owned_dispatch_11444.jl`. Part of tech-debt
#11447.
### Insertion-ordered kwargs accumulation + NamedTuple merge dispatch for splats (Issues #11381 / #11383)

Runtime keyword-argument accumulation moved from a `HashMap<String, Value>`
to an insertion-ordered `KwargsMap<V>` (ordered entries + name-to-slot index)
threaded through keyword-splat preparation
(`subset_julia_vm_vm/src/vm/type_ops/iteration.rs`) and binding
(`subset_julia_vm_vm/src/vm/exec/call.rs`,
`subset_julia_vm_vm/src/vm/exec/call_function_variable.rs`,
`subset_julia_vm_vm/src/vm/builtins_macro/eval.rs`). `f(; z=1, a=2)`'s
`kwargs...` now reports `z, a` in call order instead of hash order, and a
duplicate key within one splat source replaces the value at its existing
first-occurrence slot instead of moving it or being lost to hash order
(Issue #11383). Keyword-splat source evaluation for a non-`NamedTuple`/`Pairs`
source now also routes through real `merge(::NamedTuple, source)` multiple
dispatch (`Vm::merge_kwarg_splat_source`, via the existing
`find_best_method_index` + `sync_splat_callable_step` primitives — no
struct-name string special case in Rust), so a user-defined
`Base.merge(a::NamedTuple, ::T)` extension and Base's
`merge(a::NamedTuple, b::Zip{I1,I2})` duplicate-key validation
(`ErrorException: duplicate field name in NamedTuple: "..." is not unique`)
actually run (Issue #11381). New `subset_julia_vm/src/julia/base/namedtuple.jl`
adds the degenerate empty-side `merge` methods and the `Zip` validation
method; the fully general non-empty two-`NamedTuple` runtime merge (when
neither operand's field-name set is statically known at the call site, e.g.
inside a function generic over `::NamedTuple`) remains blocked on a
runtime-parametric `NamedTuple{names}(values)` constructor gap, filed
separately as Issue #11494 (a call-site-static merge of two literal
NamedTuples, e.g. `merge((a=1,b=2),(b=3,c=4))`, is unaffected — it already
works via the compiler's `try_compile_named_tuple_merge` constant-fold fast
path). Fixtures: `kwargs_insertion_order_11383`,
`kwargs_duplicate_overwrite_in_place_11383`,
`kwargs_splat_user_merge_dispatch_11381`,
`kwargs_splat_zip_duplicate_key_merge_11381`.
### DispatchFirst Base function keeps its builtin fallback at dynamic call sites (Issues #10786/#10871)

`compile_generic_dispatch_call`'s single-`Any`-argument fallback arm (the tail
reached once a `DispatchFirst` Base function such as `isbitstype` has a user
method table but static dispatch misses) emitted a plain `CallDynamic` whose
only candidate was the user's method — so once a user defined e.g.
`Base.isbitstype(::Type{Box}) = false`, a call like `g(T) = Base.isbitstype(T);
g(Int64)` raised `MethodError` instead of falling back to
`BuiltinOp::Isbitstype` (confirmed bytecode:
`CallDynamic(usize::MAX, 1, [Method(user)])`, no builtin fallback). The arm now
reuses the existing `BuiltinOp -> (BuiltinId, ValueType)` conversion
(`type_object_dispatch_builtin_fallback`, already used by the
`has_runtime_datatype_arg` arm for the `typeof(x)` call-site shape) and emits
`CallTypedDispatchOrBuiltin` instead, so a dispatch miss reaches the Rust
builtin. `isbits`/`ismutable` have no registered `builtin_op` (pure-Julia
catch-alls, Issue #6738: `isbits(x) = isbitstype(typeof(x))`) and are
unaffected — they keep using the generic `CallDynamic` path, which is correct
since their fallback IS the Julia method. The original #10786 `isbits(1) ==
false` symptom is currently masked by an unrelated Base compile-time
optimization and does not reproduce on its own; the fixture
`pure_julia/base_layout_predicates_dispatch_first_3911.jl` continues to pass
unchanged. New fixture:
`pure_julia/base_isbitstype_dynamic_callsite_dispatch_first_10871.jl` (static /
dynamic-unannotated-parameter / `typeof` call-site shapes, both builtin
fallback and correct user-override selection, plus `isbits`/`ismutable`
dynamic call sites).
### Checked integer constructors reject out-of-range float/BigInt correctly (Issue #11214)

Two boundary defects in the checked `Int64`/`Int32`/`UInt64`/`Int128`/
`UInt128` constructors are fixed. (1) `Float64(2.0^63)` silently saturated to
`typemax(Int64)` instead of raising `InexactError`: the old range check cast
the float to the target integer type first (a Rust SATURATING cast on
overflow), then cast the saturated result back to float and compared against
the input — but `typemax(Int64)` is not itself exactly representable in
`f64` (the nearest representable value is `2.0^63`, the exact out-of-range
input), so the round-trip falsely matched. The fix applies an explicit range
predicate directly to the original float value —
`isinteger(x) && min <= x < max`, matching upstream's
`(::Type{T})(x::AbstractFloat)` shape (`julia/base/float.jl`) — via a shared
`float_in_range` helper plus `signed_int_f64_bounds`/
`unsigned_int_f64_max_exclusive` (both bounds are powers of two, so both are
exactly representable in `f64` for every width up to 128). The identical
cast-then-round-trip-compare defect existed in every `convert_to_i8/i16/i32/
i64/i128/u8/u16/u32/u64/u128` F32/F64 arm (e.g. `Float32(2147483648.0)` ->
`Int32` also silently saturated); all were migrated to the shared helper in
the same pass. (2) An out-of-range `Int64(::BigInt)` raised `TypeError`
instead of upstream's `InexactError`; fixed to match. Verified against julia
1.12.6 (typemin/typemax boundaries and large in-range magnitudes such as
`Int128(2.0^100)` remain valid; only the previously-mishandled out-of-range
cells changed). Fixture:
`numeric/numeric_int_range_conversion_boundary_11214.jl`.
`subset_julia_vm_vm/src/vm/type_ops/conversion.rs`.
### Concrete Complex{Float64}/Complex{Float32} dispatch nondeterminism no longer reproduces (Issue #10775)

Re-verified `abs2(Complex(2, 3))` (and `Complex(2,3) / Complex(2,3)`) across
100+ fresh `sjulia` processes on current `main`: every run returns
`13::Int64` (byte-identical output), matching upstream `julia` — the
process-seed-dependent mis-dispatch reported in the issue (a concrete
`abs2(z::Complex{Float64})`/`abs2(z::Complex{Float32})` method winning over
the general `abs2(z::Complex{T}) where {T<:Real}` fallback for a
`Complex{Int64}` argument in roughly 1 of every 3 processes, on unmodified
`main` at commit `6d4b174a0`) does not reproduce anymore. Tracing every
applicability/tie-break path the runtime `CallDynamic`/`CallTypedDispatch`
resolvers can take for this call (`type_matches`, `nominal_type_names_compatible`,
`check_subtype_core` / `struct_params_are_subtype_with_lookup`,
`same_invariant_container_family_concrete_miss_core`) shows every one already
rejects the invariant-parameter mismatch deterministically — no live
HashMap-iteration-order dependency remains on the code paths `abs2` reaches.
The most likely explanation is that subsequent hardening (the #8659 dispatch
hash-iteration determinization ratchet, the #5915 `CoreSubtypeEngine`
centralization, and the #11076 module-owner nominal-name tightening) closed
this class of gap as a side effect after the issue was filed.

No code fix was needed. Added regression coverage instead:
`complex/complex_abs2_concrete_dispatch_stability_10775.jl` pins
`abs2`/`/ ` across `Complex{Int64}`/`Complex{Int32}`/`Complex{Bool}`/
`Complex{Float64}`/`Complex{Float32}`, verified against upstream `julia` and
against 100 fresh sjulia processes. `scripts/dispatch_seed_sweep.sh`'s
default category set now includes `complex` (previously only reachable via
its `--all` nightly invocation) so the multi-process determinism sweep the
issue's "suggested prevention" asked for now runs this category by default,
not only nightly. Issue #10645 (concrete `Complex{Float64}` `+`/`*`
overloads, blocked on #10775) can be re-evaluated separately; re-adding those
overloads is out of scope here.

### Regex comment termination no longer hides recursion tokens (Issue #10738)

The #10181 unsupported-recursion guard now follows PCRE comment lexing for
`(?#...)`: the first `)` ends the comment even when immediately preceded by a
backslash. Consequently `(?#\)(?R)` exposes the trailing `(?R)` to
`detect_regex_recursion` and returns the existing explicit
`regex recursion is not supported` error instead of reaching `fancy-regex`,
where the recursion token could silently mis-match. A focused bytecode-crate
unit test pins both the lexical detector and `RegexValue::new` rejection path.

### Conflicting imports are ignored with upstream's warning (Issue #11426)

A selective or renamed import whose name already has a source-earlier
module-body assignment is now ignored with upstream's
"WARNING: import of S.Box into Sink conflicts with an existing
identifier; ignored." The collect phase walks each module body in source
order recording, per value binding, the `definition_order` of the last
preceding `Stmt::Using` marker (`module_value_binding_positions`); the
new `conflict_winning_module_value_binding` helper then keeps the
existing binding authoritative at three consumers: eager imported-type
alias registration, bare `Var` reads (which previously resolved the
imported type statically), and parametric application (`A{Int}(1)` now
compiles to the module-global `ApplyTypeDynamic` form and raises the
catchable upstream TypeError instead of constructing the imported
type). Imports that precede the assignment keep static authority
(upstream's error-on-assignment lane remains future work). Fixture:
`modules/modules_import_conflict_ignored_11426.jl`.
### eval targets the module where the call appears (Issue #11421)

Bare `eval(expr)` now evaluates in the enclosing module, matching
upstream's per-module `eval(x) = Core.eval(M, x)`: the compiler attaches
its compile-time module path as a hidden first argument inside module
bodies and module functions, and the two-argument `BuiltinId::Eval` form
threads the module name into `eval_module_expr_value`, so
`module P; module C; eval(:(x = 1)) end end` installs `P.C.x` instead of
`Main.x`. The explicit `Core.eval(m, expr)` and `Base.eval(m, expr)`
spellings (previously a compile error and an UndefVarError respectively)
are implemented on the same path, accepting a Module value target.
Fixture: `modules/modules_eval_current_module_11421.jl` (@eval, eval
inside module functions, reading enclosing-module globals, explicit
Main/nested-module targets). Known remaining gap: functions *defined* by
eval inside a module still register globally (static qualified calls
`S.f(…)` fail; bare calls leak) — filed separately.
### LinRange length type parameter L (Issues #11441 / #11449)

The pure-Julia `LinRange` now matches upstream's signature
`struct LinRange{T,L<:Integer}` (`len::L` / `lendiv::L`; the `T<:Real`
bound was dropped, following upstream). `typeof(LinRange(0.0, 1.0, 5))`
reports `LinRange{Float64, Int64}` and
`typeof(range(big(1), big(2), length=3))` reports
`LinRange{BigFloat, Int64}`, matching Julia 1.12. Partial application
(`x isa LinRange{Float64}`, `LinRange{Float64} <: AbstractRange`) keeps
working, and the type-level `IteratorSize`/`IteratorEltype`/`eltype`
methods dispatch on the two-parameter form. Fixture:
`range/range_linrange_length_type_param_11441.jl` (BigFloat/Rational/
Float64 forms + trait pipeline). This closes the last open bug of
tech-debt #11449 (#11443/#11440 were already fixed; VM-native
`StepRangeLen` typeof verified upstream-identical).

### Baremodule builtin type binding authority (Issue #11419)

Core/Base ownership is now checked as lexical binding authority before builtin
type annotations, `isa`, `<:`, and static parametric DataType literals are
compiled. Core remains implicit in every module; Base-owned types require an
ordinary module, `using Base`, or an executed named import (`import Base` alone
does not expose exports). Hoisted module signatures replay builtin-only probes
at their original source positions, so an undefined annotation fails before
the method becomes active while existing user-type rename/import resolution is
left unchanged.
### range(…; length) TwicePrecision parity for Float32/Float16/narrow-int/step forms (Issues #9509 / #11440)

`range(start, stop; length)` now returns upstream's TwicePrecision-backed
`StepRangeLen` for Float32/Float16 endpoints (`StepRangeLen{Float32, Float64,
Float64, Int64}` with the ref/step collapsed to plain Float64 scalars, run in
the range's own precision) and for narrow-int / mixed endpoints via the
upstream promote chain (`HpElement`-generalized `linspace_hp` +
element-tagged `_linspace_range_f64`). `range(start; step, length)` with
float arguments moved from the pure-Julia `StepRangeLen` struct to the
VM-native TwicePrecision form (new `_steprangelen_range_f64` intrinsic,
upstream `range_start_step_length` / `floatrange` semantics with
authoritative length; `RangeValue.step_defined` discriminator). The
pure-Julia `show(::StepRangeLen)` gained upstream's zero-step constructor
form (Issue #11440). BigInt endpoints keep LinRange like upstream (missing
`L` type parameter tracked as #11441). Fixture:
`range/range_length_twiceprecision_9509.jl` (51 assertions, parity green).

### Cross-module parametric aliases and const-Union-alias bounds (Issues #11068 / #11003)

`const Y = OwnerA.X` (a qualified FieldExpression RHS) is now extracted as a
type alias — `is_type_expression` accepts module-qualified type names — and
the parametric base-name canonicalization chases alias chains before its
dotted early-return, so `OwnerB.Y{Int}()` resolves to the owning module's
inner constructor (#11068). #11003 (apply_type through const-Union-alias
bounds) was verified already resolved on main; a pinning fixture with the
negative case is added.


### String range checkbounds (Issue #10958)

Ported upstream strings/basic.jl:209-218: `checkbounds(s, r::AbstractRange)`
returns nothing in bounds (incl. empty ranges) and throws a catchable
BoundsError out of bounds, with the Bool form covering Integer and range
indices. `BoundsError.i` is now untyped like upstream boot.jl (range/tuple
indices carry through; the old `i::Int64` crushed ranges through
DynamicToI64 into a TypeError). This is the shape upstream's
`SubString{T}(s, i, j)` inner constructor relies on.


### Filtered/multi-binding generators in macro quote bodies (Issue #10923)

On the dynamic macro path, the quote constructor now builds upstream's
`Expr(:filter, cond, binding...)` and `Expr(:flatten, nested-generator)`
shapes, and macro expansion maps them onto the IR filter slot and
`MultiComprehension` (comma = cartesian, whitespace = flatten), including
lazy filtered single-binding generators. `ExprHead::Filter`/`Flatten` are
registered as nested-only heads. Flatten chains with a non-innermost filter
are explicitly rejected (the IR carries one innermost filter).


### Module-body @eval function definitions (Issue #10874)

A module-body `@eval f(x) = x + 1` now hoists its function into the module's
function list via the module-specific `extract_module_function_defs` wrapper
(module-body calls and qualified `M.f` calls resolve) while the runtime
`DefineEvalFunction` statement stays in the body — the same both-happen
behavior the top-level Program path has.


### Qualified Base const type aliases and the ÷ binding (Issues #10579 / #10695)

Qualified access to Base-internal const type aliases (`Base.Bottom`,
`Base.BitSigned`, ...) resolves to DataType values against the PRELUDE's own
alias list (not the flat shared table, which also holds user aliases that
must not become reachable as `Base.X`; the target is recomputed from the
prelude definition so same-named user aliases cannot shadow it). `Bottom`
stays qualified-only per the #10304/#10578 design. `÷` becomes a first-class
function binding via the forwarding method `Base.:(÷)(x, y) = div(x, y)`
(mirroring upstream `const ÷ = div`), fixing macro-expanded `@show 7 ÷ 2`,
value uses `f = ÷`, and missing operands. `fixture_julia_parity.sh` now
parses upstream summary tables with pipe-relative column ends (multibyte
characters in testset names shifted the previous byte-absolute positions).


### Three-valued Bool/Missing bitwise logic (Issue #10692)

Ported upstream base/missing.jl's Kleene-logic methods (`&`/`|`/`xor`/`⊻`
over Missing/Bool/Integer) into missing.jl: `false & missing === false`,
`true | missing === true`, every other missing combination is missing.
missing.jl loads after int.jl, preserving the #8197 dispatch-order contract
(Int64 stays the mixed-type runtime fallback). Fixture: 20 assertions,
parity-checked and repeated 25x for dispatch stability.


### eval() struct construction and field reads (Issue #10525)

The runtime eval mini-interpreter now calls an already-compiled struct's
default constructor by name (`eval(:(Foo(1)))` — a bare struct name with no
function methods routes as a type-object callable into
`call_runtime_callable_value`'s default-DataType construction) and reads
struct fields via dot syntax (`Expr(:., obj, QuoteNode(:name))` desugars to
`getfield(obj, :name)`, Julia's own lowering shape). Module-qualified VALUE
reads stay Issue #11073 scope (their UndefVarError propagates).


### N-dimensional block concatenation via hvncat (Issue #10381)

Ported upstream abstractarray.jl `hvncat` to pure Julia (along-dim Int form,
balanced dims form, ragged shape form + `hvncat_fill!`; sjulia adaptations
avoid `Val` dispatch). Lowering now emits the shape form
`hvncat(shape, row_first, xs...)` uniformly for `;;`/`;;;`-separated literals
with array-valued blocks (the shape form provably matches the dims form for
balanced input). `[A B; C D;;; A B; C D]` now produces the upstream (2,4,2)
array instead of the old mis-shaped (4,4) hvcat result; trailing-separator
rank padding, ragged rows, and DimensionMismatch validation match upstream.


### Accept the sprint sizehint keyword (Issue #10364)

`compile_sprint` accepts `sizehint` as upstream's no-op preallocation hint on
every route: combined with `context` it keeps the sprint_context path, on the
print fast-path exclusion it re-enters as a positional `sprint` call, and
unknown keywords still take the generic path's keyword error. The non-Float64
context route (`show(IOContext, T)` missing) is split out as Issue #11420.


### Channel take! in closures and do-block producers (Issues #10352 / #10353)

The `take!` builtin (VM `TakeString`) now falls back to method-table dispatch
for Struct/StructRef receivers (same shape as the Length fallback), so
`take!(c::Channel)` inside closures/@async bodies reaches the pure-Julia
Channel method instead of the builtin IOBuffer take! (#10352). Do-block
bodies route statement-only constructs (for/while/global/local/const) through
the general statement lowering in `lower_block_simple`, so
`Channel(sz) do ch; for ...; end` producers lower (previously
`UnsupportedExpression("for_statement")`). Added the typed
`Channel{T}(func::Function, sz::Integer)` producer constructor (#10353; no
default size — the default-arg expansion's arity-1 method collides with the
arity-1 inner constructor in the parametric dispatcher). `collect(::Channel)`
on a pending producer is split out as Issue #11417.


### Pin the Test.@test_skip contract (Issue #10350)

`@test_skip` was already implemented by the unified @test-family recorder
(Issue #10273 / PR #10367): the expression is never evaluated (a would-throw
call is not invoked), the record is Broken, and the run exits 0 — identical
to upstream. New fixture `stdlib/test_skip_broken_10350.jl` pins that
contract alongside `@test_broken`. `fixture_julia_parity.sh` now reads the
upstream summary table by header-aligned columns instead of "last two
numeric fields" (tables carrying a Broken/Fail/Error column were misread as
passed=Broken), terminating the table at the first row with an empty Total
cell.


### AoT: isless / mixed-type min-max / non-Int64 div-family (Issue #10131)

Closed the three AoT codegen gaps blocking numeric-matrix expansion. (1)
`isless` is now `AotBuiltinOp::IsLess` — `<` for integers, and the upstream
`_fpint` bit-pattern total order for floats (NaN sorts last, `-0.0 < 0.0`);
its Base bodies are call-graph leaves like the intercepted string builtins
(Issue #7058). (2) `min`/`max` cast mixed numeric operands to the Julia
promotion (`Float64` wins, then `Float32`, then the wider integer with
equal-width promoting unsigned) before comparing, and inference returns the
promoted type. (3) The AoT runtime `Value` gained I8/I16/I128/U8..U128
variants (`From`, `type_name`, Display, `PartialEq`), and the div-family
(div/fld/cld/rem/mod) native emissions are wrapped in `Value::from(...)`
when they land in a runtime-boxed slot. `fld`/`cld`/`mod`/`rem` and the float
classification predicates are AoT-claimed builtins so their Base definitions
are no longer pulled through conversion. `Value::F32` Display now matches
upstream print-form (`2.5`, no `f0`). Three AoT e2e tests compare generated
binaries' stdout verbatim against upstream julia.

## 最新対応 (2026-07-16)

### Retire bare struct identity lookups with one scoped resolver (Issue #11046)

`StructRegistry` now keeps a declaration-only owner/name index alongside its
lexical aliases. `resolve_scoped` applies exact-qualified, current-module,
Main/Base-origin, then lexical-alias ordering, while `insert_owned` preserves a
module owner even when a parametric instantiation intentionally has a bare
display name. Removed `base_struct_table`, `base_origin_bare_names`, and their
origin-table conversion paths; deterministic type-id projection now covers
shadowed field layouts without unordered canonical-entry scans. Declaration and
lexical-alias insertion are separate, so equal layouts cannot merge sibling
owners. The name-based lookup audit moved
`struct_table_bare_gets_compile` from 19 to zero and mutation-tests a disconnected
Main-owner branch.

### Invalid-UTF-8 String iterate/index/literal parity (Issue #8995 reopened scope)

Introduced `Value::CharMalformed(u32)` (the Julia Char bit pattern) and the
shared `decode_julia_char` decoder mirroring upstream string.jl's iterate
segmentation. String iterate now uses upstream's 1-based byte-offset states on
every carrier and yields exact malformed Chars (`'\xff'`, truncated multibyte,
overlong) in linear time. Covered surfaces: `s[i]` getindex on byte-backed
strings, `length` via Julia segmentation, string splat (`f(s...)` — also fixes
the pre-existing non-expansion of string splats), equality/isless by bit
pattern, `repr` escapes, one-arg `isvalid`, Char-typed slots and Char arrays
(malformed promotes storage to Any). `'\xff'` char literals and `"\xff"`
string literals lower through byte-oriented escape processing (`\xNN`/`\NNN`
are raw bytes, `\u`/`\U` codepoints) into `Literal::CharMalformed` /
`Literal::StrBytes`. Concat (`*`/`string()`) and `print(io, s)` into IOBuffer
sinks are byte-preserving. All three caches (base/prelude/compile) bumped.
Missing non-Int64 integer constructors for Char split out as Issue #11406.

### Canonicalize builtin type-name authorities (Issue #10954)

The types crate now owns one registry of 93 exact builtin spellings, their
nominal `JuliaType` projections, and parser/compiler/reflection visibility.
Type parsing, compiler first-class type-object emission, and module reflection
consume that registry while dynamic type-expression grammar remains separate.
Private registry coverage and a bytecode regression test execute the semantic
parser/compiler consumers, while the existing SubString fixture covers module
reflection. A CI/default-premerge source audit fingerprints the complete table
and rejects duplicate authorities or projection drift; four independent
negative mutations disconnect each consumer and delete an unsampled row.

### gcd/lcm for all integer widths (Issue #8812)

Added the upstream-shaped generic same-type `lcm(a::T, b::T) where {T<:Integer}`
(`checked_abs(checked_mul(a, div(b, gcd(b, a))))`), covering Int128 and the
whole UInt family with checked OverflowError semantics (the generic same-type
gcd already landed with #9315). Ported `checked_abs` / `checked_neg` into
checked.jl. Added the mixed-signedness (`Unsigned×Signed`) and mixed-width
(`Real×Real` promote fallback) methods in upstream shape, with the same-type
`T<:Real` MethodError terminator blocking the promote-fallback recursion trap
(#5966); promotes use the two-variable form (#9513). Fixture: 39 assertions,
upstream-parity checked, 25 repeat runs for dispatch stability (#10775).

### var"..." non-standard identifier syntax (Issue #8754)

The parser now merges `var"name"` into a plain `Identifier` CST leaf that
keeps the full source span (mirroring JuliaSyntax's single-identifier-token
merge). Name extraction strips the `var"..."` wrapper via the shared
`strip_var_quotes` helper, so assignment targets, function parameters,
function/struct/abstract/module names, `:var"..."` symbols, field access, and
`Meta.parse` all see an ordinary identifier named by the quoted content.
Strings with interpolation, escapes, flags, or empty content still fall back
to the generic string-macro path.

### Preserve module-owned constructor identity across dynamic calls (Issues #11153 / #11367 / #11368 / #11371 / #11373 / #11375)

Module-owned structs named `Dict` or `Set` now retain their exact owner for
qualified and bare positional calls, positional splats, keywords, keyword
splats, inferred parameterization, and explicit parameterization. Exact
qualified outer constructors still take precedence over default field
construction, while untyped explicit `Base.Dict` and `Base.Set` calls remain
available inside a shadowing module. Local and captured runtime `DataType`
values also support explicit parametric splat and keyword calls, including
lifted closure captures, with the parametric callee evaluated before its
arguments as Julia requires. The
existing constructor-owner audit now
pins the compiler's pre-splat/public-collection routing, qualified method-table
selection, parametric DataType emission, and the VM's exact-DataType
`MethodError` and default-constructor fallback paths; negative mutations prove
both compiler and runtime guards are enforced.

### Preserve explicit typed Base constructor ownership (Issue #11369)

Parameterized `Base.Dict{K,V}` and `Base.Set{T}` calls now retain the explicit
Base owner even inside modules declaring same-leaf `Dict` or `Set` types. Base
parametric definitions remain in a separate, non-source-visible registry in
fresh and cache-restored compile contexts; explicit `Base.T` type expressions
and nested fields of Base-origin structs resolve through that registry without
changing general Base method signature resolution. The combined
constructor fixture covers direct and module-nested typed Base calls alongside
the existing user-owned, dynamic, keyword, and evaluation-order matrix. The
private registry selects the definition only: instantiated Base types reuse the
existing bare top-level concrete identity instead of minting a second qualified
type-ID family. The constructor-owner audit has independent negative controls
for Base-qualified lowering, registry population, concrete identity reuse,
type-expression ownership, public collection result identity, and nested Base
field ownership.

### Derive typed-loop bail safety from exhaustive effect metadata (Issue #10814)

Replaced the two hand-maintained typed-loop `matches!` denylists with one
wildcard-free `TypedLoopOp::effects()` classification carrying data-dependent
bail and out-of-buffer-effect facts. The recognizer preserves its existing
accept/reject rule by folding those facts over the decoded block. Adding an
unclassified enum variant now fails compilation; existing RNG and transactional
array differential lanes continue to pin observable behavior.

### Re-normalize module-local alias targets in signatures (Issue #11029)

Module-local alias targets now retain their defining owner and abstract
classification when used by ordinary methods and explicit parametric inner
constructors. The exact #11029 constructor MWE and an abstract-supertype control
are pinned in the existing #11104 fixture. The separate owner-qualified struct
display mismatch remains tracked as #11365.

### Pin qualified parametric inner-constructor dispatch (Issue #8516)

Current main resolves fully qualified `M.C{M.V}()` calls to the same owner-exact
inner-constructor table as the module-body `C{V}()` form. The reopened bounded
type-argument reproducer is now part of the existing #11034 sibling-owner
fixture, preventing regression to raw field construction or a cross-owner
short-name table.

### Carry source identity in source-order comparisons (Issue #11100)

Type-alias definitions and signature use sites now carry an opaque
`SourcePosition` constructed from the active parsed-fragment scope. The sole
byte-offset comparison first accounts for source identity, so unrelated
include, package, cache, and REPL offset spaces cannot be ordered accidentally.
A required source-only audit pins the typed APIs, rejects raw lowering offset
comparisons, and is covered by API, comparison, and registry mutations. The
existing #11086 matrix continues to cover earlier/later definitions, distinct
sources, owner collisions, tombstones, and runtime signatures.

### Enforce binding provenance and runtime global-key authority (Issue #11317)

Semantic `Stmt::LocalDecl` consumers are now inventoried and must exhaustively
classify every `LocalDeclKind`. Declared-global frame-0 loads and stores route
through one compiler authority that derives the module-qualified runtime key.
The registered source-only audit rejects ignored provenance, unclassified
consumers, bare runtime keys, and registry weakening; independent negative
injections prove the primary discovery and authority detectors. Existing
consolidated VM and AoT test binaries carry a table-driven matrix across lexical
contexts, scope shapes, binding
origins, exits, and typed/dynamic representations. VM rows assert lowered IR,
qualified global opcodes, slot metadata, and execution; AoT rows prove both
provenance variants are accepted by IR conversion and execute the supported
typed path.

### Try-clause lexical ownership and strict soft-scope provenance (Issues #11305 / #11322 / #11335)

Try, catch, else, and finally retain distinct lexical owners while module-level
clauses now apply Julia's strict soft-scope assignment rules. Source-order
inventory keeps mutable globals, const globals, and retired clause-local names
distinct: mutable globals produce a fresh local plus the upstream warning,
consts and prior clause locals shadow silently, and explicit globals continue to
target the module binding. Assignment-backed candidates are separated from
function/generic identity, which remains owned by #11319. Lowering, runtime,
and CLI regressions cover fresh-name ownership, later-loop `@isdefined`, outer
global/const preservation, nested reuse of an enclosing clause-local slot,
source-order replacement of retired provenance by a later global, and exact
warning presence. General value-expression traversal and execution-aware effects
from untaken explicit-global clauses remain tracked by #11159 and #11338;
direct fresh-loop lifetime without retired provenance remains #11339.
### Unify runtime parametric-struct bound alias expansion (Issue #11142)

Runtime parametric-struct schemas now expand visible aliases through one
authority before either string-backed or structured `UnionAll` wrappers are
built. Both upper and lower bounds use recursive exact-qualified / unique-bare
lookup; ambiguous bare aliases remain unresolved and schema binder names are
protected. The former upper-only retry in `Core.apply_type` was removed, so the
validator consumes canonical bounds instead of maintaining a second lookup
policy. Unit and fixture matrices cover exact, unique, ambiguous, Union/nested,
direct apply, and runtime allocation paths; the cache-disabled AbstractAlgebra
residue fixture remains the end-to-end reachability lane.

### REPL definition delta compiler snapshot advancement (Issue #9784)

Successful new-function/new-struct live appends now commit an advanced reusable
compiler snapshot whose positional program matches the held VM. Discarded
expression mains become inert offset gaps only when a later definition needs
alignment, avoiding per-expression prefix copies. Consecutive definitions and
their references stay live-appended; failed runs leave the prior snapshot intact.
Definitions that repair a prior unresolved call still full-recompile, and persisted
methods carry the post-success world visibility needed after a held VM is dropped.

### REPL live-VM runtime-error recovery boundary (Issue #9784)

Snapshot-stable live deltas now recover the same VM after an unhandled,
Julia-catchable runtime error. A VM-owned reset unwinds to frame 0 and clears invocation-local stack,
handlers, tasks, transient roots, output/error/display state, and RNG while
preserving globals, heap/object identity, modules, installed definitions, method
worlds, and dispatch caches. Mutations completed before the exception are also
synchronized into the transitional fallback mirrors, so a later hard-scope full
recompile cannot restore a stale pre-error value. This includes indirectly
created globals, subsequent slotized read-modify-write, `ans`, and LIFO stream
redirect restoration. Upstream and differential regressions pin
`x = 41; error(...)` followed by `x == 41`, and the next ordinary input records
`vm-build=0`. Definition/type deltas with an advanced compiler snapshot, plus
runtime `@eval` calls whose pre/post definition fingerprint changes, still take
the conservative drop boundary. Host cancellation and VM-internal invariant
failures also drop. Their source-ordered transaction is the next
slice; Issue #9784 stays open.

### Replay nominal registrations after package-cache restore (Issue #11280)

Added one loader-owned reconstruction pass shared by freshly lowered and
`.ji.json`-restored package modules. After every dependency loads successfully
and before loader state is committed, it recursively registers qualified
struct, abstract-type, primitive-type, and nested-module families. A regression
whose source is empty but whose valid cache payload contains the full
declaration matrix proves that the cache-hit lane itself performs the replay.
Repeated registration is harmless because the registry uses set insertion.
Lowering-only alias/binder/quote state remains scoped to its pass, and the
unchanged serialized payload requires no package cache-version bump.

### bytecode instruction effect table と typed-I64 local CSE (Issue #9494)

shared bytecode crate に fail-closed な VM instruction effect table を追加し、未知・制御フロー・
observable instruction を既定で barrier とした。production peephole optimizer はこの table を使い、
同一 basic block 内で再出現する typed-I64 slot 式を既に materialize 済みの slot load へ置換する。
slot write、jump target、REPL の人工境界では available expression を破棄し、fusion と CSE の
old-to-new mapping を合成して function/main entry と jump target を維持する。

## 最新対応 (2026-07-15)

### Cache-independent macro-loader re-entry contract (Issue #11145)

Stdlib and bundled-package macro loading now use two independent instances of
one three-state transition helper. Same-module recursive entry skips the nested
load, successful callbacks publish `loaded` only after the full macro surface is
registered, and error/panic cleanup permits retry. A fresh-state unit contract
uses injected callbacks rather than global registries or persistent/preload
caches; it also proves different modules remain independent and runs the same
matrix for both loader kinds. Removing the re-entry guard makes the contract
fail with `Loaded` versus `Reentrant`.

### Ratchet duplicate-branch Clippy findings module by module (Issue #10725)

Inventoried 526 unique workspace-wide advisory findings by crate and module,
then consolidated the high-signal `ReturnRng` / `ReturnRange` / `ReturnRef`
implementations behind one semantics-preserving helper.
`subset_julia_vm_vm/src/vm/exec/return_ops.rs` now
denies `clippy::match_same_arms`, so later changes cannot split those return
continuations into duplicate implementations. The post-change probe reports
525 unique findings overall, zero in the protected module, and zero
`if_same_then_else` findings.

### compile / VM physical crate extraction (Issue #9090)

Extracted the compiler, interpreter/register VM, and lowering core into
`subset_julia_vm_compile`, `subset_julia_vm_vm`, and
`subset_julia_vm_lowering`. Narrow integration-owned host traits preserve
prelude/Base/package loading, VM-backed macro expansion, cancellation, and
embedded package lookup without upward crate dependencies. Cache schema
version 150 remains stable, compile↔VM coupling is 0 in all audited directions,
and independent checks stay `Fresh` after touching the sibling crate. The cold
check median improved from 47.9 s to 34.41 s; VM-only and compile-only warm
checks improved to 2.32 s and 3.19 s respectively.

### try-clause hard-scope and dynamic string-budget parity (Issues #11281 / #11301 / #11308)

Try, catch, else, and finally clauses now discard newly introduced locals and
catch binders at every normal and non-local exit, while explicit local shadows
restore the enclosing binding. Already initialized enclosing bindings and
lowering-generated result slots are excluded from clause cleanup, so a caught
const reassignment preserves the original global. Any-typed dynamic String/Char
concatenation now applies the same VM memory-budget check as typed
`StringConcat`. The MacroTools capture fixture was also corrected to evaluate
`@capture` outside `@test`'s generated try scope, matching upstream Julia.
Explicit globals in module-owned functions and catch clauses now qualify their
flat frame-0 keys with the owning module (Issue #11312). AoT timing lowering also
recurses through transparent typed-local statement blocks, preserving escaped
caller assignments such as `@time x = 42`.

### Restore upstream parity for the Base export manifest (Issue #11162)

Removed `Base` from the parsed Base export metadata. Ordinary modules still
receive the language-level implicit `Base` binding, while `using Base` and the
export consistency gate now match upstream Julia's actual export surface.
Issue #11298 added a VM-link-free source-only comparison with a required
premerge/CI registry row. Its negative controls inject `Base` into the subset
manifest and weaken the registry row; the existing Rust test remains an
independent parser-level defense.

### AoT generated-Rust ownership conventions (Issue #11202)

Defined a representation-aware borrow/clone/move contract for generated Rust,
linked it from the architecture index, and added an operational checklist for
new codegen templates. The downstream Cargo helper now exposes compiler output,
and a targeted negative control proves it detects #10663's two-call reused
`Value` E0382. Issue #10663 remains open and owns converting that control into a
positive compile regression when the codegen violation is fixed.

### Reconcile the semantic-ID plan with as-landed verdicts (Issue #11284)

Replaced disproved Phase 2a/2b/3 assumptions with landed verdicts and added a
conservative semantic verdict to every inventory row. All sites remain
identity-bearing unless an exact evidence rule classifies them as a sanctioned
lexical boundary or verified-inert table. The generated six-domain Phase 4
residual and checklist now expose the real work owned by #11078, #11095, and
#10460 without hiding mechanically misclassified `other` sites.

### compiler HashSet emission と nominal parametric owner を決定化 (Issues #10460 / #11264)

closure capture の3つの bytecode emission 経路を名前順へ canonicalize し、独立 process の
Base precompile が同じ closure environment layout を生成するようにした。cross-process test は
persistent prelude/Base cache を明示的に無効化し、同じ artifact の deserialize 同士を比較して
compiler 非決定性を見逃す穴を閉じた。加えて bare parametric alias の concrete instantiation は、
同名 family が複数 module に存在するとき alias と構造的に一致する唯一の qualified owner を
type-id reverse map と instantiation key に保持する。fresh package lowering で registry の衝突認識が
compile context より先行する場合も owner を復元し、`AbstractAlgebra.Integers{BigInt}` が bare
`Integers{BigInt}` へ崩れて dispatch 不能になる fresh/cache-restored 差を解消した。
flow-sensitive `isa` narrowing は exact concrete struct だけを解決し、bare `SubArray` family から
HashMap 上の任意 instantiation を選ばない。loader cache restore が nominal registration を
再生する一般化は follow-up #11280 で追跡する。

### builtin-shadow source binder を runtime UnionAll body へ構造 rebind (Issue #10460)

値位置の `UnionAll(var, body)` 構築時、body literal が binder 宣言より先に parse されて
`Int64` / `Module` などの nominal leaf へ固定された場合も、`CoreType` graph 上で明示 binder
へ rebind してから runtime TypeVar identity を付与する。nested UnionAll は既存 binder graph
を保持したまま bounds/body だけを再帰し、builtin または登録 user type を shadow した source
binder は canonical bare alias へ collapse せず fresh object identity (`==` だが `!==`) を保つ。
inner binder の bound から outer binder への identity 参照と、bare/qualified leaf が混在する
body で bare spelling だけを capture する規則も構造的に保持する。runtime `Vararg{T}` /
`Vararg{T,N}` は direct・semantic-wrapper・dispatch の全経路で canonical `CoreType` shape へ変換する。
これにより `vm/type_utils.rs` の最後の `.name()` fallback を削除し、
`vm_type_utils/julia_name_projection` ratchet を 1 → 0 にした。#10100 fixture の 49 assertions
から 55 assertions へ拡張し、display・`isa`・双方向 subtype・`==`・`===` を upstream と比較した。

### Constructor call-site owner resolution audit (Issue #11172)

Added a guarded source audit and explicit inventory for every remaining
`short_constructor_name` compatibility projection. New, moved, or duplicated
leaf-table probes fail; the inventory distinguishes owner-checked paths from the
two legacy fallbacks owned by #10992. The same audit pins exact-qualified and
unique-bare safeguards in runtime DataType/apply-type dispatch. Negative controls
cover a direct call-site fallback, disabled runtime owner comparison, and
required-registry weakening, while the
constructor checklist now requires the owner/parametric/kwargs/Base-collision
matrix from #11153/#11177.

### @testset-local named-function capture (Issue #11260)

Macro-expanded `@testset` hard scopes now feed bindings that are live at each
local function definition into the same pre-compilation capture analysis used
by module-level `let` scopes. Short- and full-form named function bodies
therefore compile captured reads as `LoadCaptured`, while later lexical
predeclarations remain excluded until their stores execute.

### Semantic audit-selftest anchors and target selection (Issue #11274)

Audit negative-control source edits now share fail-loud exact-one literal and
regex helpers. Binder-sensitive controls delimit semantic owners and capture
local names instead of pinning incidental spellings; a ratcheted TSV classifies
all helper-owned anchors. The harness can list or select controls by mutation
target, and guarded premerge automatically runs controls whose target files
changed. Registration mode rejects raw anchor/needle count-replace pairs, so
the stale-anchor failures from #10895/#11269 cannot silently recur.

### cfg(test) source-audit scanner contract (Issue #11208)

The type-representation string-reparse audit now runs a focused synthetic
matrix before scanning repository baselines. Immediate, blank-line, and
adjacent-attribute test-module layouts must hide test-only reparse tokens, while
a production token after a cfg-gated non-module item must remain visible. A
sandbox mutation restores the pre-#11207 trivia transition and proves the
focused diagnostic fires; the existing production-token injection continues to
prove the ratchet's false-negative direction. The same full harness exposed and
repaired #11269: the local-shadow injector now tolerates the live `span`
parameter spelling while retaining its exact-one-arm assertion.
### Enforce parametric lower bounds after structured Type-object binding (Issue #11233)

Compile-time dispatch now preserves a method TypeVar's declared upper and lower
bounds when `Type{W{T}}` structurally extracts `T` from a type-object argument.
Previously that invariant path rebound the extracted type with both bounds set
to `None`, so an invalid `T` could select the bounded method while the equivalent
runtime function-value call correctly selected the fallback. Bound checks now
receive the registered struct hierarchy, so a user-defined abstract supertype
correctly satisfies `T>:Leaf` without weakening rejection of unrelated types.
Unit and fixture coverage pair rejecting, exact, and abstract-supertype lower-bound
cases plus structured upper-bound accept/reject cases across direct/runtime calls;
the fixture matches upstream in cold and warm cache modes.

### Lexical struct-new authority prevention audit (Issue #11211)

The LambdaContext routing audit now owns six rules: the existing predicate and
context-routing constraints plus a complete `new_struct_name` mutation
inventory, token checks for the root-helper/lifted-function/runtime-eval/
structural-collector owners, and registration of the four ownerless-lookup
fixtures as one semantic family. Its negative self-test injects the forbidden
post-hoc slice/watermark stamp. The lowering checklist now covers ordinary
nested functions, lifted closures, macro-lifted thunks, runtime eval functions,
eval-lifted descendants, and ownerless lookup.
### Guard STATUS/DONE archive budget before merge (Issue #11263)

Moved old dated STATUS/DONE sections verbatim into the 2026 archives and
restored both live files below the 3,000-line budget. Archives retain stable
newest-to-oldest chronology across repeated batches. A fixed-threshold read-only
wrapper runs from the required default source-only guarded-premerge registry;
negative controls cover STATUS, DONE, aggregate-runner reachability, and registry
row removal/weakening.

### Rust toolchain and feature-gated Clippy contract (Issues #11253/#11258/#11259)

Rust 1.95 is now the workspace MSRV and Rust 1.95.0 is pinned as the exact local
reference with `clippy` and `rustfmt`. `run_clippy_lanes.sh` mechanically owns
the `default` (all workspace members), `repl`, `aot`, and `aot,cranelift`
all-targets commands; premerge, the AoT gate, and current-stable CI consume that
registry. A source-only audit plus injected negative control rejects lane
removal or an AoT-to-default downgrade. Activating the combined lane exposed
and fixed seven feature-intersection compiler/Clippy failures, including a
checked tuple-index conversion and a stale `Function` test initializer. Running
the same registry on current stable Rust 1.97 then exposed and fixed five new
AoT lint diagnostics without raising the Rust 1.95 MSRV/reference contract.
### runtime resolver candidate-order permutation matrix (Issue #11252)

Fixed, slice-backed, typed-dynamic, and callable-value runtime resolver
adapters now share a forward/reverse candidate permutation test using the
user-abstract versus builtin-generic shape from #11230. A seeded first-winner
control proves the harness detects order dependence. Callable-value dispatch
now retains equal top-score/fixedness candidates and selects a unique structured
strict-dominance winner; its single-winner path remains allocation-free and
equivalent legacy rows keep their prior stable precedence. The complete
`subset_julia_vm_types` suite (514 tests) is green.

### centralized definition-order fragment chronology (Issues #11036/#11128/#11134)

Independently lowered prelude, Base, REPL, and package fragments now cross one
`DefinitionOrderCursor` authority. Package Modules are inserted after stamped
`using`/`import` anchors instead of after the complete user Program. Package-
local dependency anchors and compiler method collection consume the same
chronology, preserving later constructor replacement across fresh/cache-
restored, REPL, type-stability, and Core-IR CLI paths. Core-IR v5 rejects stale
pre-fix chronology. Recursive rebasing covers stored definitions and executable
block copies, preventing block-local signatures from being misclassified as
forward references (#11144). `check_definition_order_merges.sh` inventories
every raw or independent boundary, including aliased mutations.
### Vector/Matrix generic alias の TypeVar を構造参照し ratchet を premerge 強制 (Issues #10460 / #11241)

`vm/type_utils.rs` の generic alias 判定が `VectorOf` / `MatrixOf` の要素型を
`.name()` で文字列化して binder 名を回収していた 2 経路を、unbounded source
`TypeVar` node の直接 match に置き換えた。concrete element は alias binder とみなさず
concrete element と無関係な binder の組合せは fail closed にする unit test を追加し、
`vm_type_utils/julia_name_projection` baseline を 2 → 1 に縮小。builtin nominal leaf と
同名の source binder (#10100/#10613) は lowering が構造 node を保持するまで単一の互換
fallback に集約した。さらに #11236 の postmortem に従い、既存の ratchet を
`source_only_audits.tsv` の `premerge_default=true` lane へ登録した。Actions 無効環境でも
guarded premerge が今後の display-string bridge 増加を阻止する。

### 配列要素型の structured carrier で RuntimeUnionAll identity を保持 (Issue #11236)

`ArrayElementType::Abstract(String)` に identity-bearing な `UnionAll` / runtime
`TypeVar` graph を格納していた経路を、`JuliaType` を直接保持する boxed
`Structured` carrier に置き換えた。配列の allocation・index・reflection は表示名を
再 parse せず同じ graph を受け渡し、3 次元以上の `Array{T,N}` も structured
`RuntimeParametric` として構築する。optimizer では boxed element を保守的に `Any` として
扱い、`SubArray` などの iteration を誤って unbox しない。serialized bytecode schema の
追加に伴い Base cache version を 147 へ更新し、upstream/sjulia fixture、focused unit tests、
type-string reparse ratchet で回帰を固定した。

### Lezer-compatible parser rewrite M0/M1: oracle CLI + Canonical CST common model (Issue #11049)

- `tools/lezer-oracle.mjs`: lezer-julia (extern/, `extern/MANIFEST.tsv` に SHA
  固定) で Julia ソースを解析し、`subset_julia_vm_parser_common/schemas/
  canonical-cst.schema.json` (仕様全文: Issue #11225) 準拠の Canonical CST JSON (`version: 1`) を出力
  する oracle CLI。UTF-16→UTF-8 バイトスパン変換、lezer ノード名正規化
  (`Program→SourceFile`, `⚠→ErrorNode`, 演算子トークン→`Operator` 等)、
  文字列内 `StringFragment` 合成、エラー回復ノードごとの `UNEXPECTED_TOKEN`
  診断を実装。
- 新クレート `subset_julia_vm_parser_common`: `Span` / `NodeKind`(正準カタログ
  + `Other` passthrough)/ `NodeValue` / `CstNode` / `Diagnostic` /
  `CanonicalDocument` と、スパン不変条件検証 (`validate_spans`)・木 diff
  (`first_divergence`)。
- lezer-julia テストコーパス 130 ケースの oracle スナップショットを
  `subset_julia_vm_parser_common/tests/oracle_snapshots/` にコミット
  (再生成: `bash scripts/gen_lezer_oracle_snapshots.sh`)。Node.js なしで
  走るスキーマ・不変条件・正規化カナリアテスト付き。
- 運用ガイド: `docs/vm/LEZER_PARSER.md`(ロードマップ M0/M1 完了、M2 Lexer 以降は未着手)。

### 入れ子 runtime TypeVar の型リテラル identity を構造的に保持 (Issue #10861)

`DataType` 引数の内部まで `RuntimeTypeVar` を検出し、parametric type 構築を
display string の再 parse ではなく structured `JuliaType` で行うようにした。
`Vector` / `Matrix` / `Type` / `Tuple` の canonical variant 選択は
`JuliaType::from_structured_parametric` に集約し、literal builder と UnionAll
substitution (`Core.apply_type`) の両 lane が同じ authority を使う。
これにより `Tuple{Vector{A}}` の parameter identity と whole-type `==` / `===`、
`Type{Vector{A}}` の singleton subtype、`Array{Vector{A}}` の trailing UnionAll
と free TypeVar identity を upstream 同様に保持する。user parametric type の
under-applicationが既存の upper bound と identity を保つことも負の回帰 guard にした。

### 例外型 taxonomy funnel — 送出クラスの単一権威化 (Issue #11146 / #10813 Phase 2a)

sjulia の例外クラスは各 raise 箇所が「一番近い」`VmError` variant を独自に選ぶ
ad hoc 方式で、**3箇所**が独立に「その error は何か」を決めていた: (1) raise 箇所
(variant + 自由文メッセージ — variant と**別のクラス名**を名乗るメッセージを書けた)、
(2) `vm_error_to_exception_value` の各 arm がハードコードする Julia struct 名リテラル、
(3) `is_catchable_vm_error`(「byte-for-byte で同期を保て」とコメントで requestする
手動リスト = 規約)。#10354 の fixture-fallout 実測で判明した 5 root cause のうち **4件が
まさに (1) の形**(`VmError::TypeError(format!("ArgumentError: ..."))` — メッセージは
ArgumentError と言うが `typeof(caught)` は TypeError)。

**funnel**: `VmError::exception_class()`(`subset_julia_vm_bytecode/src/error.rs`)を
唯一の variant → upstream 例外クラス写像とし、catch-all arm 無しのコンパイル時網羅 match
に。(2)(3) は funnel から**導出**する形に変更 — 例外オブジェクトの struct 名は
`ExceptionClass::julia_name()` から、catchability は `julia_name().is_some()` から。
raise 箇所はもはやクラスを選べず(variant を選ぶとクラスが決まる)、新 variant 追加は
クラス宣言までコンパイルが通らない。**構造的保証はここまで**で、メッセージ文字列は
コンパイラが縛れない自由文なので、そこは監査 (下記 R1) で担保する — 過大に「全部
構造的」と主張しない。

**型不一致の修正**(いずれも事前に julia 1.12.6 で検証): corpus `convert_failure`
(`convert(Int,"a")` → TypeError から **MethodError** へ、全幅の `convert_to_*` fallback
を 1 helper に集約)と `method_error_noncallable`(`z=5; z(1)` → 4 つの呼び出し経路 +
関数合成が各自「近い」error を選んでいた → **MethodError**)。corpus の残り 2 件
(`undef_var_call` / `memory_undef_ctor`)は **in-flight の PR #11163 (#10354) が既に修正済み**
で、あちらは新 `Instr` + CACHE_VERSION bump を伴うため重複実装は wire-ID/cache 衝突に
なるだけと判断し**意図的に重複させていない**(#11163 を先にマージすれば type-mismatch は 0)。

**メッセージ/variant 矛盾サイトの一掃**: `<:`/`isa` arity・負 Memory サイズ・Memory OOB
(7)・文字境界 byte index (2)・Enum 不正値・文字列スライス/タプル添字 (6)・LinearAlgebra
形状 (4)・`Meta.parse` 失敗。**Julia 層も同じ欠陥**で、`error("ArgumentError: ...")` /
`error("DimensionMismatch: ...")` は ErrorException を投げるためメッセージとクラスが矛盾
していた(#10354 が `_block_vcat` 1件を手で直したのの一般形)→ 64 サイトを
`throw(ArgumentError(...))` / `throw(DimensionMismatch(...))` に変換。**表示文字列は
byte-identical**(ErrorException の showerror は生メッセージを出すので `"<Class>: "`
接頭辞が文字列上だけでクラスの役をしていた)で、変わるのは `typeof(e)` のみ = 狙い通り。

**Phase 1a から引き継いだ 2 バグ**(`types/signature_forward_reference_11025.jl`)も
**修正**。これは「型が違う」ではなく「**例外ですらない**」(`typeof(caught) == String`):
`eval` のランタイム関数定義が型付きパラメータ未実装 → `VmError::NotImplemented` →
#8664 の設計で Julia 例外オブジェクトを持たない。upstream は**シグネチャ注釈を定義実行時に
即評価**するので未束縛名は定義位置で `UndefVarError` — コンパイル経路は既に同形
(`Instr::LoadAny` probe, #10396/#11025)だがランタイム eval 経路だけ抜けていた。同じ名前集合
(各パラメータ注釈 + `where` 束縛の上界、binder 自身は除く)を probe するようにして解決。
fixture の 2 assertion は**弱めず強化**(型を見ない `@test_throws` → 実際に検査される
`try`/`catch` + `isa UndefVarError`)。あわせて `NotImplemented` を「例外オブジェクト無し」
から **ErrorException** に再分類 — 未実装機能はユーザ到達可能なので生 `String` を catch
させるのは擁護できない(#8664 の論拠はクラス選択の理由であって「クラスを持たない」理由では
ない)。注釈が**解決する**型付き eval 定義は依然 raise(`::Any` として定義すると暗黙の
誤 dispatch = 本 epic が「クラッシュより危険」と呼ぶ silent-wrong-result になるため)。

**エンフォースメント**: `scripts/check_exception_taxonomy_funnel.sh` を
`scripts/source_only_audits.tsv` に `premerge_default=true` で登録(既定の guarded-premerge
ランナーが実行)、`check_audit_negative_selftest.sh` に **4 つの negative self-test**
(規則ごと)。R1 = variant と矛盾するクラス名で始まるメッセージ禁止(**この規則が手動
スキャンの見落とし 2 件を検出**)、R2 = catch 時 builder の struct 名リテラル禁止、
R3 = funnel の catch-all arm 禁止 + `is_catchable_vm_error` の委譲強制、R4 = Julia 層の
`error("<Class>: ...")` 禁止(コンストラクタ引数が必要な残 58 件は
`docs/vm/EXCEPTION_TAXONOMY_JULIA_BASELINE.tsv` でラチェット = 減る方向のみ)。監査は
**funnel 自体を parse** して variant→class 表を作るので、守る対象とズレようがなく、対象が
移動したら vacuous pass ではなく FAIL する(#9129 F2)。

probe: 発散 **12 → 8**、**型のみ不一致は 0 件に**(exact match 26 → 30; うち
`undef_var_call`/`memory_undef_ctor` は先行マージされた PR #11163 が担当 = 重複回避の
判断どおり)。副産物として #11190(lambda ローカル代入が
兄弟スコープの同名ローカルと衝突すると capture と誤判定)を起票。
### Runtime UnionAll の generic alias 同一性を binder spelling から分離 (Issue #11013)

`RuntimeUnionAll` を TypeVar identity に基づいて alpha projection してから
generic alias を認識するよう統一し、`Vector{X} where X` が `X` や macro
gensym の綴りに依存せず `Vector` と表示・等値・双方向 subtype 一致するようにした。
display / `JuliaType::type_eq` / VM `<:` の別々の lane を同じ構造判定へ揃えた。
上下 bound がある wrapper は alias へ collapse せず、例えば
`Vector{X} where X>:Int64` は bare `Vector` の proper subset のまま保持する。

### struct-body global helper の context routing と audit 復旧 (Issues #11179/#11183/#11186/#11187/#11197/#11204)

`lower_struct_definition_with_ctx` を live-context の全経路に通し、global helper 内の
named/anonymous function loweringを中央 `lower_*_with_ctx_if_needed` authorityへ統合した。
lifted closure が `new{T}` を呼ぶ場合も `new_struct_name` を継承する fixture で固定。
transparent begin/let/assignment-RHS と `@kwdef` でも helper を失わず、mixed/multiple
splat の `new` は全引数を tuple 化してから一括展開するため先行引数を落とさない。
ordinary lexical function だけが enclosing struct の `new` authority を継承し、runtime
`@eval` function では遮断する。owner のない `new` は通常の name lookup として扱うため、
upstream 同様に catch 可能な `UndefVarError`（または user binding）になる。
eval-defined function 内の anonymous closure でも同じ lookup を使い、compile-time の
`Unknown function` trap へ退行しない。lifted async/task thunk は生成時の lexical
authority を使うため eval 境界を越えず、ownerless `new{T}` と keyword/splat も保持する。
同時に #11136 の reviewed inventory growth を ratchet に反映し、source-only auditを
 13/13 greenへ戻した。

### Root-cause quality analysis and prevention handoff (Issue #10452)

- Replayed historical label events to freeze and classify the exact 403-Issue
  population, plus four comparable weekly quality cohorts, with a reproducible
  standard-library collector.
- Completed the #10465 guarded differential contract: semantic paths select it
  automatically and VM/AoT covers coprime pi, Aizawa, and Mandelbrot. Recorded
  the acceptance/ownership handoff in `QUALITY_PREVENTION_PLAN.md`.

### 構造体テーブルを所有者スコープ ID (`StructId`) で再キー化 (Issue #11078)

Phase 2b (#10459 semantic-ID epic) の本体。`SharedCompileContext::struct_table` /
`RuntimeCompileContext::struct_table` を `HashMap<String, StructInfo>` から
`StructRegistry` へ移行し、エントリを `StructId { module: ModuleId, local }` で
キー化した。名前は id 空間への **エイリアス**であり、`name -> StructId` の索引 1 本だけが
字句解決境界 (`SEMANTIC_IDENTITIES.md` の必須性質 #3)。`base_struct_table` は
`HashMap<String, StructId>`(同一 id 空間へのエイリアス写像)になり、構造体レイアウトの
第二のテーブルではなくなった。

ratchet の実移動 (`scripts/check_name_based_lookup.sh`):

| パターン | before | after |
|---|---:|---:|
| `structinfo_name_maps_compile` | 61 | **0** |
| `struct_table_bare_gets_compile` | 20 | 19 |

`docs/vm/SEMANTIC_ID_INVENTORY.tsv`: struct ドメイン 353 → 266、anchor 93 → 34、総計 874 → 795。

**なぜ表面的な改名ではないか**: 名前キーのテーブルは、モジュールの bare エイリアスが
同名エントリを上書きした時点でエントリを **物理的に失う**。この消失こそが
`base_struct_table` + `base_origin_bare_names` (#10078/#10257) の存在理由で、
上書きされた `StructInfo` の私的コピーを保持していた。id キーなら衝突は
**shadow するだけで破壊しない**ため、エイリアス写像は「衝突前にどの id を指していたか」だけを
覚えればよい。この性質はテストで固定済み
(`a_clobbering_bare_alias_shadows_but_never_removes_an_entry`、および production 側の
`test_julia_type_to_value_type_origin_table_prefers_base_signature_issue_10258` が
shadow された Base エントリを id 経由で今も取得できることを assert)。
これは当該ワークアラウンド撤去の前提条件だが、撤去自体は本 PR では行わない
(bare 名がどの構造体に解決されるかが変わり、Base 全体に波及するため)。

**wire 変更なし / id は導出 (CACHE_ARCHITECTURE.md Pattern A)**: `StructInfo` に
`Serialize` は無く、`RuntimeCompileContext` は `#[serde(skip)]` (#3973)。struct table は
両レーンで再構築されるため relocation table は不要 — issue が想定していた
「`StructDefInfo` 約 25 箇所 + wire 変更」は不要と判明した。`CACHE_VERSION` は
schema fingerprint 対象ファイルが変わったため 142 → 143。

**決定性**: 両レーンとも構造体登録前に `register_module_ids` の決定的な walk で
module interning を seed するため、owner id が構造体登録順に依存しない
(キャッシュレーンは `struct_defs` を先頭で一括登録するので登録順は実際に食い違う)。
`base_cached_compile_struct_table_matches_fresh_compile_10265` に
`struct_id_snapshot` の assert を追加し、fresh compile と cache restore で
全ての名前が同一の `StructId` に解決されることを実 Base キャッシュで検証。

### #10459 ratchet の登録漏れを解消 (Issue #11078)

`scripts/check_name_based_lookup.sh` は `scripts/source_only_audits.tsv` に
未登録だったため `run_source_only_audits.sh` / `premerge_gate.sh` のどちらからも
実行されておらず、`origin/main` で **RED のまま放置**されていた
(`typevar_core_bindings` が 12 → 13 (#11096) → 15 (#11138) と漂流)。
登録 (13/13) + 3 サイトの分類付き baseline 是正で解消。#10870 と同じ
「どこからも強制されない audit の baseline が腐る」クラス。

### 兄弟モジュール同名構造体メソッドの登録衝突 — builtin family 形状の被覆 (Issue #11094)

#11094 は PR #11138 の owner 保存 dispatch 射影で修正済み。ただし #11138 の fixture は
4 形状のうち 3 つしか固定していなかったため、残り 1 つ (bare 名が内部の
`is_known_struct_family` builtin コンテナ一覧と衝突する形状) を
`modules/sibling_builtin_family_struct_dispatch_11094.jl` で固定した
(#11138 直前のコミットで RED、以後 GREEN を確認)。
### techdebt(#10813) Phase 0: 例外の型・発生層・捕捉可能性 棚卸し + decomposition (Issue #10813)

Issue #10813(例外の型・発生層・捕捉可能性の系統的乖離 epic)の Phase 0 のみを
納品。`scripts/panic_debt_classification.py`(#10869 Phase 0)を手本に、
report-only generator `scripts/exception_parity_probe.py`(+ `.sh` wrapper)を
追加: undefined-var/MethodError/BoundsError/DivideError/InexactError/
ArgumentError/TypeError/KeyError/DomainError/StackOverflow/parse
error/kwarg error 等をカバーする 40-case corpus を bare/try-catch-wrapped の
両形態で `julia` と `sjulia` 双方に対して実行し、例外**型**一致と**捕捉可能性**
一致を突き合わせる。committed snapshot は `docs/vm/EXCEPTION_PARITY_PROBE.tsv`
(40 cases, 38 comparable, 26 exact match / 12 divergent)。

3 つの主張を実測で検証(詳細 `docs/vm/EXCEPTION_PARITY.md`): (1) 型のみ不一致
4件(`convert(Int,"a")`が TypeError vs upstream MethodError など — #10481 が
`sqrt`向けに閉じた同クラスが `convert` で再現し、「funnel 不在」を裏付ける)。
(2) 発生層の乖離は確認したが縮小傾向 — Evidence 記載 3件中2件(#10406/#10511)
は既に closed で元 MWE のまま upstream と完全一致(corpus に regression
sentinel として常設)、残り #10593 のみ再現。(3) `@test_throws` の型無視
(#10354)が検出網の盲点であることをスローアウェイパッチ(未同梱、適用→計測→
revert)で定量化: 161 fixture 中 7 file / 21 中 13 assertion が Pass→Fail に
反転し、julia 実行との突き合わせで **13/13 が genuine sjulia bug**(fixture
過剰指定はゼロ)。`regex_recursion_reject_10181.jl` の残り 8 assertion は
`@test_throws "message"`(メッセージ部分一致形式、upstream `do_test_throws`
の第2形態)が未実装なことによる計測アーティファクトと判明 — 真の修正には
`isa T` と `String`/`Regex` 部分一致の両方が必要。

Phase sub-issue 4件を `#10813` にネイティブリンク: 1a は既存 #10354 を採用
(コメントで棚卸し証跡を追加)、2a(#11146 taxonomy funnel)/2b(#11147
raise-layer sweep)/3(#11148 generic-fallback sweep + enforcement ratchet)を
新規起票。本 PR に Rust 本体変更なし(スローアウェイパッチは revert 済み、
`Test.jl` の diff はゼロ)。`scripts/exception_parity_probe.{sh,py}` は
report-only・never-gating(Phase 3 でエンフォースメント化)。

### AoT vm_aot 差分レーンの拡張 + 常時ゲート化 (Issue #10815)

`scripts/metamorphic_equivalence.sh --lane vm_aot`(VM vs
`juliars --minimal-prelude --emit-binary`)の corpus
(`tests/equivalence/vm_aot.tsv`)を **3 → 11 ケース**に拡張: 既存だが
未参照だった `builtin_stdout_parity_6999.jl`/`mandelbrot_scalar_aot.jl`、
および新規 5 fixture(Bool/比較/単項演算子、gcd/lcm/factorial、String
連結、user-defined 再帰関数、break/continue、いずれも upstream julia +
VM + AoT の三者一致を確認、`subset_julia_vm/tests/fixtures/aot/manifest.toml`
にも登録)。既知の乖離 `scope_sibling_rebind_10251`(sibling for-loop の
同名 local が first-seen 型に統合され InexactError で panic、対 #10523)を
`docs/vm/EQUIVALENCE_KNOWN_DIVERGENCES.tsv` に two-sided 登録。

拡張作業自体が新規 AoT bug 3 件を検出・起票: #11180(`Complex` prelude が
無条件に emit する `const im: Complex` が、Rust の pattern-position 識別子解決
規則により同名ローカル引数と衝突 — 型推論側 `lookup_global_or_const` は
scope-aware なのに Rust 識別子 emit 側は非対応)、#11181(`prelude_aot.jl`
文書化済みの `range(start,stop,length)` が実際には呼び出し不能・E0425)、
#11182(`v[i] = expr` が read 用の bounds-checked block 式をそのまま
lvalue として emit し E0070 — `AotStmt::Assign` は Dict のみ特別扱い)。
副次的に `--features aot` テストビルドが `main` 上で既に赤(#11196、PR
#11005 の `new_struct_name`/`global_new_helpers` フィールド追加が
`#[cfg(test)]` 27 サイトに未反映 — 誰も `test_aot.sh` を定常運用して
いなかったため不可視だった)を発見し、機械的に修正(値はいずれも
`None`/`Vec::new()`、他の既存サイトと同じ規約)。

**強制**: `scripts/test_aot.sh`(AGENTS.md hard rule #8 の必須 AoT ゲート)
に新規ステップ [4/8] `sjulia --features repl` ビルド、[5/8] `--lane vm_aot`、
[6/8] `--selftest` を追加(`--no-metamorphic` でスキップ可)。従来
`--lane vm_aot` を実行するのは lead 認証時の `premerge_gate.sh
--metamorphic`(`subset_julia_vm/src/*` 変更で自動選択)のみで、
mandatory な per-PR ローカルゲートは一度も回していなかった —
#10870/#10912 と同型の「ゲートは存在するが誰も回していない」欠落。
`scripts/check_test_aot_vm_aot_lane.sh`(source-only、
`scripts/source_only_audits.tsv` に `test_aot_vm_aot_lane` として登録、
negative selftest 2 件を `check_audit_negative_selftest.sh` に追加)が
`test_aot.sh` の両呼び出しの存在と corpus 行数下限(11)を ratchet。

再実装クレーム(AoT が scope/型導出を VM と独立に再実装している)は
単一 PR で閉じられる規模ではないため、3 件の decomposition issue に
分割: #11195(scope/binding-identity 統合、#10251/#10523/#11180 が
同一根本原因の 3 独立バグであることを示す、着地順 1)、#11200
(statement/assignment-target lowering の `SharedFunctionPlan` 統合検討、
#10796/#11182 がその証跡、#11195 に依存、着地順 2)、#11202(生成 Rust
の所有権規約の文書化、対 #10663、優先度最低・独立)。元 issue の P0
(`--pure-rust` smoke)は #10731 が未修正のため意図的に見送り —
今 wire すると即ゲート赤化するため、#10731 修正と同一 PR で追加する。
### techdebt(#10459) Phase 2a continuation: 12 個の named module/global テーブルの scope judgment (Issue #11032)

#10988 が re-scope した 12 テーブル (`module_functions`/`exports`/`constants`/
`struct_names`/`usings`/`abstract_names`, `module_imported_bindings`,
`global_types`/`inference_global_types`/`global_const_structs`,
`global_struct_names`, `module_aliases`) を個別に判定。**結論: 12 個とも
`ModuleId` 再キー化の対象ではない**。11 個は完全修飾モジュールパス(または
それを組み合わせた複合キー)で構成上衝突不可能 — `register_module_ids` が
intern するパスとバイト同一(既存の
`register_module_ids_matches_collect_module_info_paths_issue_10988` で pin
済み)。残る bare 名キー3テーブル (`global_types`/`inference_global_types`/
`global_const_structs`) は upstream Julia 1.12.6 との MWE 比較で実害なしと確認
(前者は型不一致時に `ValueType::Any` へ widen する既存の安全網、後者は参照側が
実行時の値から解決するため)。

唯一の実バグ: `module_aliases` を構築する `imported_submodule_aliases`
(`core_compiler.rs`) が非決定的な `HashSet` 反復順で同名サブモジュール
エイリアスの衝突を「後勝ち」させていた(upstream は「先勝ち + warning」)
— Issue #11176 として起票・修正。既に利用可能な source-ordered
`resolved_usings: &[ResolvedUsingImport]` を使い `entry().or_insert()`
(先勝ち) に書き換え、新規 `ModuleId` 型は不要だった(#10989/#10990 と同型の
「real bug の方を直す」着地)。回帰 fixture: 両方向の import 順序を固定する
`modules_submodule_alias_first_using_wins_11176`。

`check_name_based_lookup.sh` は `module`/`global` ドメインを一切ゲートしていない
(6パターンとも `typevar`/`struct` 専用)ため、baseline の移動はゼロ
(そもそも動かせる baseline が存在しない)。`SEMANTIC_ID_INVENTORY.tsv`:
module ドメイン 55(変化なし)、global ドメイン 63(変化なし) — 総計の差分は
本 Issue の diff と無関係な並行作業のドリフトのみ。詳細な per-table verdict は
`docs/vm/SEMANTIC_ID_MIGRATION.md`「Phase 2a continuation (Issue #11032)」節。
### `@test_throws` が期待する例外を実際に検査するようになった (Issue #10354 / #10813 Phase 1a)

`subset_julia_vm/src/julia/stdlib/Test/src/Test.jl` の `@test_throws` マクロの
`catch` 節が `_test_record!(true, ...)` を無条件に呼んでおり、期待型 `T` を
一度も検査していなかった(#10813 Phase 0 が定量化した検出網の盲点)。upstream
`do_test_throws`(`julia/stdlib/Test/src/Test.jl`)に準拠する形で実装し直し、
以下の全形態をサポート: **Type**(`isa` 判定、抽象型は具象サブタイプも許容)、
**例外値**(型 + 全フィールドの `isequal` 一致 — `nfields`/`getfield` で
upstream `isequalexception` を再現)、**String**/**Regex**(`sprint(showerror,
e)` の部分一致/マッチ)、**Array/Tuple**(全要素一致)、**Function**
(メッセージに適用し `true` を要求)。ミスマッチ時の Fail メッセージは
upstream 同様 `Expected: T / Thrown: U` 形式で両方を明示する。ヘルパー
(`_test_throws_matches`/`_test_throws_describe`/`_test_throws_thrown_describe`)
はマクロの quote 展開が呼び出し側スコープで走るため `_test_result` と同様に
`export` が必要だった。

Test.jl 変更は **161 fixture 全ての `@test_throws` の挙動を変える**ため、
#10813 Phase 0 が計測した「7 file / 13 assertion が Pass→Fail に反転する」
問題を同一 PR で解消(#10813 の "Sequencing note" 通り、harness 修正と型修正を
分離しない)。13 件を精査し、11 件(6 fixture)は「この呼び出し箇所で正しい
例外クラスを送出する」局所修正で upstream 一致に到達 — いずれも ad hoc
special-case ではなく構造的な直し:
- `dispatch/subtype_isa_arity_5493.jl`(4件): `<:`/`isa` builtin の arity
  検査(`vm/builtins_types.rs::check_builtin_arity`)が
  `VmError::TypeError("ArgumentError: ...")` という誤ラベルのバリアントで
  投げていたのを既存の `VmError::ArgumentError` に修正。
- `generator/generator_trait_matrix_9566.jl`(3件): `length(f::Flatten)` が
  未実装で `MethodError` になっていたのを、upstream
  `flatten_length`(`julia/base/iterators.jl`)の値レベル版として実装
  (`_flatten_length_from_first`: NTuple/Number は計算、それ以外は
  upstream 同様 `ArgumentError`)。
- `memory/memory_single_arg_methoderror_10324.jl`(1件 = Issue #10737):
  `Memory{T}(undef)` の `undef` はどの global にも解決できず
  `compile_memory_constructor` が汎用 `compile_expr` で解決を試みて
  `UndefVarError` になっていた(1 引数 ctor は元々 `MethodError` を送出する
  意図だった)。`is_undef` 検出時は `Instr::PushUndef` を直接 emit するよう
  修正 — 同ファイルの 2 引数 `Memory{T}(undef, n)` 分岐が既に使っている
  パターンと同じ。**#10737 をこの PR で root-cause fix、close。**
- `memory/test_memory_primitive_boundary.jl`(1件): `NewMemoryDynamic`/
  `NewMemoryDynamicTyped` の負サイズチェックが同じ「`TypeError` に
  `"ArgumentError: "` 文字列を埋め込む」誤りだったのを `VmError::ArgumentError`
  + upstream と一致するメッセージ("invalid GenericMemory size: ...")に修正。
- `modules/module_selective_using_globals_7955.jl`(1件 = corpus
  `undef_var_call`): 未定義の名前を**呼び出す**位置("Unknown function: X")が
  汎用 `ThrowError`(`ErrorException`)にフォールバックしていたのを、新規
  `Instr::ThrowUndefVarError(String)`(`ThrowMethodError` と同じ形)で
  `VmError::UndefVarError` を直接送出するよう修正。同名を**読む**位置は
  既に正しく `UndefVarError` だった — 呼び出し位置固有のギャップ。
- `array/ncat_double_semicolon_line_wrap_10519.jl`(1件): `_block_vcat`
  (`julia/base/array.jl`)の列数不一致チェックが `error()`
  (`ErrorException`)を投げていたのを `throw(ArgumentError(...))` に変更
  (メッセージ文言は upstream の hvcat 実装と別経路のため完全一致はしないが
  型は一致)。

残り 2 件(`types/signature_forward_reference_11025.jl`)は局所修正の対象外と
判断し `@test_broken` でトラック(assertion 自体は upstream 正解
`e isa UndefVarError` のまま弱めていない): `eval` によるランタイム関数定義の
型付きパラメータが `vm/builtins_macro/eval.rs` で未実装
(`VmError::NotImplemented` — closed #8647 の残存ギャップ)で、これは
Issue #8664 の設計判断により Julia 例外オブジェクトを持たず
`typeof(caught) == String` になる。個別呼び出し箇所の型選択ミスではなく
機能未実装 + アーキテクチャ判断であり、#11146(taxonomy funnel epic、
issue 本文で本ケースを "its own distinct defect" として明記済み)の scope。

regex/regex_recursion_reject_10181.jl の 8 assertion(`@test_throws
"message"` 形式)は今回の String マッチ実装で自然に green化(型に依存しない
メッセージ部分一致のため)。

回帰カバレッジ: `tests/testset_exit_code_8191_tests.rs` に
`test_throws_checks_expected_exception_10354`(全形態 × 正しい/間違った
期待値のペア、Fail 側は fixture harness の failing-testset gate — Issue
#9360 — のため fixture化できず Rust 統合テストに)と
`test_throws_fail_message_names_expected_and_thrown_10354`
(`Expected:`/`Thrown:` 文言の存在確認)を追加。green 側は新規 fixture
`stdlib/test_throws_type_check_10354.jl`(全形態の正しい期待値、julia
パリティ確認済み)。`Instr::ThrowUndefVarError` 追加で Base cache schema
fingerprint が変わるため `CACHE_VERSION` を bump
(`subset_julia_vm_compile/src/compile/precompile.rs`)。

### 14件目: arrow lambda の keyword parameter lowering (Issue #10354)

型検査は一度きりの監査ではなく**常設**の検出網である、という実証。main への
rebase 時点で 14件目の bug を即座に検出した。#11135(PR #11155、本ブランチの
分岐後に main へ landing)の fixture
`kwargs/annotated_kwarg_default_type_11135.jl` は arrow lambda
`(y, x = 2; k::Integer = "oops") -> (y, x, k)` に対し
`@test_throws MethodError` を主張しており、これは upstream 通りで**正しい**。
だが sjulia は `UndefVarError` を投げていた。main 上でこの assertion が
"pass" していたのは harness が型を見ていなかったからで、fixture 作者には
知りようがなかった。

根因は例外分類の選択ミスではなく、**lowering の重複**: arrow (`->`) の
parameter collector 2箇所(`lowering/function/short_form.rs` /
`lowering/expr/call.rs`)が、named function 用に
`lowering/function/signature.rs` が既に持っている post-`;` keyword parameter
の match を、それぞれ**部分的にコピー**していた。そこから独立した 2つの
乖離が発生:
(1) 型注釈付き keyword(`k::Integer = 3`)は LHS が `TypedExpression` の
`Assignment` として parse されるため、コピー側のどの arm にも当たらず
`_ => {}` で**黙って捨てられ**、keyword が signature から消滅 — body の `k` が
global load になり `UndefVarError`、`k = 3` を渡すと "unsupported keyword
argument"(いずれも upstream は正常動作)。コピー側は declared type も
`None` 固定で捨てており、#11024/#11081 の処理が arrow に一切届いていなかった。
(2) parser の `Assignment -> KwParameter` rewrap が arrow の parameter list
**全体**に適用されていたため、`;` より前の**位置引数のデフォルト**
(`(y, x = 2; k = 3) ->`)まで keyword に化け、`f(1, 5)` が
`NoMethodFound`(upstream は `(1, 5, 3)`)。

修正は構造的に: `signature::parse_kwparam_node` を単一の authority として
抽出し、arrow の collector 2箇所を両方そこへ通す(ローカルの
`lower_arrow_kwparam` は削除)。parser の rewrap は `Semicolon` で分割し、
`;` 前の `Assignment` は位置引数デフォルトのまま残す。重複した shape 判定こそが
#11024/#11081/#11135 を経て arrow と named function を乖離させた原因であり、
authority の一本化が再乖離を防ぐ。arrow の全 11 形態が upstream julia 1.12.6
と完全一致し、`annotated_kwarg_default_type_11135.jl` は 47/48 → **48/48**
(upstream と同一)に。**assertion は一切弱めていない。**
回帰 fixture: `kwargs/arrow_lambda_kwparams_10354.jl`。

絞り込み中に発見した構造の異なる第3の shape(匿名
`function (y, x=2; k=3) ... end` **値**形式が parameter list ごと失われる)は
本PRでは修正せず **Issue #11174** として起票。
