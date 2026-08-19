# SubsetJuliaVM — アーキテクチャ概要

*最終更新: 2026-07-12*

この文書は、新しいコントリビューターが最初に読むべき単一の入り口です。
SubsetJuliaVM が何であるか、Julia プログラムがどのように流れるか、実装の各構成要素が今どこにあるか、そして次に読むべき詳細文書はどれかを説明します。
この概要と個別トピックの文書が矛盾する場合は、個別トピックの文書（あるいはソースコード）が勝ちます。
docs の issue を起票してください。

---

## 1. SubsetJuliaVM とは何か

SubsetJuliaVM（sjulia）の標準実行経路は、Julia の厳密な部分集合を iOS、WebAssembly、CLI で VM バイトコードとして実行します。
iOS の C ABI と WebAssembly の wasm-bindgen binding は、実行時のネイティブコード生成に依存しません。
この経路は、パーサー、ロワリング、コンパイラをアプリケーションに含め、ソースを CompiledProgram に変換してから Vm が解釈します。
一方、aot feature は Core IR から Rust を生成する別経路であり、cranelift feature には object 出力とデスクトップ向け JIT API があります。
後者は標準の iOS、WebAssembly、CLI の VM 経路には含まれません。

```
Julia source
    │
    ▼
┌─────────┐   ┌──────────┐   ┌──────────────┐   ┌────┐
│  Parser │ → │ Lowering │ → │   Compiler   │ → │ VM │
└─────────┘   └──────────┘   └──────────────┘   └────┘
                                                   │
                              Swift/iOS via C ABI ◄┘
                              (plus WASM binding and opt-in AoT)
```

以下の 2 つのコミットメントが、以降のすべてを形作っています。

- **出力の整合性。**
  プログラムの出力は公式 Julia と一致しなければなりません。
  目標はバイト単位の一致であり、すべての fixture を `julia` と sjulia の両方で実行して検証します（`scripts/fixture_julia_parity.sh`、North Star 指標 NS-1）。
- **上流駆動の設計。**
  実装選択が不明瞭な場合は、`julia/` に含まれるベンダー提供の上流ソースを読み、ad hoc な特別扱いではなく同じ一般的な仕組みを採用してください。
  2026-07 のいくつかのエピック（型インターン、isbits アンボックス化、永続的 REPL、ジェネレーターの脱糖）も、このルールの直接的な適用です。

---

## 2. ワークスペース構成：クレート

ワークスペースは、コンパイラ・共有プログラム表現・実行時が独立して進化（および再構築）できるように層化されています。
Cargo の依存関係は、この図で厳密に下向きに張られています（矢印は「依存する」を意味します）。

```
subset_julia_vm_ffi          subset_julia_vm_web
(C ABI staticlib/cdylib)     (wasm-bindgen bindings)
        │                            │
        └────────────┬───────────────┘
                     ▼
subset_julia_vm            (integration crate)
  integration crate: pipeline, compile/, vm/, REPL, AoT, .jl embeds,
  public API — optionally depends on subset_julia_vm_runtime
  (AoT generated-code runtime support) behind the "aot" feature
                     │
                     ▼
subset_julia_vm_bytecode   (shared program representation)
  shared program representation: Instr, the full Value model,
  ValueType/ArrayElementType, VmError, rng, slot/peephole finalizers,
  CompiledProgram, wire IDs
                     │
                     ▼
subset_julia_vm_types      (type system and inference primitives)
  JuliaType, CoreType + subtype/dispatch resolver (inference_core/),
  LatticeType lattice algebra, promotion, inference cache keys
                     │
                     ▼
subset_julia_vm_ir         (span + error layer)
                     │
                     ▼
subset_julia_vm_parser     (lexer/parser/CST)
```

実務上重要な注意点は以下の通りです。

- `subset_julia_vm_bytecode` は **stable な serde ワイヤー層**です。
  Base/prelude キャッシュはこれらの型をシリアライズします。
  ワイヤー識別子（`BuiltinId`、`Intrinsic`）は `compile/instr_wire_ids.rs` と `scripts/check_instr_wire_ids.sh` で固定されており、スキーマに影響する変更には `CACHE_VERSION` の更新が必要です。
  詳細は `CHECKLISTS.md` の「クレート分割・モジュール移動時の影響チェック」を参照してください。
- `subset_julia_vm_ffi` は `[lib] name = "subset_julia_vm"` を維持しているため、Xcode は引き続き `libsubset_julia_vm.a` を生成します。
  ワークスペースの `default-members` からは除外されているため、単なる `cargo build` では staticlib/cdylib の成果物はビルドされません。
- 残りの分割作業（統合クレートから `subset_julia_vm_compile` と `subset_julia_vm_vm` を物理的に切り出すこと）は Issue #9090 で追跡されています。
  前提条件はすでに満たされ、ラチェットとして維持されています。
  `scripts/audit_compile_vm_coupling.sh` は、直接結合を `compile_to_vm = 0`、実行時の `vm_to_compile = 0`、ソーステストの `vm_to_compile_tests = 0` に抑えています。
  現在の #9090 の活動は、機械的な可視性スイープです。
  `repl/`、`api/`、`ffi_support`、`macro_runtime`、テスト、ベンチマーク、バイナリにおける `Value` モデルの import を、`crate::vm` の再エクスポートではなく `subset_julia_vm_bytecode` 経由に直接ルーティングし、`crate::vm` への参照を実際の実行時境界（`Vm` 実行、VM メモリ統計、フォーマット、linalg 実体化）だけに縮小します。
  REPL に残る内部ヘルパーは、内部の `pub(crate)` に手を伸ばすのではなく、専用ファサード（`compile::repl_support`、`vm::repl_support`）を経由する必要があります。
  また、ホストのコンパイル/キャッシュエントリポイント（ベンチマークや例を含む）は、直接の `compile`/`compile::cache` 内部ではなく `compile::host_support` を通ります。
