# SubsetJuliaVM: iOS で Julia のサブセットを動かすための静的 VM

**最終更新**: 2026-06-11

この記事は、SubsetJuliaVM の現在の設計と実装方針を日本語で説明する技術ブログです。
正確な仕様一覧は [SUPPORTED_FEATURES.md](SUPPORTED_FEATURES.md)、詳細な設計は
[ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) を参照してください。

---

## はじめに

Julia は数値計算、配列処理、多重ディスパッチに強い言語です。一方で、通常の Julia
実行環境は JIT コンパイルと大きなランタイムを前提にしています。iOS や WebAssembly
のように実行時コード生成を前提にしにくい環境では、この前提がそのまま使えません。

SubsetJuliaVM は、その制約を避けるために作られた静的パイプラインです。Julia の全機能を
丸ごと移植するのではなく、厳密に定義したサブセットを解析し、Core IR に下げ、VM
bytecode として実行します。

```
Julia source
    -> Parser
    -> Lowering
    -> Compiler
    -> VM
    -> Swift/iOS via C ABI
```

目標は単に「Julia っぽい言語」を作ることではありません。可能な限り本家 Julia と同じ
構文、同じ結果、同じエラー境界を保ちながら、no-JIT の実行環境に収まる形へ落とし込むことです。

## なぜ「サブセット」なのか

Julia は非常に動的な言語です。マクロ、`eval`、生成関数、メソッド追加、型推論、世界年齢、
反射 API などが密接に絡みます。これらをすべて iOS 向けの小さな VM に持ち込むと、実装も
実行時コストも大きくなりすぎます。

SubsetJuliaVM は、ここを割り切ります。

- Julia の構文はできるだけそのまま受け入れる
- 対応できるものは lowering と compiler で静的に判定する
- 対応できないものは span と hint を持つエラーにする
- 実行時 VM は、iOS で成立する小さく予測可能な実行系にする

つまり、言語表面は Julia に寄せ、実行モデルは静的 VM に寄せます。この分離がプロジェクトの
中心です。

## Parser から VM まで

### Parser

Parser は Julia ソースを Concrete Syntax Tree に変換します。現在は pure Rust の
`subset_julia_vm_parser` が中心で、Native と WASM で同じコードパスを使います。
`where` 節、マクロ、アロー関数、修飾名、juxtaposition など、Julia らしい構文を扱います。

ここで重要なのは、Parser が「実行できるか」を決めすぎないことです。Julia の構文として
読めるものをできるだけ CST にし、意味的な対応可否は lowering 以降へ渡します。

### Lowering

Lowering は CST を Core IR へ変換します。ここで構文糖衣をほどき、関数定義、型注釈、
スコープ、マクロ展開、closure capture などを整理します。

SubsetJuliaVM では、対応可否の境界を lowering に集める方針を取っています。たとえば構文は
読めても実行モデルに乗らないものは、ここで `UnsupportedFeature` として span 付きで返します。
これは iOS 側のエラー表示にも効きます。どこが問題かをユーザーに示せるからです。

### Compiler

Compiler は Core IR を VM bytecode に変換します。ここでは多重ディスパッチ、型推論、
定数伝播、union splitting、call selection などが行われます。

特に重要なのが抽象解釈ベースの型推論です。`LatticeType` を使い、変数、フィールド、分岐、
ループの型を保守的に追跡します。`x !== nothing` や `obj.field isa T` のような条件分岐は、
then/else branch で型を絞り込みます。現在は `obj.inner.value` のような nested field
refinement も代表ケースをサポートしています。

### VM

VM は stack-based bytecode interpreter です。`Value` enum で Julia 値を表現し、
`vm/exec/` 以下の per-instruction handler が実行します。

VM の設計で重視しているのは、AoT だけに寄せすぎないことです。iOS で no-JIT runtime を
成立させるには、VM 実行そのものが速く、安定していて、panic しない必要があります。そのため、
型付き配列、hot loop の predecode、typed slot、dynamic dispatch helper など、VM 側の実行性能も
継続的に改善しています。

## Pure Julia First

SubsetJuliaVM の実装方針は「Pure Julia First」です。

`subset_julia_vm/src/julia/` には、Base や stdlib の一部が Julia で実装されています。演算子、
promotion、collections、strings、iterators、broadcast、Complex、Rational など、多くの表面 API は
Rust builtin ではなく Julia のメソッドとして持ちます。

Rust builtin は、どうしても Julia だけでは書きにくい境界に絞ります。

- ファイル I/O
- OS との境界
- hash table の内部操作
- CPU レベルの数値 intrinsic
- VM のメモリ表現に直接触る primitive

この方針にすると、Base の振る舞いを本家 Julia に近づけやすくなります。また、関数が増えるたびに
Rust 側へ特殊ケースを足すのではなく、Julia の多重ディスパッチで自然に拡張できます。

## 現在サポートしている代表的な機能

2026-06-11 時点では、SubsetJuliaVM は小さな実験 VM というより、かなり広い Julia サブセットを
動かす実行系になっています。

代表的には次の範囲をサポートしています。

- `if` / `for` / `while` / `try` / `catch` / `finally`
- 通常関数、short-form 関数、lambda、do 構文、再帰
- keyword arguments、varargs、splat、戻り値型注釈
- module / using / import / export / module-qualified call
- macro 定義、quote、補間、hygiene、`@test`、`@time`、`@show`、`@kwdef`、`@enum`
- 多重ディスパッチ、parametric struct、`Type{T}` dispatch、`where`
- `Union`、`Nothing`、`Missing`、例外型、field/property access
- typed Array、Matrix、slice、view、broadcast
- Dict、Set、Tuple、NamedTuple、Range、iterator protocol
- Complex、Rational、BigInt、BigFloat の代表ケース
- 文字列、Regex、IOBuffer、Printf、path/filesystem API のサブセット
- Test、Printf、Iterators、Broadcast、Statistics、Random、Dates、InteractiveUtils、LinearAlgebra の一部
- `.sjir` Core IR、`.sjvmbc` VM bytecode、AoT Rust codegen
- C ABI と WASM/Web API