- 詳細な所有権テーブル、移行フェーズ、ビルド時間の測定値は `CRATE_SPLIT.md` に記載されています。

---

## 3. パイプライン：ステージごとの解説

### 3.1 パーサー（`subset_julia_vm_parser`、`src/parser/` の薄いアダプタ）

パーサーは Julia のソーステキストを **Concrete Syntax Tree（CST）** に変換します。
subset_julia_vm_parser は lexer、parser、CST、span、エラー回復を pure Rust で所有します。
メインクレートの parser モジュールは、その Parser の結果をパイプラインが扱う ParseOutcome に包みます。
WebAssembly binding も同じパーサークレートを依存関係として使います。
対応範囲の数値はコーパスのバージョンと計測時点に依存するため、この概要では固定値を記載しません。

### 3.2 ロワリング（`src/lowering/`）

ロワリングは CST を **Core IR**（簡潔で正規化された表現）に変換します。

- 構文糖衣を展開します。
  短形関数、`.=` ブロードキャスト、`where` 句、ジェネレーター式（上流と同じ形の脱糖で `Base.Generator` / `Iterators.Filter` / `Iterators.Flatten` へ、Issue #9200）です。
- 型注釈を `JuliaType` ノードに解析します。
- ネストした関数定義とクロージャー捕獲を収集します。
- `MacroExpander` の継ぎ目（`lowering/macro_expander.rs`）を通じてマクロを展開します。
  マクロの **実行時** は統合クレートのルートに存在し、object-safe なトレイトを介してインストールされるため、`lowering/` は VM への上向き依存を持ちません。
- 未対応の構文エラーを正確なソース span とともに報告します。

エントリポイントは `Lowering::lower(parse_outcome) -> LowerResult<Program>` です（`lowering/mod.rs`）。
`include` 対応版は `LoweringWithInclude::lower` です。
詳細は `LOWERING.md` を参照してください。

### 3.3 コンパイラ（`src/compile/`）

コンパイラは Core IR を平坦な VM バイトコード命令（`Instr`）の列に変換します。
サブステージは以下の通りです。

| フェーズ | 場所 | 責務 |
|----------|------|------|
| メソッドテーブル構築 | `compile/mod.rs` | すべての関数シグネチャを登録する |
| 抽象解釈 | `compile/abstract_interp/` | 関数の戻り値と呼び出しサイトの絞り込みのための lattice ベース推論 |
| 推論トレース（開発者向け） | `compile/inference_trace.rs` | `infer_with_trace(...)` — 1 関数についてステートメントごとの環境スナップショットを取得する。Julia の `typeinf_code` に類似（Issue #3512）。オプトインしない限りゼロコスト |
| コアコンパイル | `compile/core_compiler.rs`、`compile/stmt.rs`、`compile/expr/` | 式/文ごとにバイトコードを emit する |
| 式レベル推論 | `compile/expr/infer/` | 各式の `ValueType` を決定する |
| ディスパッチ選択 | `compile/expr/binary/`、`compile/expr/call/` | 呼び出しサイトごとに静的ディスパッチか実行時ディスパッチかを選ぶ |
| 定数伝播 | `compile/const_prop/` | コンパイル時に定数を評価する。const SSA 呼び出しのための `is_foldable` 具体的評価（Issue #9497） |
| 効果推論 | `compile/effects/` | DCE および CSE を制御する副作用/一貫性ビット。§8.2 を参照 |
| SSA ロワリング + 共有プラン | `compile/ssa_ir/`、`SSA_IR.md` | 両バックエンドが消費するバックエンド中立の `SharedFunctionPlan`（型定義は `subset_julia_vm_bytecode/src/shared_plan.rs`）を生成する（Issue #9089） |
| union 分割 | `compile/union_split/` | union 型に対するディスパッチを分割する |
| 転送関数 | `compile/tfuncs/` | Julia の `add_tfunc` を模した、arity/cost メタデータ付きの型レベル転送ルール（Issue #3509） |
| 型安定性解析 | `compile/type_stability/` | 型不安定なコードパスを検出する |
| プリコンパイル済みキャッシュ | `compile/precompile.rs`、`compile/cache.rs` | 起動時の prelude/Base の再コンパイルをスキップする。§9 を参照 |

出力は `CompiledProgram`（`Vec<Instr>` + メタデータ）であり、`subset_julia_vm_bytecode::program` が所有します。

codegen やパフォーマンス作業では、実行時の高速パスに触れる前に最終的なバイトコードをダンプしてください。
コマンドは `cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode <file.jl>` です。

### 3.4 バイトコード境界（`src/bytecode.rs` ファサード）

コンパイラコードは、プログラム表現を `bytecode` ファサードを通じて import し、`src/vm/` パスを介して import することはありません。
ファサードは、バイトコードクレートが所有する ISA、プログラムメタデータ、Value モデル、スタック終了化ヘルパーを再エクスポートします。
結合監査は、直接の `compile → vm` 参照をゼロに固定しています。
多くの歴史的な `vm::…` モジュールパスは再エクスポートエイリアスによって有効なままですが、`#9090` のスイープで pure ヘルパー系のファサードパス（`vm::util::parse_parametric_params`、`vm::typed_scalar_binary_instr`、実行時シグネチャ投影）は削除されました。
新しいコードは、これらをバイトコードが所有する実装に直接 import してください。

### 3.5 VM（`src/vm/`）

VM は `CompiledProgram` を **スタックベースのバイトコードインタプリタ**で解釈します。
レジスタ VM は同じ Vm に組み込まれた opt-in の実行器であり、SJULIA_REGISTER_VM またはホスト API の強制指定があるときだけ適格な直接呼び出しを処理します。
変換できない関数は通常のスタック VM へフォールバックするため、レジスタ VM は標準経路の既定実装ではありません。

- **設計上シングルスレッド**です（`SINGLE_THREADED_VM.md`）。
  VM/セッションインスタンスは `Rc`/`RefCell`/`thread_local!` な状態を使用でき、設計文書に別段の記載がない限り `Send`/`Sync` を保つ必要はありません。
- **型付き実行ブロック**（`vm/executable.rs`）：小さな型付きバイトコードループを、保守的に事前デコードし、ループローカルスロットを持つ融合スカラー演算にまとめます（`Complex` コンストラクター形や早期リターン形も含む、Issue #9654）。
  該当しない場合はスタックインタプリタにフォールバックします。
- **実行時特殊化**（`vm/specialize/`）：型注釈のない関数は `CallSpecialize` を通じて、実行時に現れた `ValueType` の組に合わせた専用バイトコードを生成・キャッシュします。
  broadcast は呼び出し地点の配列型（例：`Matrix{ComplexF64}`）を per-element callee へ伝播する一括型付きカーネルを持ちます（Issue #10704）。
- **panic 禁止の規律**：実行時エラーは span を持つ `VmError` 値として返され、Rust の panic ではありません。
  ラチェットは `PANIC_FREE.md` にあります。

主な構成要素は以下の通りです。

| ファイル/ディレクトリ | 責務 |
|----------------------|------|
| `vm/mod.rs` | `Vm<R>` — 最上位の実行ループ |
| `vm/exec/` | 命令ごとのハンドラー（件数は手動管理の数字ではなくソースクエリで確認すること） |
| `vm/dispatch.rs`、`vm/dynamic_ops/` | 実行時ディスパッチ。§6 を参照 |
| `vm/executable.rs` | 事前デコードされた型付きループブロック |
| `vm/builtins_*.rs`、`vm/builtins_{macro,reflection,sets}/` | ビルトイン実装 |
| `vm/matmul/`、`vm/hof_exec/`、`vm/type_ops/`、`vm/specialize/` | 行列乗算、高階関数、実行時型演算、値特殊化 |
| `vm/formatting.rs` | `format_value()` / `format_sprintf()` |
| `vm/register_gate.rs`、`src/register_vm.rs` | レジスタ VM ゲート。§8 を参照 |

詳細文書：`CALL_INSTRUCTIONS.md`、`PANIC_FREE.md`、`VM_MEMORY_MANAGEMENT.md`。

### 3.6 主要データ構造（最初に読むべきファイル）

- **`CompiledProgram`**（`subset_julia_vm_bytecode/src/program.rs`） — コンパイラの出力かつ VM の入力。
  `code: Vec<Instr>`（すべての関数が平坦に連結）、`functions: Vec<FunctionInfo>`（開始インデックス、arity、名前、オプションの `shared_plan`）、`struct_defs`、メインスクリプトの `entry` インデックス、グローバルスロット/show メソッドのメタデータを持つ。
- **`Frame`**（`subset_julia_vm_vm/src/vm/frame.rs`） — 関数呼び出しごとに 1 つ。
  ボックス化された `Value` ローカルスロットに加え、スロット化されたローカル用のボックス化解除された型付きスロットベクトル（`slot_i64`、`slot_f64`、…）、リターンアドレス、エラーメッセージ用の関数名を持つ。
- **`CoreCompiler<'a>`**（`subset_julia_vm_compile/src/compile/core_compiler.rs`） — 主要なコンパイル状態。
  登録済みの `method_tables` と `SharedCompileContext`（struct 定義、グローバル型、show メソッド）を借用し、`self.code: Vec<Instr>` に emit する。
- **`Instr`**（`subset_julia_vm_bytecode/src/instr.rs`） — スタックベースの命令セット（push/演算/分岐/呼び出しの亜種。呼び出し系は §6.1 を参照）。
  大きな enum なので、variant 数は文書に数字を書き込むのではなくソースクエリで確認すること。

---

## 4. Pure Julia と Rust：3 つのレイヤー

```
Layer 3 — Pure Julia    subset_julia_vm/src/julia/
           Base, stdlib, bundled packages — same file paths as upstream

Layer 2 — Rust VM       subset_julia_vm_vm/src/vm/
           Dispatch machinery, display, array carriers, built-ins

Layer 1 — Intrinsics    subset_julia_vm/src/intrinsics.rs
           CPU instructions (add_int, mul_float, …), memory primitives
```

**ルール（"Pure Julia First"）**：移植対象の上流ファイルと同じパスで、まずレイヤー 3 に実装してください。
Rust ビルトインは、その操作が Julia で表現できない場合（ファイル IO、ハッシュ、CPU レベルの算術）にのみ追加してください。
`BUILTIN_REMOVAL.md` は、既存の Rust ビルトインを Pure Julia に移行する方法を記録しています。
設計の根拠は `PURE_JULIA_DESIGN.md` に、どのハンドラーが何を所有するかは `BUILTIN_OWNERSHIP.md` に記載されています。

---

## 5. 型システム

### 5.1 3 つの表現