もちろん、これは「本家 Julia の全機能」を意味しません。対応範囲は fixture tests と
ドキュメントで固定し、未対応のものは [UNIMPLEMENTED.md](UNIMPLEMENTED.md) に集めています。

## 配列とコレクション

Julia らしさを保つうえで、配列と iteration は避けて通れません。

SubsetJuliaVM は 1D/2D 配列、typed array storage、linear indexing、slice、`begin`/`end`
index、broadcast、`map`/`filter`/`reduce` 系をサポートします。`SubArray` と `view` も代表的な
1D/2D/3D ケースで親配列への aliasing を保ちます。

Dict と Set も Pure Julia 実装と VM primitive の境界を分けながら、`get!`、`mergewith`、
集合演算、iteration、内包表記の代表ケースを扱います。

このあたりは、単に「関数が呼べる」だけでは不十分です。要素型が保たれるか、reshape や view が
stale な raw storage を読まないか、HOF が配列の shared-parent を壊さないか、といった細かい
互換性が重要になります。そのため `docs/vm/CODE_AUDITS.md` には、配列アクセスに関する監査ルールも
置いています。

## 型推論とディスパッチ

SubsetJuliaVM は Julia の完全な compiler ではありませんが、静的 VM として十分な実行性能を
出すには型推論が必要です。

型推論は `LatticeType` を使います。`Concrete(Int64)`、`Union{Int64,Nothing}`、`Const(v)`、
`Top`、`Bottom` などを使い、関数の戻り値や局所変数の型を求めます。

この型情報は bytecode selection に使われます。たとえば `Int64 + Int64` は typed intrinsic に
寄せられます。一方で `Any` や union が絡む場合は、runtime dispatch helper に任せます。

大事なのは、速さのために互換性を壊さないことです。型が確定できないときは無理に決め打ちせず、
保守的に runtime dispatch へ戻します。間違った高速化より、少し遅くても正しい実行を優先します。

## エラーは API の一部

SubsetJuliaVM では、エラーも実装対象です。

パースエラー、unsupported feature、runtime error は、できるだけ source span と hint を持ちます。
これは CLI だけでなく、Swift/iOS 側でエラー位置をハイライトするためにも必要です。

また、実装中に本家 Julia と違う挙動を見つけた場合は、先に bug Issue として切り出す運用にしています。
互換性の差分を「たまたま通る workaround」として埋め込むと、あとで仕様が見えなくなるからです。

## iOS と Web で使うために

iOS 連携は C ABI を通します。

- `compile_and_run`
- `compile_and_run_with_output`
- `compile_and_run_detailed`
- cancellation API
- result/free API

Web 側は `subset_julia_vm_web` が WASM API を提供します。`run_from_source`、IR 実行、
Unicode 入力支援、サポート機能の概要取得などがあります。

このように、SubsetJuliaVM は単体 CLI だけでなく、アプリに組み込む実行エンジンとして設計されています。

## AoT の位置づけ

AoT は Core IR から Rust codegen へつなぐ実験的な経路を持ちます。`.sjir` から Rust を生成し、
最適化パスとして DCE、constant folding、loop optimization、inlining などを持ちます。

ただし、現時点での主戦場は VM です。AoT は重要ですが、iOS の no-JIT runtime を成立させるには
VM の互換性、実行速度、panic-free 性が先に効きます。AoT は VM の代替というより、静的 pipeline の
もう一つの出力先として育てています。

## ドキュメントとテストの役割

SubsetJuliaVM では、テストとドキュメントを実装の一部として扱います。

fixture は `subset_julia_vm/tests/fixtures/` にカテゴリ別で置きます。新しい Julia 機能を足すときは、
まず本家 Julia で期待値を確認し、それを SubsetJuliaVM の fixture に固定します。

ドキュメントは役割を分けています。

- [SUPPORTED_FEATURES.md](SUPPORTED_FEATURES.md): 現在サポートしている公開機能
- [UNIMPLEMENTED.md](UNIMPLEMENTED.md): 未実装、制限、残課題
- [DONE.md](DONE.md): 実装済み項目の正規ログ
- [STATUS.md](STATUS.md): 日々の進捗ログ
- [WORKAROUNDS.md](WORKAROUNDS.md): Issue 付き workaround の一覧
- [CHECKLISTS.md](CHECKLISTS.md): 新機能追加時の確認項目

古くなった記事や調査メモは、削除せず `docs/vm/archived/` へ移します。歴史的経緯は残しつつ、
現行仕様と混ざらないようにするためです。

## まとめ

SubsetJuliaVM は、Julia をそのまま小さくしたものではありません。Julia の構文と実行結果への互換性を
できるだけ保ちながら、iOS や WebAssembly で動く静的 VM に再構成するプロジェクトです。

そのために、Parser、Lowering、Compiler、VM を明確に分け、Pure Julia First の方針で Base を育て、
型推論と多重ディスパッチを VM 実行へつなげています。

本家 Julia のすべてを実装する道ではありません。けれど、サブセットを正確に定義し、テストで固定し、
エラー境界を明示することで、実用的な Julia 実行環境を no-JIT の世界へ持ち込むことはできます。

現在の詳細な対応範囲は [SUPPORTED_FEATURES.md](SUPPORTED_FEATURES.md) を、設計の入口は
[ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) を参照してください。