sjulia は、パイプラインの異なる段階で 3 つの型表現を使用します。
これらはすべて Julia 型システムの lossy な投影です。

```
コンパイル時                    バイトコード境界           実行時
────────────────────────────    ─────────────────────       ─────────────────
LatticeType                     ValueType                   Value
(抽象解釈)                      (命令オペランド)             (タグ付き union)
```

- **`LatticeType`** — `subset_julia_vm_types/src/runtime_types/lattice.rs`。
  抽象解釈器の lattice：`Bottom`、`Const(v)`、`Concrete(T)`、幅制御付き union、フロー感性条件、`Top`。
  この lattice は意図的に平坦であり（無限に上昇する鎖がない）、標準のバイトコード VM で推論を終了可能にする設計判断です（`LATTICE_TYPE.md`）。
- **`JuliaType`** — `subset_julia_vm_types/src/types/`。
  コンパイラが式に対して持つ Julia レベルの視点。
  パラメトリックな形（`VectorOf`、`TupleOf`、`Struct(name)`）を含み、`ValueType` に集約される前の表現です。
- **`ValueType`** — `subset_julia_vm_bytecode/src/value_type.rs`。
  オペランドレベルの約 50 variant のタグ（`I64`、`F64`、`Struct(type_id)`、`Any`、…）。
  `ValueType::Any` は「静的には分からない — VM が実行時にディスパッチする」を意味します。
- この 3 つ間の橋渡し変換は、`runtime_types` ファサード（`runtime_types::bridge`）に集約され、モジュールごとの ad hoc ヘルパーには存在しません。

詳細文書：`TYPE_SYSTEM.md`、`NUMERIC_TYPES.md`、`PROMOTION.md`。

### 5.2 インターンされた型同一性（Issue #9197、2026-07）

実行時の型同一性は、以前は文字列とハッシュに基づいていましたが、現在は **インターン**されています。
`subset_julia_vm_bytecode/src/type_intern.rs` は、各具体的な型に安定した整数 `ConcreteTypeId` を割り当て、「同じ型か？」は整数比較になります。
これは上流の `jltypes.c` における型キャッシュと同じ形であり、型の等価性はポインタの等価性です。
ディスパッチキャッシュへの影響は §6 で述べます。
スライスごとの詳細は設計文書 `TYPE_INTERNING.md` にあります。
残る文字列キー表面（メモリ内の `HashMap<String, MethodTable>` テーブルと `vm/dispatch.rs` の低頻度な解決パスパーサー）は、そこで文書化されたフォローアップとして追跡されています。

### 5.3 `runtime_types` ファサード

`subset_julia_vm/src/runtime_types/` は、共有の実行時型システム表面です。
型インターン、`MethodTable`/`MethodKey` データ、`ExceptionType`、`Effects`/`EffectBit`、`TypeEnv`、パラメトリック型引数推論、橋渡し変換を含みます。
新しいコードは、このファサードに依存すべきであり、新たな直接の `compile ↔ vm` import を増やすべきではありません。
結合監査がその方向を強制します。

---

## 6. ディスパッチシステム

### 6.1 呼び出しパス

すべての呼び出しサイトは、3 つの形のいずれかにコンパイルされます。

```
Call site
   │
   ├─ Static ────────────► Instr::Call(func_index, nargs)
   │   (types fully known)   direct function call
   │
   ├─ Dynamic ───────────► Instr::CallDynamic(…) / CallDynamicBinary*(…)
   │   (some args Any)       shared-resolver eligibility + ranked candidates
   │
   └─ Typed dispatch ────► Instr::CallTypedDispatchOrBuiltin*(…)
       (user methods may     runtime signature match with builtin fallback
        shadow builtins)
```

第一級関数値（`Instr::CallFunctionVariable*`）は、呼び出しサイトごとの汎用ディスパッチキャッシュを持ちます（Issue #9739）。

### 6.2 適格性判定、その後ランキング

実行時ディスパッチは、候補を **共有ディスパッチリゾルバー**（`subset_julia_vm_types` の `inference_core/dispatch_resolver.rs`）が実装する CoreType typemap/サブタイプ関係を通じてフィルタリングします。
Issue #8548 以降、適格性判定は構造的です。
候補はランキングが考慮される前に CoreType シグネチャチェックを満たす必要があります。
スコアはすでに適格な候補を順序付けます。
スコアは不適格な候補を合格させることはありません。

**ルール**：新しい動的ディスパクトハンドラーは、必ず共有リゾルバー（`resolve_runtime_core_signature_candidates*()` / `resolve_callable_value_candidates()`）を通す必要があります。
インラインスコアリングを書いてはいけません。
詳細は `BINARY_DISPATCH.md` と `CALL_INSTRUCTIONS.md` を参照してください。

### 6.3 キャッシュと無効化（post-#9197）

- **L1** — 呼び出しサイトごとのインラインキャッシュ。
  キーはインターンされた `ConcreteTypeId` の厳密な一致です。
  ヒットは定義により厳密一致です。
  古い未検証ハッシュによる確率的マッチングは廃止されました。
- **L2** — インターンされた型 ID 列をキーとする共有ディスパッチキャッシュ。
  上限付きの追い出しがあります。
- **First-arg index** — sealed-primitive な第一引数 typemap（`FirstArgIndex`）が、シグネチャチェック前に候補集合を絞り込みます（struct/abstract 第一引数インデックスは追跡中のフォローアップ）。
- **精密な無効化** — メソッドの（再）定義は、解決先が変更されたジェネリック関数に属するキャッシュエントリ（およびビルトインフォールバックエントリ）のみを破棄します。
  すべてのキャッシュをクリアするのではありません。
  無関係な温まった呼び出しサイトは温まったままです。
  これは上流の backedge 無効化（`gf.c`）のシングルスレッド版であり、永続的 REPL の world-age セマンティクスを実現可能にするものです（§8）。

### 6.4 二項演算子

二項演算子は、常に同期させておく必要がある 2 つのコードパスを持ちます。
コンパイル時選択（`compile/expr/binary/`）と実行時ハンドラー（`vm/exec/call_dynamic_binary.rs`、`binary_both.rs`、`binary_no_fallback.rs`）です。
promote フォールバックの再帰トラップと、それを守る数値行列オラクルについては `BINARY_DISPATCH.md` と CLAUDE.md の「Numeric operators」を参照してください。

---

## 7. 値、メモリ、isbits アンボックス化

### 7.1 `Value` モデル

`Value`（`subset_julia_vm_bytecode/src/value/value_enum.rs`）は、実行時のタグ付き union です。
現在のモデルのハイライト（網羅的ではありません。enum を読んでください）を以下に示します。

- 機械数値：`I8..I128`、`U8..U128`、`F16/F32/F64`、`Bool`、および `BigInt`/`BigFloat`。
- `Str(StrRef)` — 共有される不変の `Rc<str>`（clone は refcount を増やす）。
  `StrBytes` は珍しい非 UTF-8 の上流 `String` ペイロードを運ぶ。
- `Memory(MemoryRef)` / `MemoryRef(…)` — 平坦な型付きバッファ `Memory{T}` は **唯一の Rust 境界コレクションキャリア**です（Issue #6624）。
  `Array{T,N}`、`Dict`、`Set` はこれの上に構築された Pure Julia 構造体です。
  `Value::Dict` / `Value::Set` variant は存在しません。
- `Struct(StructInstance)` は不変 struct 用、`StructRef(usize)` は可変 struct 用で、VM の `struct_heap` へのインデックスです。
- `ExprArgs(ExprArgsCarrier)` — `expr.args` にのみ使用される意図的に閉じた可変配列キャリア。
  newtype witness によって hub 外での使用はコンパイルエラーになります（Issue #8918）。
- `Generator(Box<GeneratorValue>)` — ネイティブな遅延ジェネレーター表現（§8.3）。
- `DataType`、`RuntimeTypeVar`、`RuntimeTypeName`、`Module`、`Function`、`Closure`、`Expr`/`QuoteNode`/`Symbol`/`GlobalRef`（メタプログラミング）、`Regex`、`Rng`、`IO`、`StaticArray`/`StaticArrayInline`（平坦な小さな SVector/SMatrix、N≤4 で `Copy` 40 バイトインライン）。

### 7.2 isbits アンボックス化（Issue #9198、2026-07）

isbits 不変 struct（すべてのフィールドが値、例：`Complex{Float64}`）は、もはや常にヒープボックス化されるわけではありません。

- **ローカル変数**：型付きループ内の 2 フィールド isbits struct は、SROA（Scalar Replacement of Aggregates）によってスカラースロットに分解されます（`compile::complex_sroa` を Complex 以外に一般化）。
  型付きの `z = z*z + c` ループは、イテレーションあたり 21 回のヒープ確保から、インタプリタの下限 1 回まで減少しました（struct に起因する確保は 0）。
- **配列**：`T` がすべて `Float64` の isbits struct であるような `Vector{T}` は、生の連続した f64（`ArrayData::StructF64`）を保持します。
  論理的な eltype は 1 箇所に保持されます（`ArrayElementType::StructInlineF64`）。
  `Complex{Float64}` の配列もこの一般バッファを共有し、`sizeof` は上流と完全に一致します。
- **高速パス**：既存の Complex Rust 高速パスは、一般仕組みが導入された後に A/B 測定で廃止可否を評価し、**保持**されました（削除すると動的パスのベンチマークが 2.8–3.6 倍悪化したため）。
  その測定は恒久的な退行ガードです。
  設計文書：`REGISTER_VM.md` の「Multi-Slot Scalar Unboxing」。

メモリモデル文書：`VM_MEMORY_MANAGEMENT.md`、`MEMORY_PRIMITIVE.md`、`MEMORYREF.md`、`COLLECTIONS.md`。

---

## 8. 直線的なコードを超えた実行セマンティクス

### 8.1 REPL 評価モデル（Issue #9199、2026-07-08 以降のデフォルト）

本番 REPL は単一の persistent 評価モデルです（`repl/session.rs`）。
**生きた VM** が評価を横断して存続します。
グローバルは VM のバインディングテーブルに存在します（value→expression→value の往復はありません）。
式/グローバル代入入力、Main-owned method の新規定義・対応済み extension/replacement、
まったく新しい非 parametric/no-inner concrete struct、Main abstract type、非 parametric
primitive type、`@enum`、単純なユーザーモジュールは、再配置可能な差分として
コンパイルされ、生きた VM に追加されます。

definition delta は function と全 nominal family を1つの source-order transaction
として扱います。compiler は function body と concrete/abstract/primitive/enum の
整列済み metadata tail を先に構築しますが、VM は current-input nominal tail を
private reservation として保持します。`DefineEvalFunction` / `DefineEvalStruct` /
`DefineEvalAbstractType` / `DefineEvalPrimitiveType` / `RegisterEnum` だけが各 binding
を source 位置で公開します。catchable top-level error では typed activation log の
厳密な prefix を検証し、VM registry、persistent compiler snapshot、session mirror
を同じ境界へ射影します。enum の thread-local formatting registry は別所有権なので、
未 commit transaction を RAII guard で復元します（Issues #9784/#11635、cache schema
172）。live VM を取り出す前に function/specialization/activation/nominal queue の
setup を preflight し、取り出した後に拒否される部分状態を作りません。

function table での可視性は「先頭から何個が source method か」ではなく、
`ReplDefinitionActivation` が指す primary / refresh の index set が正です。
この set に属する function は source marker 到達まで world-gated、属さない
lambda/HOF・do-block・generator body/predicate helper は world 1 で即時可視にします。
helper body は function-index 整列のため compiler snapshot に保持しますが、
Julia-visible generic/method-source registry には公開しません。catchable error 時も
同じ activation index set で到達 prefix を検証します（Issue #9784）。

それ以外（Base/preload-owned method、parametric/inner-constructor struct、type
redefinition、複雑な module/import/macro/type-alias/baremodule、opaque runtime
`eval`）は、蓄積された完全な再コンパイルに安全にフォールバックします。
メソッドの（再）定義は VM の world カウンターを増やし、精密に無効化します（§6.3）。

旧 `Legacy` モデルと `EvalModel` セレクタは Issue #9784 の部分対応で削除済みです。
差分ハーネスは persistent 実行を upstream 確認済み golden と比較し、独立した2セッションの決定性も検証します。
複雑な入力向け full-recompile fallback の再注入・状態 mirror はまだ必要なため、retirement list 完了まで Issue #9784 は open のままです。
スライスごとの設計文書（どの入力形が生きた差分パスを使うかの正確な定義を含む）は `ADR_REPL_EVAL_MODEL.md` にあります。

### 8.2 効果（Issue #9205）

効果要約（DCE および CSE を制御する一貫性/純粋性ビット）は、**メソッドごと**に計算されます。
`EffectSummaries { by_name, by_method }` の `by_method` は `MethodKey` をキーとします。
これにより、純粋な `f(::Int)` が、同じ名前の不純な兄弟によって汚染されることがなくなりました（上流の per-`CodeInstance` `ipo_effects` の類似物）。
すべての `BuiltinOp` は明示的に効果分類される必要があります。
exhaustive な `match` により、分類されていない新しい variant はコンパイルエラーになります。
単体監査がその分類を保証します（Issue #9323 — 可変コンストラクターが純粋と誤分類され、2 つの `Ref(0)` セルが CSE によってエイリアス化したことが、この監査が存在する理由です）。

**reflection surface と optimizer は同一ソースを共有する（Issues #10145,
#10264）。** `Base.infer_effects`（reflection、`vm/builtins_reflection/mod.rs`
`compose_function_effects`）と DCE の whole-program fixpoint
（`compile::effects::propagation::infer_program_effects`）は、同じ
body walker（`subset_julia_vm_types::runtime_types::function_effects::
compute_function_effects` / `compute_stmt_effects` / `compute_block_effects`）
を呼び出します — 制御フロー（`if`/`elseif`/`else`、三項演算子、短絡評価
`&&`/`||`、ループ）の join ロジックは一箇所にしか存在しません。#10145 の
根本原因は歩行ロジックの重複ではなく、reflection 側だけが持つ登録範囲の差
でした: `compile::constants::needs_reflection_registration` が「全パラメータ
が具象型」なユーザー関数を `specializable_functions`（reflection が body を
再解析するための IR 保持先）に登録していなかったため、
`infer_program_effects` が無条件で解析する `program.functions
[base_function_count..]` と reflection が実際に解析できる関数集合がずれて
いました。`is_user_defined` パラメータでこの母集団を一致させ（Base/Core は
コスト回避のため従来どおり狭いゲートを維持）、両者が「同じ body walker を、
同じ関数母集団に対して」走らせることを保証しました。

### 8.3 ジェネレーター（Issue #9200）

ジェネレーター式は上流と同じ形（`Base.Generator`、`Iterators.Filter`、`Iterators.Flatten` — SIMPLE / FILTERED / PRODUCT / FLATTEN 形）に脱糖されます。
そのため、利用者は通常の iterate プロトコルの値を見ます。
**実行表現**はネイティブのままです（`BuiltinOp::Generator` + `Value::Generator` + 早期内包表記の高速パス）。
純粋な iterate コレクションに置き換える案を A/B 測定したところ、5–21 倍遅く、かつ正確性等価でもなかったため、ネイティブ表現を保持しました。
数値と判断ルールは `GENERATOR_REPRESENTATION.md` にあります。

---

## 9. バックエンドと起動

### 9.1 バックエンド

| バックエンド | 状態 | 注記 |
|-------------|------|------|
| **スタック VM** | どこでも（iOS、WASM、CLI）デフォルトの実行時 | 主にこの文書の対象 |
| **レジスタ VM** | 実験的、`SJULIA_REGISTER_VM=1` でオプトイン | `SharedFunctionPlan` を持つ関数のみをロワリング（Issue #9089）。レガシー Core-IR パス上の関数はスタック VM のまま。切り替え判断は測定でゲート（`REGISTER_VM.md`） |
| **AoT** | `aot` feature で有効化する別経路 | Core IR を解析、推論、最適化して Rust を生成する。`cranelift` feature は object 出力とデスクトップ向け JIT API を追加する。標準 VM、iOS FFI、WebAssembly binding では有効にならない |
| **WASM** | `subset_julia_vm_web` 経由で `wasm-pack` | サイズに敏感：`web-release` プロファイルが LTO 設定を所有 |
| **Wasm AoT** | 実験的、`aot-wasm` feature | 共通 AoT 解析後の backend-neutral `IrModule` を standalone core Wasm に encode。import/fallback なし。scalar + ABI v2 arbitrary-rank UInt8 descriptor subset のみ |

Issue #9089 以降、スタック VM とレジスタ VM の両方のロワリングは、1 つの共有計画 IR（`SharedFunctionPlan`）を消費します。
`FunctionInfo.shared_plan` フィールドは `#[serde(skip)]` なので、実行時のみ存在し、キャッシュのワイヤー形式は変更されません。
これにより、将来の命令セット進化に対するバックエンドごとのコストを抑えます。

Wasm AoT の linear-memory descriptor ABI v2 は 40-byte aligned header と
inline `{dim:u64,stride:i64}` rank pair を使う。UInt8 tag は stable value 1、rank
上限は 8、layout_id は 0。静的 tag/rank、checked product/extent、metadata/data
disjointness と Julia one-based axis bounds を全て検証してから load/store する。
負 stride は Todo 5 まで trap する。詳細は root の `COMPILER_SPIKE.md` を参照。

### 9.2 起動キャッシュ

コールドスタートは、Base をメモリに読み込むことに支配されています。
パイプラインは、リリースバイナリに 2 つのプリコンパイル済み成果物を埋め込みます（2 段階ビルド、CLAUDE.md の「Build & Test」を参照）。

- **prelude プログラムキャッシュ**（解析済み・ロワリング済み prelude）
- **Base バイトコードキャッシュ**（シリアライズ済みの Base bytecode）

constructor/type provenance は merge 後の span や module 配列位置から推測しません。
prelude 生成時に `StructDef::is_base_origin` を top-level / nested module へ再帰付与し、
prelude Program cache と `.sjir` に保存します。これにより cold compile、cache prime、
cache restore が同じ Base/user 境界を使います (Issue #10959; Base constructor identity の
一時 fence は W-70 / #10962)。
explicit inner constructor の self は binder 名だけでなく完全な `TypeExpr` pattern として
cache に保存し、runtime の `FunctionInfo` 名には module-qualified `Foo{...}` を使います。

キャッシュアーキテクチャ（スレッドローカルキャッシュ + レジストリ）は `CACHE_ARCHITECTURE.md` にあります。
スキーマ規律：シリアライズされる型を変更する場合は `CACHE_VERSION` を更新する必要があります。
wire ID は宣言順に依存しないように命令の同一性を固定します。
キャッシュのデコード方法や起動時間の測定値は、ビルド条件とキャッシュ形式に依存します。
性能を変更する場合は、その時点の測定手順と結果を別の記録に残します。

同じ 2 段階ビルド（キャッシュ生成 → `SJULIA_PRELUDE_PROGRAM_CACHE` / `SJULIA_BASE_CACHE` を設定して再ビルド）は、クロスターゲットの配布バイナリにも使われます。
たとえば `docker/Dockerfile.pizero-armv6` は、クロスビルドした ARMv6 バイナリ自身を qemu で実行してキャッシュを生成し、埋め込み済みの静的バイナリを出力します（Raspberry Pi Zero 1 で初回起動から高速）。

---

## 10. 正確性インフラ

- **Parity fixtures** — `subset_julia_vm/tests/fixtures/<category>/` 以下の fixture プログラムを、landing 前に上流 `julia` と sjulia の両方で検証します。
  `manifest.toml` が期待値を宣言します。
  件数と通過率は fixture、feature、上流ソースの版によって変わるため、実行時点のテスト結果を正とします。
- **North Star 指標**（`NORTH_STAR.md`） — parity 率、コーパス parse 率、iOS sample 率、ベンチマーク比、コールドスタート、フルスイートの健全性、debt barometer を追跡します。
  NS-1/NS-2 は単調なラチェットです。下げるには、書面による理由と issue が必要です。
- **監査スクリプト**（`scripts/check_*.sh`、`CODE_AUDITS.md`） — リポジトリルールを機械的に検査します。
  監査自体もネガティブセルフテストで検証します。監査の本数やセルフテストの件数はスクリプトの現在の内容を正とします。
- **差分ハーネス** — REPL Legacy-vs-Persistent（§8.1）、数値行列オラクル（すべての型ペア × 演算子のスイープ。ratchet された allowlist があり、現在 allowlist された相違はゼロ）、差分ファジング（`DIFFERENTIAL_FUZZING.md`）。
- **Workarounds ledger**（`WORKAROUNDS.md`） — すべての ad hoc な特別扱いが issue とリンクされてカウントされています。登録されていないものを追加すると監査が失敗します。

CI について：ワークフロー定義は `.github/workflows/` 以下に存在しますが、GitHub Actions は現在このリポジトリでは実行されていません。
上記のゲートは、マージ前に **ローカル**で実行されます（フル `cargo nextest run --release`、clippy `-D warnings`、fmt、監査、該当する場合は AoT ゲート）。

---

## 11. リポジトリマップ

```
ailujsoi/
├── subset_julia_vm/            # 統合クレート
│   └── src/
│       ├── parser/             #   subset_julia_vm_parser 上の薄いアダプタ
│       ├── lowering/           #   CST → Core IR（+ MacroExpander 継ぎ目）
│       ├── ir/                 #   Core IR 型（Expr、Stmt、Block）
│       ├── compile/            #   Core IR → バイトコード（§3.3 の表を参照）
│       ├── bytecode.rs         #   _bytecode に対するコンパイラ向けファサード
│       ├── vm/                 #   スタックインタプリタ（§3.5 の表を参照）
│       ├── register_vm.rs      #   レジスタ VM バックエンド（ゲート付き）
│       ├── runtime_types/      #   共有型ファサード：インターン、メソッド
│       │                       #   テーブル、効果、橋渡し（§5.3）
│       ├── repl/               #   REPL セッション（persistent な生きた VM）
│       ├── julia/              #   Pure Julia Base/stdlib/packages（レイヤー 3）
│       ├── aot/                #   AoT パイプライン（feature "aot"）
│       ├── api.rs, ffi_support #   ホスト向け実行 API
│       └── intrinsics.rs       #   レイヤー 1 プリミティブ
├── subset_julia_vm_bytecode/   # 共有プログラム表現（ワイヤー層）
├── subset_julia_vm_types/      # 型システム + 推論コア + lattice
├── subset_julia_vm_ir/         # Span + エラー層
├── subset_julia_vm_parser/     # Lexer / Parser / CST
├── subset_julia_vm_ffi/        # C ABI（staticlib/cdylib、ヘッダーは include/）
├── subset_julia_vm_web/        # wasm-bindgen bindings
├── subset_julia_vm_runtime/    # AoT 生成コードの実行時支援
├── SubsetJuliaVMApp/           # SwiftUI iOS アプリ（samples、REPL、editor）
├── mobile/                     # Flutter アプリ
├── web/                        # ブラウザーデモ（WASM binding の利用側）
├── julia/                      # ベンダー提供の上流 Julia（参照のみ）
├── extern/                     # 参照用パッケージ clone（未追跡。MANIFEST.tsv で版を固定）
├── vendor/                     # ベンダー化した Rust 依存（例: astro-float-num）
├── benchmarks/                 # ベンチマークソースと実行スクリプト（BLOG.md の測定対象）
├── examples/                   # 実行例（mandelbrot.jl など）
├── docker/                     # 非標準環境の互換ビルド（Raspberry Pi armv7/armv6、Termux）
├── memory/                     # セッション横断の知見記録（MEMORY.md が索引）
├── docs/vm/                    # 設計文書（このディレクトリ）
└── scripts/                    # 監査スクリプト、ベンチマーク、E2E ハーネス
```

### 主要な `docs/vm/` リファレンス

| トピック | 文書 |
|----------|------|
| 型システム / lattice / プロモーション | `TYPE_SYSTEM.md`、`LATTICE_TYPE.md`、`PROMOTION.md` |
| 部分型判定（上流アルゴリズムとのギャップ表） | `SUBTYPING.md` |
| 型インターン（ディスパッチ同一性） | `TYPE_INTERNING.md` |
| ロワリング / CST | `LOWERING.md` |
| 呼び出し命令 / 二項ディスパッチ | `CALL_INSTRUCTIONS.md`、`BINARY_DISPATCH.md` |
| REPL 評価モデル | `ADR_REPL_EVAL_MODEL.md` |
| タスクスケジューラ / VM レベル継続（#10269） | `ADR_TASK_CONTINUATIONS.md` |
| ジェネレーター表現 | `GENERATOR_REPRESENTATION.md` |
| 正規表現の PCRE2 parity | `REGEX_PCRE2_PARITY.md` |
| レジスタ VM / アンボックス化設計 | `REGISTER_VM.md` |
| バックエンド戦略（AoT スコープ / 生成 Rust 所有権） | `ADR_BACKEND_STRATEGY.md`、`AOT_OWNERSHIP_CONVENTIONS.md` |
| キャッシュ / compile-context 復元 | `CACHE_ARCHITECTURE.md`、`COMPILE_CONTEXT_REHYDRATION.md` |
| 起動レイテンシ / TTFX / キャッシュ戦略 | `TTFX_AND_CACHING.md` |
| Pure Julia 設計 / ビルトイン廃止 | `PURE_JULIA_DESIGN.md`、`BUILTIN_REMOVAL.md` |
| コレクション / メモリ | `COLLECTIONS.md`、`VM_MEMORY_MANAGEMENT.md`、`MEMORY_PRIMITIVE.md` |
| 数値型 | `NUMERIC_TYPES.md` |
| Panic-free VM | `PANIC_FREE.md` |
| クレート分割 | `CRATE_SPLIT.md` |
| 指標 | `NORTH_STAR.md` |
| 根因別の品質改善 owner / differential gate / KPI | `QUALITY_PREVENTION_PLAN.md` |
| 状態 / 完了 / 未実装 | `STATUS.md`、`DONE.md`、`UNIMPLEMENTED.md` |

---

## 12. 新機能追加のチェックリスト

1. **公式実装を見つける。** `julia/base/` または `julia/stdlib/` で探す。
2. **Pure Julia で再現する。** `subset_julia_vm/src/julia/` 以下で、上流と同じパスに置く。
3. **fixture テストを追加する。** まず上流 `julia` で fixture を実行する。`bash scripts/fixture_julia_parity.sh <fixture>` で整合性をチェックする。
4. **ディスパッチをチェックする。** 新しい動的ハンドラーは共有リゾルバーを通す（§6.2）。混合型の数値メソッドは上流を反映し、promote フォールバックの再帰トラップを避ける。
5. **文書を更新する。** `STATUS.md` / `DONE.md` / `UNIMPLEMENTED.md`。
6. **ゲートを実行する。** `timeout 1800 cargo nextest run --release`、clippy、fmt。AoT に触れる変更には `bash scripts/test_aot.sh`。

完全なワークフロー（git 規約、workaround ルール、issue-first discovery ルール）は `CLAUDE.md` を、変更ごとのチェックリストは `CHECKLISTS.md` を参照してください。
