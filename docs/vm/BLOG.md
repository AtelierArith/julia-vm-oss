# SubsetJuliaVM: Julia の部分集合を、JIT なしで動かす

SubsetJuliaVM（sjulia）は、Julia の言語設計を保ちながら、iOS や WebAssembly、CLI 上で実行できる**静的パイプライン**である。

**静的パイプライン**とは、プログラムの実行前にソースコードを解析し、最終的な実行形式（ここでは VM 用のバイトコード）を生成しておき、実行時はそれを解釈する方式である。
実行中に新たな機械語やバイトコードを生成する必要がなく、あらかじめ決まった命令列を動かす。

公式 Julia は実行時に **LLVM** を使ってネイティブコードを生成する **JIT** コンパイラを持つ。
この設計はワークステーションやサーバーで強力だが、iOS や Web ブラウザのような「実行中の機械語生成を禁じる」環境ではそのまま動かせない。

sjulia はこの制約に対する別の答えである。
Julia ソースを前もって解析し、独自の **バイトコード** を生成し、Rust で書かれた **スタック VM** 上で実行する。
本稿は、Julia 本家ユーザーや言語処理系に関心のある読者に向けて、その設計と実行モデルを説明する。

専門用語の定義は [SubsetJuliaVM 用語集](GLOSSARY.md)にまとめてある。

## 目次

- [SubsetJuliaVM の目的](#subsetjuliavm-の目的)
- [Julia 本体との違い](#julia-本体との違い) — 静的パイプライン、JIT と AOT、アーキテクチャ
- [Stack VM とは何か](#stack-vm-とは何か)
- [ソースコードはどう実行されるか](#ソースコードはどう実行されるか)
- [一つの関数を両処理系で追う](#一つの関数を両処理系で追う) — `f(x) = x + 1` の機械語とバイトコード
- [JIT ではない実行時特殊化](#jit-ではない実行時特殊化)
- [Rust 実装で得られること](#rust-実装で得られること)
- [実行パフォーマンス](#実行パフォーマンス) — coprime π と Mandelbrot の処理系間比較
- [読み進める入口](#読み進める入口)

## SubsetJuliaVM の目的

sjulia の目的は「Julia 全機能の再実現」ではない。

目的は、関数・数値・配列・構造体・**複数ディスパッチ**・簡易的なパッケージ読み込みなど、Julia の中核的な計算表現を、モバイルや Web など JIT を置けない場所へ持ち出すことである。

この設計には二つの意図がある。
一つは実用的なもので、研究用の Julia コードをスマートフォンアプリやブラウザ上のデモ・教材として動かす。
もう一つは教育的なもので、Julia のソースがどう構文木になり、型推論でどう絞られ、どのような命令列に変わるかを、比較的小さな Rust コードベースで追えるようにする。

## Julia 本体との違い

### 静的パイプラインの意味

sjulia を「静的パイプライン」と呼ぶのは、実行時の動的なコード生成に依存しないためである。

静的パイプラインでは、Parser → Lowering → Compiler → VM バイトコードという各段階が、プログラムの実行前に完了する。
実行時に行われるのは、生成済みのバイトコードを VM が解釈することだけである。

これは Julia 本家の JIT と対照的である。
Julia 本家では、関数が初めて呼ばれるときに型推論と LLVM codegen が走り、実行中に新たな機械語が作られる。
sjulia では、実行中に LLVM を動かしたり CPU の機械語を生成したりすることはない。

この性質が、sjulia を iOS やブラウザ上の WebAssembly のような、実行中に CPU の機械語を生成できない環境で動かせる理由である。

ここでいう「機械語生成」とは、プログラム自身が実行中に CPU が直接実行できる新しいバイナリを作ることである。
iOS では、アプリが実行可能なメモリ領域を確保して新しい機械語を書き込むことが許されていない。
WebAssembly では、ブラウザ上の Wasm モジュール内から実行可能メモリを確保したり、CPU の機械語を直接生成したりすることはできない。
新しいコードを動かすには、ホスト側の JavaScript に Wasm バイナリを生成してもらい、`WebAssembly.instantiate` する形になる。
したがって、LLVM を使って実行中に機械語を作る Julia 本家の JIT は、これらの環境ではそのまま動かせない。

### JIT と AOT の違い

公式 Julia は **JIT（Just-In-Time）コンパイル** を採用している。
関数が初めて呼ばれるとき、Julia は引数の型を集め、その型に特化した高速な機械語を **LLVM** 経由で生成する。
同じ関数でも `Int64` 用、`Float64` 用、`ComplexF64` 用など、実行時に実際に現れた型に応じて異なる機械語が作られる。

sjulia の標準経路は **AOT（Ahead-Of-Time）なバイトコードコンパイル** である。
ソースコードを実行前に解析し、VM が解釈するバイトコード命令列を生成する。

実行時に追加の特殊化が行われる場合もある。
たとえば `CallSpecialize` は、型注釈のない引数を持つ関数に対し、実行時に現れた具体的な `ValueType` の組に合わせたバイトコードを生成する。
ただし、これは CPU の機械語を作る Julia 本家の JIT とは異なり、生成されるのは同じ VM が読むバイトコードだけである。

この違いは「速いか遅いか」だけではない。
公式 Julia は大規模な JIT システムとして総合的な計算環境を作る。
sjulia はその一部を犠牲にして、JIT を置けない場所へ Julia 風の計算を運ぶ。

### アーキテクチャの違い

公式 Julia の実体は Julia 自身で書かれたランタイム、型推論器、LLVM バックエンドの連携である。
対照的に sjulia は Rust の複数クレートに分かれ、構文解析、型表現、バイトコード、VM、C ABI、WASM API を分担する。

```text
Julia source
    │
    ▼
┌─────────┐   ┌──────────┐   ┌──────────────┐   ┌────┐
│  Parser │ → │ Lowering │ → │   Compiler   │ → │ VM │
└─────────┘   └──────────┘   └──────────────┘   └────┘
                                                   │
                         Swift/iOS via C ABI ◄─────┘
                         WASM binding / CLI
```

実際の Rust API はおおむね次の形に近い。

```rust
let program = parse_and_lower(src)?;
let compiled = compile_with_cache(&program)?;
let mut vm = Vm::new_program(compiled, StableRng::new(seed));
let value = vm.run()?;
```

この流れの中で、文字列は **CST** になり、**lowering** によって **Core IR** になり、コンパイラによって **CompiledProgram**（バイトコード列）になり、最後に VM が値を動かす。

同じ Julia ソースを入力しても、Julia 本家と sjulia では lowering 後の処理と実行主体が異なる。

| 段階 | Julia 本家 | sjulia |
|---|---|---|
| 構文解析 | Julia 構文を解析して AST を作る | Rust 製パーサーが CST を作る |
| Lowering | AST を lowered `CodeInfo` へ変換する | CST を独自の Core IR `Program` へ変換する |
| 型の決定 | 呼び出し時の引数型を使って型推論し、メソッドを特殊化する | 静的に得られる型を推論し、必要なら実行時の値型に応じて VM バイトコードを特殊化する |
| コード生成 | LLVM IR を作り、LLVM が CPU の機械語へ変換する | `CompiledProgram` に Stack VM 用の命令列を格納する |
| 実行 | CPU が型ごとに生成された機械語を実行する | Rust 製 VM が命令を順に読み、値スタックと呼び出しフレームを更新する |
| 生成物の単位 | CPU アーキテクチャに依存するネイティブコード | iOS、WASM、CLI で共通に解釈できる VM バイトコード |

Julia 本家の最終生成物は CPU が直接実行する機械語なので、JIT コンパイル後の計算を高速に実行できる。
sjulia の最終生成物は CPU 非依存の VM バイトコードなので、同じ Rust 製 VM を各プラットフォーム向けに事前ビルドすれば、実行中に機械語を生成せずに Julia ソースを処理できる。

## Stack VM とは何か

VM（Virtual Machine）は、実際の CPU の代わりにソフトウェアで動く「仮想の計算機」である。
ソースコードを直接動かすのではなく、あらかじめ変換した命令列である**バイトコード**を一つずつ読んで実行する。

**Stack VM** は、その中でも値を積み重ねる**スタック**というデータ構造を中心に計算を進める VM である。
スタックとは、皿を重ねたような置き場である。
新しい値を入れると一番上に積まれ、取り出すときも一番上から取る。

命令は、必要な値をスタックの上から取り出し、計算結果をまたスタックの上に戻す。
たとえば `1 + 2` は、次のようなバイトコードになる。

```text
PushI64(1)   # スタックに 1 を積む → [1]
PushI64(2)   # スタックに 2 を積む → [1, 2]
AddI64       # 上から 2 つを取り出して足し、結果を積む → [3]
```

実行後、スタックの一番上には `3` が残る。
命令は「スタックに積む」「スタックから取る」だけで済むため、コンパイラは比較的単純な命令列を作れる。
これが Stack VM の利点である。

欠点は、値を取り出すたびにスタック操作のオーバーヘッドがかかることである。
実際の CPU は名前のついた高速な**レジスタ**を使うため、同じ計算でも Stack VM は機械語より遅くなることが多い。

sjulia では、コンパイラが作るバイトコードがこの Stack VM 向けである。
`f(x) = x + 1` の汎用バイトコードは、最終的に次のような流れになる。

```text
LoadSlot(0)            # 引数 x をスタックに積む
PushI64(1)             # 定数 1 をスタックに積む
CallDynamicBinaryBoth  # スタック上の 2 値を取り出して + を実行し、結果を積む
ReturnAny              # スタックの上の値を呼び出し元へ返す
```

型が分かる場合は、より具体的な命令が使われる。
`f(1)` なら整数加算の融合命令が、`f(3.0)` なら浮動小数点加算命令が選ばれる。
型が分からない場合は、汎用のスタック操作命令に戻る。

sjulia が Stack VM を標準に選んだのは、実装を小さく保ちやすく、iOS や WebAssembly のように実行中の機械語生成が制限された環境でも動かせるためである。
同じ Julia ソースから、後で述べる AoT バックエンドやレジスタ VM 実験経路へ発展させることもできるが、基本的な実行系は Stack VM である。

## ソースコードはどう実行されるか

sjulia は Julia ソースを最終的に VM のバイトコードへ変換する。

```text
Julia source
    ↓
CST
    ↓
Core IR の Program
    ↓
CompiledProgram
    ↓
VM が扱う Value
```

最初の段階では、Rust 製の **lexer** と **パーサー** が文字列を読む。
`f(x) = x + 1` という1行は、名前 `f`、引数 `x`、本体 `x + 1` を持つ関数定義として認識される。

次の **lowering** 段階では、表記上の形をコンパイラが扱いやすい Core IR へ変換する。
短い関数定義も `function ... end` 形式も、最終的には同じような関数定義ノードとして Program に入る。

コンパイラは Program を読み、関数表、型情報、メソッド表、命令列を作る。
`f(x) = x + 1` は型注釈のない関数なので、`x` が整数か実数か複素数かは、定義を読んだだけでは決まらない。

VM は命令列を実行し、実際の値をスタックや **フレーム** に置いて計算する。
呼び出し時に型が分かれば、VM はその型に合わせた **実行時バイトコード特殊化** を使う場合がある。

## 一つの関数を両処理系で追う

ここからは 1 行の関数定義 `f(x) = x + 1` を素材に、Julia 本体と sjulia が同じソースをどう処理するかを具体的に追う。

### Julia 本体: 型ごとに機械語を生成する

Julia 本体は、まず `f(x) = x + 1` を型に依存しない形へ **lower** する。
`@code_lowered f(1)` で見ると、`+` はまだ動的な呼び出しである。

```julia
CodeInfo(
1 ─ %1 = Main.:+
│   %2 =   dynamic (%1)(x, 1)
└──      return %2
)
```

型推論の後、Julia 本体は入力の型ごとに異なる typed `CodeInfo` を作る。
`f(1)` では整数加算になる。

```julia
CodeInfo(
1 ─ %1 = intrinsic Base.add_int(x, 1)::Int64
└──      return %1
) => Int64
```

`f(3.0)` では、`1` が `1.0` として扱われ、浮動小数点加算になる。

```julia
CodeInfo(
1 ─ %1 = intrinsic Base.add_float(x, 1.0)::Float64
└──      return %1
) => Float64
```

`f(im)` では、Julia は複素数の実部と虚部を取り出し、実部へ `1` を足して新しい `Complex{Int64}` を作る。

```julia
CodeInfo(
1 ─ %1 =   builtin Base.getfield(x, :re)::Bool
│   %2 = intrinsic Core.zext_int(Core.Int64, %1)::Int64
│   %3 = intrinsic Core.and_int(%2, 1)::Int64
│   %4 = intrinsic Base.add_int(%3, 1)::Int64
│   %5 =   builtin Base.getfield(x, :im)::Bool
│   %6 = intrinsic Core.zext_int(Core.Int64, %5)::Int64
│   %7 = intrinsic Core.and_int(%6, 1)::Int64
│   %8 = %new(Complex{Int64}, %4, %7)::Complex{Int64}
└──      return %8
) => Complex{Int64}
```

`f(2.0 + 3im)` では、実部だけが `add_float` で更新され、虚部はそのまま新しい `ComplexF64` に入る。

```julia
CodeInfo(
1 ─ %1 =   builtin Base.getfield(x, :re)::Float64
│   %2 = intrinsic Base.add_float(1.0, %1)::Float64
│   %3 =   builtin Base.getfield(x, :im)::Float64
│   %4 = %new(ComplexF64, %2, %3)::ComplexF64
└──      return %4
) => ComplexF64
```

Julia 本体は、この typed `CodeInfo` をもとに LLVM を通して機械語を生成する。
ここが JIT と呼ばれる部分である。

### sjulia: 汎用バイトコードと呼び出し地点の特殊化

sjulia で同じ定義を `--dump-bytecode` すると、型注釈のない `f` は次のような汎用バイトコードになる。

```text
[5432] f(x::Any) -> Any
  slots:
    #0   x                            :: unknown param
  bytecode:
     98594: LoadSlot(0) ; slot #0 x::unknown
     98595: PushI64(1)
     98596: CallDynamicBinaryBoth(DynamicAdd, [...])
     98597: ReturnAny
```

この4命令は、関数の意味をかなり直接に表している。
`LoadSlot(0)` は引数 `x` を読む。
`PushI64(1)` は整数リテラル `1` をスタックに積む。
`CallDynamicBinaryBoth` は、左右の実行時型を見て `+` の候補から実際の計算を選ぶ。
`ReturnAny` は結果を返す。

なお、`CallDynamicBinaryBoth(DynamicAdd, [...])` の `DynamicAdd` は、動的二項演算 `+` の内部識別子である。
左右の値を見て `Int64`、`Float64`、複素数、BigInt などに対応する `+` の実装へ分岐するためのタグであり、浮動小数点専用ではない。
以前は同じ wire name（`add_float`）に由来する `AddFloat` という表示だったが、浮動小数点専用に見えて紛らわしいため、現在の dump では `DynamicAdd` と表示される。

この汎用バイトコードは、`f(x) = x + 1` という定義が読まれた時点で生成される。
`x` に型注釈がないため、定義時には `x` が整数か実数か複素数かは決まらず、どの型が来ても動く命令列が作られる。

入力型が呼び出し地点で見える場合、sjulia は main バイトコード側でより具体的な命令を出すことがある。
`f(1)` は `LoadAddConstI64Slot` という融合命令になる。

```text
98843: PushI64(1)
98844: StoreSlotI64(296) ; __sjulia_inline_arg_f_0::I64
98845: LoadAddConstI64Slot(296, 1)
98846: Dup
98847: CallBuiltin(IOPrint, 1)
```

`f(3.0)` では、`3.0` を `Float64` の **スロット** に入れ、`1` を `Float64` へ変換してから `AddF64` を実行する。

```text
98852: PushF64(3.0)
98853: StoreSlotF64(297) ; __sjulia_inline_arg_f_1::F64
98854: LoadSlotF64(297)
98855: PushI64(1)
98856: ToF64
98857: AddF64
```

`f(im)` では、`im` が struct として読まれ、`+` のメソッドへ通常呼び出しが行われる。
この場合、複素数の処理は Base 側の `+` メソッドに入る。

```text
98864: LoadSlotStruct(91) ; slot #91 im::Struct
98865: StoreSlotStruct(298) ; __sjulia_inline_arg_f_2::Struct
98866: LoadSlotStruct(298)
98867: PushI64(1)
98868: Call(1454, 2) ; call #1454 + argc=2
```

`f(2.0 + 3im)` では、先に `2.0 + 3im` そのものを作り、その結果に対して `+ 1` を呼ぶ。

```text
98875: PushF64(2.0)
98876: PushI64(3)
98877: LoadSlotStruct(91) ; slot #91 im::Struct
98878: Call(1457, 2) ; call #1457 * argc=2
98879: Call(1453, 2) ; call #1453 + argc=2
98880: StoreSlotStruct(299) ; __sjulia_inline_arg_f_3::Struct
98881: LoadSlotStruct(299)
98882: PushI64(1)
98883: Call(1454, 2) ; call #1454 + argc=2
```

### 変数経由の呼び出しと型が隠れる場合

変数に入った値から `f` を呼び出す場合も、呼び出し地点で型が分かれば同様に特化される。

```bash
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e 'f(x) = x + 1; x = 1.0; f(x); y = 3; f(y)'
```

`f` 自体の定義は先ほどと同じ汎用バイトコードのままである。

```text
[5432] f(x::Any) -> Any
  slots:
    #0   x                            :: unknown param
  bytecode:
     98594: LoadSlot(0) ; slot #0 x::unknown
     98595: PushI64(1)
     98596: CallDynamicBinaryBoth(DynamicAdd, [...])
     98597: ReturnAny
```

一方、`main` バイトコードの実行系列では、それぞれの呼び出しが異なる命令に展開されている。

```text
103379: DefineEvalFunction(5432)          ; f を定義
103380: PushF64(1.0)
103381: StoreSlotF64(298) ; slot #298 x::F64
103382: LoadSlotF64(298) ; slot #298 x::F64
103383: StoreSlotF64(299) ; slot #299 __sjulia_inline_arg_f_0::F64
103384: LoadSlotF64(299) ; slot #299 __sjulia_inline_arg_f_0::F64
103385: PushI64(1)
103386: ToF64
103387: AddF64
103388: StoreSlotF64(300) ; slot #300 #discard#1::F64
103389: PushI64(3)
103390: StoreSlotI64(301) ; slot #301 y::I64
103391: LoadSlotI64(301) ; slot #301 y::I64
103392: StoreSlotI64(302) ; slot #302 __sjulia_inline_arg_f_1::I64
103393: LoadAddConstI64Slot(302, 1)
103394: ReturnI64
```

`x = 1.0` としたあとの `f(x)` は、`Float64` のスロットに入った値を読み出し、`1` を `ToF64` で浮動小数点に変換してから `AddF64` を実行する。
`y = 3` としたあとの `f(y)` は、`Int64` の値に対して `LoadAddConstI64Slot` という融合命令が使われる。
つまり、関数 `f` 自体は汎用の動的ディスパッチメソッドとして定義されたままだが、呼び出し側では引数の型が分かっているため、それぞれの型に特化した専用バイトコードにインライン展開・最適化される。

複素数を変数に入れて呼び出す場合も、呼び出し地点で型が struct であることが分かっていれば、同様に struct 用の経路が選ばれる。

```bash
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e 'f(x) = x + 1; w = im; f(w)'
```

```text
103379: DefineEvalFunction(5432)          ; f を定義
103380: LoadSlotStruct(91) ; slot #91 im::Struct
103381: StoreSlotStruct(298) ; slot #298 w::Struct
103382: LoadSlotStruct(298) ; slot #298 w::Struct
103383: StoreSlotStruct(299) ; slot #299 __sjulia_inline_arg_f_0::Struct
103384: LoadSlotStruct(299) ; slot #299 __sjulia_inline_arg_f_0::Struct
103385: PushI64(1)
103386: Call(1454, 2) ; call #1454 + argc=2
103387: ReturnStruct
```

`w = im` としたあとの `f(w)` は、`im` を struct スロットに読み込んでから `+` メソッドを呼び出す。
これは直接 `f(im)` と書いた場合と本質的に同じ命令列である。

```bash
cargo run -p subset_julia_vm --bin sjulia --features repl -- --dump-bytecode -e 'f(x) = x + 1; z = 2.0 + 3im; f(z)'
```

```text
103379: DefineEvalFunction(5432)          ; f を定義
103380: PushF64(2.0)
103381: PushI64(3)
103382: LoadSlotStruct(91) ; slot #91 im::Struct
103383: Call(1457, 2) ; call #1457 * argc=2
103384: Call(1453, 2) ; call #1453 + argc=2
103385: StoreSlotStruct(298) ; slot #298 z::Struct
103386: LoadSlotStruct(298) ; slot #298 z::Struct
103387: StoreSlotStruct(299) ; slot #299 __sjulia_inline_arg_f_0::Struct
103388: LoadSlotStruct(299) ; slot #299 __sjulia_inline_arg_f_0::Struct
103389: PushI64(1)
103390: Call(1454, 2) ; call #1454 + argc=2
103391: ReturnStruct
```

`z = 2.0 + 3im` としたあとの `f(z)` では、まず `2.0 + 3im` を作るための複素数演算が `main` 側に展開され、その結果を `z` の struct スロットに保存してから `+ 1` のメソッド呼び出しを行う。

この出力から、sjulia の標準経路が機械語ではなく VM 命令で動いていることが見える。
整数と浮動小数点数は型付き命令へ落ち、複素数は struct とメソッド呼び出しで処理される。

型が隠れる書き方では、sjulia はより動的な命令に戻る。
たとえば値を `Any` 配列に入れてループで取り出すと、ループ本体では `x` の具体型が一つに決まらない。

```julia
f(x) = x + 1
a = Any[1, 3.0, im, 2.0 + 3im]
for x in a
    println(f(x))
end
```

この場合、ループ本体には `CallDynamicBinaryBoth` が残る。

```text
98869: LoadSlot(298) ; x::unknown
98870: StoreSlot(300) ; __sjulia_inline_arg_f_0::unknown
98871: LoadSlot(300)
98872: PushI64(1)
98873: CallDynamicBinaryBoth(DynamicAdd, [...]) ; DynamicAdd は動的 `+` のタグ
98874: Dup
98875: CallBuiltin(IOPrint, 1)
```

### 4 つの入力の結果

```julia
f(x) = x + 1

println(f(1))
println(f(3.0))
println(f(im))
println(f(2.0 + 3im))
```

Julia 本体と SubsetJuliaVM は、この例では同じ結果を出す。

```text
2
4.0
1 + 1im
3.0 + 3.0im
```

`f(1)` は整数の `1` に整数の `1` を足すので `2` になる。
`f(3.0)` は浮動小数点数として計算されるので `4.0` になる。
`f(im)` は実部へ `1` が足されるので `1 + 1im` になる。
`f(2.0 + 3im)` は実部だけが増えるので `3.0 + 3.0im` になる。

この例の要点は、同じ `x + 1` でも、入力型が見える場所では具体的な命令へ落ち、型が見えない場所では動的ディスパッチが残ることである。
Julia 本体はその先で機械語を作り、sjulia は VM バイトコードとして実行する。

### なぜ 4 つの呼び出しでバイトコードが異なるのか

4 つの呼び出しで異なるバイトコードが生成されるのは、単に `f` が 4 種類あるからではない。
ソース上の定義は 1 つだけであり、`f(x) = x + 1` の関数本体そのものは、上で見たように `x::Any` を受け取る汎用バイトコードとして持たれる。
違いが出るのは、その `f` を呼ぶ側である。
呼び出し地点に `1`、`3.0`、`im`、`2.0 + 3im` という具体的な式が書かれているため、main 側のコンパイルでは「この呼び出しでは `x` に何が入るか」をかなり早い段階で知ることができる。

その情報が見える場合、sjulia は毎回 `f` の汎用本体へ素朴に飛ぶだけではなく、呼び出し地点に合わせて `x + 1` をより具体的な VM 命令列へ落とす。
これは CPU の機械語を生成する Julia 本体の JIT とは違うが、Julia の型システムと複数ディスパッチを VM バイトコードの段階で反映する、という意味では同じ問題を扱っている。
どの `+` を呼ぶべきかは、左辺と右辺の型で決まる。
したがって、同じ `x + 1` でも、`x` が `Int64` か、`Float64` か、複素数かによって、VM が選べる命令は変わる。

`f(1)` では、引数もリテラル `1` も `Int64` である。
この場合は boxed な `Value` を作って汎用の `+` を呼ぶ必要がなく、スロットから `Int64` を読み、定数 `1` を足すだけでよい。
そのため `LoadAddConstI64Slot` という融合命令に畳まれる。
これは「スロットを読む」「定数を足す」という 2 つの操作を 1 命令にまとめた形で、VM の dispatch 回数も一時値も減らせる。

`f(3.0)` では、引数が `Float64` である。
右辺の `1` はソース上は整数リテラルだが、`Float64 + Int64` の計算では浮動小数点側に合わせて計算する必要がある。
そのため、バイトコードでは `PushI64(1)` のあとに `ToF64` が入り、最後に `AddF64` が実行される。
ここではまだ VM 命令列だが、命令の時点で「この加算は浮動小数点加算でよい」と決まっている。

`f(im)` では事情が変わる。
`im` は単なる `Float64` ではなく、複素数を表す struct 値である。
複素数に整数を足す処理は、整数や浮動小数点数のような単一のプリミティブ加算命令では表しにくい。
そこで sjulia は struct を読み出し、Base 側に定義された `+` メソッドを通常の関数呼び出しとして呼ぶ。
結果として、実部に `1` が足され、虚部はそのまま残る。

`f(2.0 + 3im)` では、さらに前段の式 `2.0 + 3im` がある。
main 側のバイトコードは、まず `3 * im` と `2.0 + ...` を評価して `ComplexF64` の値を作る。
その後で、その複素数を `f` の引数として扱い、`+ 1` のメソッド呼び出しを行う。
つまりこの場合の差分は、`f` の中の `x + 1` だけでなく、引数そのものを作るための複素数演算もバイトコードに現れる点にある。

このように、sjulia のバイトコードは「関数定義の汎用形」と「呼び出し地点で型が見えている形」の両方を持つ。
型が見える場所ほど、VM は `LoadAddConstI64Slot` や `AddF64` のような具体的で軽い命令を選べる。
型が見えない場所では、`CallDynamicBinaryBoth(DynamicAdd, ...)` のように、実行時の値を見てから `+` の実装を選ぶ命令が残る。
この差が、4 つの呼び出しのバイトコードの違いである。

### 依存する関数を介した型情報の伝播 — `g(x) = 2x - 1; f(x) = g(x) + 1`

ここまでの例は `f` が直接 `x + 1` を計算する単純な形だった。
次に、`f` が別の未型注釈関数 `g` を呼ぶ場合を追う。

```julia
g(x) = 2x - 1
f(x) = g(x) + 1

x = 3
f(x)
y = 3.0
f(y)
z = im
f(z)
w = 2.0 + im
f(w)
```

このとき、`f` の仮引数 `x` の型情報は `g` に伝わるのだろうか。
端的に言えば、**直接の関数呼び出しとしては伝わらない**。
`g` も `f` も `x::Any` を受け取る汎用メソッドとしてコンパイルされる。
しかし、`g` が `f` に **インライン展開** されるなら、`f` の呼び出し地点で行われる **呼び出し地点特殊化** に `g` の中身も含まれる。

Julia 本体の型付き中間表現を見ると、`f` の特殊化メソッドの中に `g` の計算がそのまま展開されている。

```julia
# f(3)
CodeInfo(
1 ─ %1 = intrinsic Base.mul_int(2, x)::Int64
│   %2 = intrinsic Base.sub_int(%1, 1)::Int64
│   %3 = intrinsic Base.add_int(%2, 1)::Int64
└──      return %3
) => Int64
```

```julia
# f(3.0)
CodeInfo(
1 ─ %1 = intrinsic Base.mul_float(2.0, x)::Float64
│   %2 = intrinsic Base.sub_float(%1, 1.0)::Float64
│   %3 = intrinsic Base.add_float(%2, 1.0)::Float64
└──      return %3
) => Float64
```

```julia
# f(im)
CodeInfo(
1 ─ %1  =   builtin Base.getfield(x, :re)::Bool
│   %2  = intrinsic Core.zext_int(Core.Int64, %1)::Int64
│   %3  = intrinsic Core.and_int(%2, 1)::Int64
│   %4  = intrinsic Base.mul_int(2, %3)::Int64
│   %5  =   builtin Base.getfield(x, :im)::Bool
│   %6  = intrinsic Core.zext_int(Core.Int64, %5)::Int64
│   %7  = intrinsic Core.and_int(%6, 1)::Int64
│   %8  = intrinsic Base.mul_int(2, %7)::Int64
│   %9  = intrinsic Base.sub_int(%4, 1)::Int64
│   %10 = intrinsic Base.add_int(1, %9)::Int64
│   %11 = %new(Complex{Int64}, %10, %8)::Complex{Int64}
└──       return %11
) => Complex{Int64}
```

```julia
# f(2.0 + im)
CodeInfo(
1 ─ %1 =   builtin Base.getfield(x, :re)::Float64
│   %2 = intrinsic Base.mul_float(2.0, %1)::Float64
│   %3 =   builtin Base.getfield(x, :im)::Float64
│   %4 = intrinsic Base.mul_float(2.0, %3)::Float64
│   %5 = intrinsic Base.sub_float(%2, 1.0)::Float64
│   %6 = intrinsic Base.add_float(1.0, %5)::Float64
│   %7 = %new(ComplexF64, %6, %4)::ComplexF64
└──      return %7
) => ComplexF64
```

`f(3)` では整数乗算、減算、加算が連なる。
`f(3.0)` では浮動小数点版の同じ列になる。
`f(im)` と `f(2.0 + im)` では複素数の実部と虚部が個別に処理される。
`g` は個別の関数呼び出しとして残っていない。

sjulia のバイトコードでも、`f` の汎用本体に `g` の中身が展開されている。

```text
[5432] g(x::Any) -> Any code=98531..98537 entry=98531
  slots:
    #0   x                            :: unknown param
  bytecode:
     98531: PushI64(2)
     98532: LoadSlot(0) ; slot #0 x::unknown
     98533: CallDynamicBinaryBoth(DynamicMul, [...])
     98534: PushI64(1)
     98535: CallDynamicBinaryBoth(DynamicSub, [...])
     98536: ReturnAny

[5433] f(x::Any) -> Any code=98537..98547 entry=98537
  slots:
    #0   x                            :: unknown param
    #1   __sjulia_inline_arg_g_0      :: unknown
  bytecode:
     98537: LoadSlot(0) ; slot #0 x::unknown
     98538: StoreSlot(1) ; slot #1 __sjulia_inline_arg_g_0::unknown
     98539: PushI64(2)
     98540: LoadSlot(1) ; slot #1 __sjulia_inline_arg_g_0::unknown
     98541: CallDynamicBinaryBoth(DynamicMul, [...])
     98542: PushI64(1)
     98543: CallDynamicBinaryBoth(DynamicSub, [...])
     98544: PushI64(1)
     98545: CallDynamicBinaryBoth(DynamicAdd, [...])
     98546: ReturnAny
```

`g` 自身も汎用メソッドとして存在する。
しかし `f` の本体は `g` を呼んでいない。
`g(x)` の計算が `f` の中に直接展開されているため、`f` が特殊化されるときに `2x - 1` の部分も一緒に特殊化される。

`main` 側では、それぞれの呼び出しが型に応じた命令に分かれている。

```text
103328: DefineEvalFunction(5432)
103329: DefineEvalFunction(5433)
103330: PushI64(3)
103331: StoreSlotI64(298) ; slot #298 x::I64
103332: CallSpecializeI64Slots(CallSpecializeSlots { spec_func_index: 1883, slots: [298] }) ; specialize #1883 f slots=[slot #298 x::I64]
103333: StoreSlotI64(299) ; slot #299 #discard#1::I64
103334: PushF64(3.0)
103335: StoreSlotF64(300) ; slot #300 y::F64
103336: CallSpecializeF64Slots(CallSpecializeSlots { spec_func_index: 1883, slots: [300] }) ; specialize #1883 f slots=[slot #300 y::F64]
103337: StoreSlotF64(301) ; slot #301 #discard#2::F64
103338: LoadSlotStruct(91) ; slot #91 im::Struct
103339: StoreSlotStruct(302) ; slot #302 z::Struct
103340: LoadSlotStruct(302) ; slot #302 z::Struct
103341: CallSpecialize(1883, 1) ; specialize #1883 f argc=1
103342: Pop
103343: PushF64(2.0)
103344: LoadSlotStruct(91) ; slot #91 im::Struct
103345: Call(1453, 2) ; call #1453 + argc=2
103346: StoreSlotStruct(303) ; slot #303 w::Struct
103347: LoadSlotStruct(303) ; slot #303 w::Struct
103348: CallSpecialize(1883, 1) ; specialize #1883 f argc=1
103349: ReturnStruct
```

`f(x)`、`f(y)` では `CallSpecializeI64Slots` / `CallSpecializeF64Slots` が使われ、整数・浮動小数点専用の特殊化本体が呼ばれる。
`f(z)`、`f(w)` では `CallSpecialize` が使われ、複素数用の特殊化本体が呼ばれる。
`g` 単体の呼び出しはどこにもない。

この例の実行結果は次の通りである。

```text
6
6.0
0 + 2im
4.0 + 2.0im
```

`f(3)` は `2*3 - 1 + 1 = 6` となる。
`f(3.0)` は浮動小数点版で `6.0` となる。
`f(im)` は `2*im - 1 + 1 = 0 + 2im` となる。
`f(2.0 + im)` は実部が `2*(2.0) - 1 + 1 = 4.0`、虚部が `2*1 = 2` となり `4.0 + 2.0im` となる。

### 抽象型アノテーションを持つ callee の場合

次に、`g` に抽象型 `Real` のアノテーションをつけた場合を見る。

```julia
g(x::Real) = 2x - 1
f(x) = g(x) + 1

x = 3
f(x)
y = 3.0
f(y)
z = im
f(z)
w = 2.0 + im
f(w)
```

`g` は `Real` の部分型を受け入れるメソッドとしてコンパイルされる。
`Real` は抽象型なので、`g` の汎用バイトコードは具体型が決まらず、未型注釈の場合と同じく動的な二項演算命令になる。

```text
[5432] g(x::F64) -> Any code=98531..98537 entry=98531
  slots:
    #0   x                            :: unknown param
  bytecode:
     98531: PushI64(2)
     98532: LoadSlot(0) ; slot #0 x::unknown
     98533: CallDynamicBinaryBoth(DynamicMul, [...])
     98534: PushI64(1)
     98535: CallDynamicBinaryBoth(DynamicSub, [...])
     98536: ReturnAny
```

dump 上は `x::F64` と表示されるが、実際には `Real` の部分型である `Int64` も `Float64` も受け入れる。

`f` の汎用バイトコードでは、`g` への呼び出しが `CallDynamic` として残る。

```text
[5433] f(x::Any) -> Any code=98537..98542 entry=98537
  slots:
    #0   x                            :: unknown param
  bytecode:
     98537: LoadSlot(0) ; slot #0 x::unknown
     98538: CallDynamic(18446744073709551615, 1, [Method(5432)])
     98539: PushI64(1)
     98540: CallDynamicBinaryBoth(DynamicAdd, [...])
     98541: ReturnAny
```

型注釈がつくと、コンパイラは `g` を独立したメソッドとして扱い、`f` の中に展開しなくなる。
`main` 側では `f` 自体は `CallSpecializeI64Slots` / `CallSpecializeF64Slots` / `CallSpecialize` で特殊化されるが、`f` の内部で `g` を呼ぶ部分は、実行時に値の型を見て処理を選ぶ呼び出しのままである。

```text
103327: CallSpecializeI64Slots(CallSpecializeSlots { spec_func_index: 1883, slots: [298] }) ; specialize #1883 f slots=[slot #298 x::I64]
103331: CallSpecializeF64Slots(CallSpecializeSlots { spec_func_index: 1883, slots: [300] }) ; specialize #1883 f slots=[slot #300 y::F64]
103336: CallSpecialize(1883, 1) ; specialize #1883 f argc=1
103343: CallSpecialize(1883, 1) ; specialize #1883 f argc=1
```

Julia 本体では、`f(3)` では `g` が展開されて型付き命令になる。

```julia
# @code_typed f(3)
CodeInfo(
1 ─ %1 = intrinsic Base.mul_int(2, x)::Int64
│   %2 = intrinsic Base.sub_int(%1, 1)::Int64
│   %3 = intrinsic Base.add_int(%2, 1)::Int64
└──      return %3
) => Int64
```

一方、`g(x::Real)` は複素数を受け入れないため、`f(im)` や `f(2.0 + im)` では `MethodError` になる。

```julia
# @code_typed f(im)
CodeInfo(
1 ─ builtin Core.throw_methoderror(Main.g, x)::Union{}
└──     unreachable
) => Union{}
```

sjulia でも同様に `f(z)` / `f(w)` で `MethodError` になる。

```text
6
6.0
Runtime error: MethodError: no matching runtime method candidate at line 1:29
```

### 複数の具体型メソッドを持つ callee の場合

最後に、`g` に `Int64` / `Float64` / `ComplexF64` 用の複数メソッドを定義した場合を見る。

```julia
g(x::Int64) = 2x - 1
g(x::Float64) = 2x - 1
g(x::ComplexF64) = 2x - 1

f(x) = g(x) + 1

x = 3
f(x)
y = 3.0
f(y)
z = 2.0 + im
f(z)
```

この場合、`g` の各メソッドは型ごとに個別にコンパイルされる。

```text
[5432] g(x::I64) -> I64 code=98531..98536 entry=98531
  slots:
    #0   x                            :: I64 param
  bytecode:
     98531: PushI64(2)
     98532: LoadMulI64Slot(0) ; slot #0 x::I64
     98533: PushI64(1)
     98534: SubI64
     98535: ReturnI64

[5433] g(x::F64) -> F64 code=98536..98543 entry=98536
  slots:
    #0   x                            :: F64 param
  bytecode:
     98536: PushI64(2)
     98537: ToF64
     98538: LoadMulF64Slot(0) ; slot #0 x::F64
     98539: PushI64(1)
     98540: ToF64
     98541: SubF64
     98542: ReturnF64

[5434] g(x::Struct(105)) -> ComplexF64 code=98543..98557 entry=98543
  slots:
    #0   x                            :: Struct param
    #1   __sjulia_cx_re_x             :: F64
    #2   __sjulia_cx_im_x             :: F64
  bytecode:
     98543: LoadSlotStruct(0) ; slot #0 x::Struct
     98544: GetField(0)
     98545: StoreSlotF64(1) ; slot #1 __sjulia_cx_re_x::F64
     98546: LoadSlotStruct(0) ; slot #0 x::Struct
     98547: GetField(1)
     98548: StoreSlotF64(2) ; slot #2 __sjulia_cx_im_x::F64
     98549: PushF64(2.0)
     98550: LoadMulF64Slot(1) ; slot #1 __sjulia_cx_re_x::F64
     98551: PushF64(1.0)
     98552: SubF64
     98553: PushF64(2.0)
     98554: LoadMulF64Slot(2) ; slot #2 __sjulia_cx_im_x::F64
     98555: NewStruct(105, 2)
     98556: ReturnStruct
```

`f` の汎用バイトコードでは、`g(x)` が `CallTypedDispatch` になる。

```text
[5435] f(x::Any) -> Any code=98557..98562 entry=98557
  slots:
    #0   x                            :: unknown param
  bytecode:
     98557: LoadSlot(0) ; slot #0 x::unknown
     98558: CallTypedDispatch("g", 1, 5432, [5432, 5433, 5434])
     98559: PushI64(1)
     98560: CallDynamicBinaryBoth(DynamicAdd, [...])
     98561: ReturnAny
```

`CallTypedDispatch("g", 1, 5432, [5432, 5433, 5434])` は、候補メソッドの中から実行時の型に応じて一つを選ぶ命令である。
`f` の仮引数型は、どの `g` メソッドを使うかを選ぶ処理を通じて `g` には渡らない。`g` は実際に渡された値の型を見て、使うメソッドを選択する。

`main` 側では `f` の呼び出しが `CallSpecializeI64Slots` / `CallSpecializeF64Slots` / `CallSpecialize` に分かれる。

```text
103349: CallSpecializeI64Slots(CallSpecializeSlots { spec_func_index: 1885, slots: [298] }) ; specialize #1885 f slots=[slot #298 x::I64]
103353: CallSpecializeF64Slots(CallSpecializeSlots { spec_func_index: 1885, slots: [300] }) ; specialize #1885 f slots=[slot #300 y::F64]
103360: CallSpecialize(1885, 1) ; specialize #1885 f argc=1
```

実行結果は次の通り。

```text
6
6.0
4.0 + 2.0im
```

### 型変数を使ったパラメトリックメソッドの場合

最後に、`f` の引数を型変数 `T` で縛った場合を見る。

```julia
g(x) = 2x - 1
function f(x::T) where T <: Number
    g(x) + 1
end

f(3)
f(3.0)
f(2.0 + im)
```

`f` は `Number` の部分型を受け入れるパラメトリックメソッドとしてコンパイルされる。
しかし、sjulia では `T` を「メソッドを選ぶための制約」として使うだけで、その具体型を使って関数本体を特殊化しない。

`f` のバイトコードは `x::Any` の汎用メソッドのままになる。

```text
[5433] f(x::Any) -> Any code=98537..98547 entry=98537
  slots:
    #0   x                            :: unknown param
    #1   __sjulia_inline_arg_g_0      :: unknown
  bytecode:
     98537: LoadSlot(0) ; slot #0 x::unknown
     98538: StoreSlot(1) ; slot #1 __sjulia_inline_arg_g_0::unknown
     98539: PushI64(2)
     98540: LoadSlot(1) ; slot #1 __sjulia_inline_arg_g_0::unknown
     98541: CallDynamicBinaryBoth(DynamicMul, [...])
     98542: PushI64(1)
     98543: CallDynamicBinaryBoth(DynamicSub, [...])
     98544: PushI64(1)
     98545: CallDynamicBinaryBoth(DynamicAdd, [...])
     98546: ReturnAny
```

`main` 側では、`CallSpecialize` ではなく `CallResolved(5433, 1)` となる。

```text
103330: PushI64(3)
103331: CallResolved(5433, 1) ; call #5433 f argc=1
```

`f` が型注釈付きの定義済みメソッドなので、呼び出し側はメソッド名から直接解決できる。
ただし `f` の本体は `T` が具体型に置き換わらないため、呼び出し地点特殊化は行われず、`g` も汎用命令を実行する。

Julia 本家では `f(3.0)` を呼ぶと `T` が `Float64` に束縛され、その特殊化メソッドの中に `g` が展開される。

```julia
# @code_typed f(3.0)
CodeInfo(
1 ─ %1 = intrinsic Base.mul_float(2.0, x)::Float64
│   %2 = intrinsic Base.sub_float(%1, 1.0)::Float64
│   %3 = intrinsic Base.add_float(%2, 1.0)::Float64
└──      return %3
) => Float64
```

sjulia では現状、型変数を使った呼び出し地点特殊化は行われない。
`T <: Number` の制約はメソッドディスパッチには機能するが、`f` の仮引数型が `g` に伝搬して `Float64` 専用のバイトコードになるわけではない。

### 呼び出し側を具体型メソッドでハードコードした場合

先ほどの例とは逆に、`f` 側を `Float64` / `Int64` / `ComplexF64` ごとに具体型メソッドとして書いた場合を見る。

```julia
g(x) = 2x - 1
f(x::Float64) = g(x) + 1
f(x::Int64) = g(x) + 1
f(x::ComplexF64) = g(x) + 1

f(3.0)
f(3)
f(2.0 + im)
```

この場合、`f` の各メソッドは型ごとに個別にコンパイルされ、`g` の中身がそれぞれの `f` メソッドに展開される。

```text
[5433] f(x::F64) -> F64 code=98537..98548 entry=98537
  slots:
    #0   x                            :: F64 param
    #1   __sjulia_inline_arg_g_0      :: F64
  bytecode:
     98537: LoadSlotF64(0) ; slot #0 x::F64
     98538: StoreSlotF64(1) ; slot #1 __sjulia_inline_arg_g_0::F64
     98539: PushI64(2)
     98540: ToF64
     98541: LoadMulF64Slot(1) ; slot #1 __sjulia_inline_arg_g_0::F64
     98542: PushI64(1)
     98543: ToF64
     98544: SubF64
     98545: PushI64(1)
     98546: CallDynamicBinaryBoth(DynamicAdd, [...])
     98547: ReturnF64

[5434] f(x::I64) -> I64 code=98548..98557 entry=98548
  slots:
    #0   x                            :: I64 param
    #1   __sjulia_inline_arg_g_1      :: I64
  bytecode:
     98548: LoadSlotI64(0) ; slot #0 x::I64
     98549: StoreSlotI64(1) ; slot #1 __sjulia_inline_arg_g_1::I64
     98550: PushI64(2)
     98551: LoadMulI64Slot(1) ; slot #1 __sjulia_inline_arg_g_1::I64
     98552: PushI64(1)
     98553: SubI64
     98554: PushI64(1)
     98555: CallDynamicBinaryBoth(DynamicAdd, [...])
     98556: ReturnI64
```

`f(x::Float64)` では浮動小数点専用命令が、`f(x::Int64)` では整数専用命令が使われている。
`g` 自体は汎用メソッドとして存在するが、`f` からは呼ばれず、各 `f` メソッドの中に `2x - 1` の計算がコピーされている。

`main` 側でも、`CallSpecialize` ではなく `CallResolved` で目的の `f` メソッドを直接呼び出す。

```text
103352: PushF64(3.0)
103353: CallResolved(5433, 1) ; call #5433 f argc=1
103355: PushI64(3)
103356: CallResolved(5434, 1) ; call #5434 f argc=1
```

このように、`f` 側を具体型メソッドでハードコードすれば型に特化したバイトコードが得られる。
ただし、組み合わせが増えるたびにメソッドを増やす必要があり、手間とコード重複が増える。

### まとめ

`f` の仮引数型が `g` にそのまま渡るわけではない。
`f` も `g` も個別に `x::Any` の汎用メソッドとして定義される。
`g` が `f` にインライン展開されていれば、`f` の呼び出し地点で引数型が見えている限り、`g` の中の計算も含めて型に特化したコードが生成される。
`g` が `f` に展開されずに独立したメソッドとして残れば、`f` の本体には `g` への関数呼び出しが残る。
そのとき `g` は `f` の仮引数型ではなく、`f` から渡された実際の値の型に基づいて、個別に特殊化されたり複数メソッドの中から適切なものを選んだりする。
抽象型アノテーション、複数の具体型メソッド、型変数を使ったパラメトリックメソッドがある場合、`g` は `CallDynamic` や `CallTypedDispatch` として残る。
一方、`f` 側を具体型メソッドでハードコードすれば、`g` の中身を展開して型に特化したバイトコードを得られる。

具体的には、次のようになる。

- `f` の中では `x` は `Any` として扱われている。
- したがって `g` には「`x` は `Int64` だ」という情報は渡らない。
- `g` は自分に渡された実際の値の型（実行時の型）を見て、個別に特殊化されたり複数メソッドの中から適切なものを選んだりする。

イメージとしては、`g` がもっと大きな関数（ループや分岐がたくさんあるなど）だと、コンパイラは「コピーするより呼び出した方が小さい」と判断して、`g` を `f` に展開しないかもしれない。

この挙動は Issue #10873 で追跡している。

### 補足：CallDynamic は型を見て実装を選ぶ分岐命令

`CallDynamic`（や `CallDynamicBinaryBoth` など）は、実行時に値の型タグを見て、それに応じた低レイヤの演算を選ぶ命令である。

具体例で言うと、

```text
CallDynamicBinaryBoth(DynamicAdd, [...])
```

は「スタック上の 2 値の型を見て、整数なら整数加算、浮動小数点数なら浮動小数点加算、複素数なら複素数加算、……を選んで実行する」という動作になる。

つまり、

- コンパイル時には「どの型が来るか」は分からない
- 実行時に実際の `Value` や `ValueType` を調べる
- その型に対応した実装（低レイヤ演算）を選んで計算する

この点が、`CallSpecialize` とは異なる。

`CallSpecialize` は「実行時に型が分かったら、その型専用のバイトコードを新しく生成してキャッシュする」のに対し、`CallDynamic` は「毎回値の型を見て、あらかじめ用意された汎用実装の中から選ぶ」だけである。

したがって、`CallDynamic` は「生成」ではなく「選択・分岐」を行う命令、という理解が正確である。

## JIT ではない実行時特殊化

sjulia の実行時特殊化は「実行時にコードを生成する」という点で JIT と似ているが、同じものではない。
この節でその違いと仕組みを整理する。

### sjulia は JIT バイトコンパイラではない

sjulia にも「型に応じた特殊化」という概念はある。
しかし、これを Julia 本家の **JIT** と同一視することはできない。

Julia 本家の JIT は **LLVM** を使って CPU が直接実行する機械語を生成する。
sjulia は実行前に VM 用のバイトコードを生成する **AOT** コンパイラである。

sjulia には **実行時バイトコード特殊化** もある。
`CallSpecialize` などは、実行時に現れた具体的な型に合わせて VM バイトコードを追加生成する。
ただし、生成されるのはあくまで VM が解釈するバイトコードであり、CPU の機械語ではない。

したがって、sjulia は「実行時特殊化を伴う AOT バイトコードコンパイラ」と表現するのが正確である。
「JIT バイトコンパイラ」と呼ぶと、LLVM による機械語生成を連想させて誤解を招く。

### 実行時特殊化の仕組み

sjulia の **実行時バイトコード特殊化** は、型注釈のない関数に対して、実行時に現れた引数型に合わせた専用バイトコードを生成する仕組みである。

コンパイラは、引数の型が静的に分からない関数呼び出しに対して `Instr::CallSpecialize` を生成する。
たとえば `f(x) = x + 1` の `x` に型注釈がなければ、`f(1)` や `f(3.0)` の呼び出しは通常の `Call` ではなく `CallSpecialize` になる。

`CallSpecialize` は初めて実行されるとき、実際の引数の `ValueType` を見て専用バイトコードの生成を試みる。
生成は `subset_julia_vm_vm/src/vm/specialize/` 以下の runtime specialization engine が担当する。
対応している構文には代入、加算代入、配列・フィールド代入、`for` / `foreach` / `while` / `if`、二項・単項演算、関数呼び出し、配列・タプル・範囲の構築などがある。
対応していない構文（`try` やネストした関数定義など）は特殊化に失敗し、元の汎用バイトコードへフォールバックする。

特殊化に成功すると、生成されたバイトコードはキャッシュされる。
代表的なものは `specialization_i64_cache` と `specialization_f64_cache` で、引数がすべて `Int64` または `Float64` の場合の高速経路である。
たとえば `mygcd(a, b)` のような未型注釈関数が、型付きループ内から I64 引数で呼ばれると、初回呼び出しで I64 専用のバイトコードが生成され、以降はそのキャッシュが使われる。

型付きループ（typed loop）は `CallSpecializeI64Slots` や `CallSpecializeF64Slots` といった融合命令を認識する。
これらは、ループ内で未型注釈関数が I64 や F64 引数で呼ばれる場合に、ループ本体を高速な型付き実行ブロックとして動かすための命令である。
キャッシュされた特殊化本体は、ループのたびに live cache から解決され、メソッドの再定義などでキャッシュが無効化された場合は自動的に generic 実行へ戻る。

特殊化に失敗したり、引数型がキャッシュと一致しなかったりすると、`CallSpecialize` は元の汎用バイトコードを実行する。
これにより「高速化できる場合は速く、できない場合は正しく動く」という挙動になる。

実行時特殊化は「実行時にコードを生成する」という点で JIT と似ているが、生成されるのは VM 用のバイトコードであり、CPU の機械語ではない。
この違いが、sjulia を「JIT を持たない静的パイプライン」と呼ぶ根拠である。

### 未解決の課題：型伝搬と TTFX のトレードオフ

前節の `f(x) = g(x) + 1` の例で見たように、sjulia では現状、`f` の仮引数型が未インラインの `g` に渡らない。
もし `f` の引数型を `g` まで伝搬させ、未インラインの呼び出しも含めて実行時に専用バイトコードを生成できれば、より多くの場面で最適な命令列を作れるようになる。

しかし、その仕組みを入れることは JIT への一歩を踏み出すことでもある。
呼び出しチェーンが深いほど、初回実行時に生成すべきバイトコードが増え、**TTFX（Time To First eXecution）** が悪化する。
Julia 本家の TTFX の大きな原因は、LLVM を通した機械語生成にある。
sjulia は VM バイトコードしか生成しないため、本家ほどの遅延にはならないが、それでも「実行時にコードを生成するコスト」は増える。

このため、sjulia には次のトレードオフがある。

| 選択 | 初回コスト | 実行中性能 | 備考 |
|---|---|---|---|
| 現状：呼び出し元の型を callee に伝搬しない | 小さい | `CallDynamic` / `CallTypedDispatch` 部分は非最適 | iOS/WASM の機械語生成禁止を満たす |
| 型を伝搬させて実行時に広く特殊化する | 大きくなる | より最適なバイトコードが作れる | JIT バイトコードコンパイラに近づく |

どこまで型を伝搬させ、どこまで実行時特殊化を進めるかは、sjulia にとって未解決の設計問題である。
現状は「機械語を生成しない」という制約を優先し、必要なところだけ呼び出し地点で特殊化する形を取っている。

## Rust 実装で得られること

Rust 実装の利点は、単に「速いコードを書ける」ことだけではない。
Rust はネイティブライブラリ、WebAssembly、C ABI を比較的同じ設計から出しやすい。

### Julia 本家とのソースコード規模の比較

リポジトリに追跡されているソースファイルの物理行数を、同じ拡張子基準で数えると次のようになる。
2026 年 7 月 12 日時点の sjulia `9ffc2bfae` と、`./julia` に置いた Julia 本家 `15346901f0` を比較した。

| 言語群 | sjulia | Julia 本家 |
|---|---:|---:|
| Rust (`.rs`) | 598,142 | 0 |
| Julia (`.jl`) | 231,625 | 352,370 |
| C / C++ (`.c`, `.h`, `.cpp`, `.cc`, `.cxx`) | 1,622 | 152,386 |
| クライアント (`.swift`, `.dart`, `.js`) | 25,517 | 0 |
| スクリプト (`.py`, `.sh`) | 24,963 | 1,201 |
| その他の低水準コード (`.s`, `.S`, `.ll`, `.scm`) | 0 | 15,224 |
| **合計** | **881,869** | **521,181** |

この集計は空行とコメントを含み、テスト、fixture、リポジトリ内の Julia パッケージ実装も含む。
ビルド生成物、Git が追跡しない依存物、文書、画像は含まない。
したがって、これはリポジトリが保守するソースの量を示す値であり、処理系中核の実装量や機能の充実度を直接比較する値ではない。
sjulia の行数が Julia 本家を上回るのは、VM 本体に加えて、多数の回帰テスト、互換 fixture、モバイルと Web のクライアントを同じリポジトリで管理しているためである。

iOS では、Rust の VM を **static library** としてビルドし、SwiftUI アプリから C ABI 経由で呼べる。
この形なら、アプリの中で小さな Julia 風コードを評価できる。

Android でも、同じ Rust コアを **native library** として組み込む道がある。
ホスト側の UI は別でも、構文解析、コンパイル、VM 実行の中核を共有できる。

Web では、`wasm-bindgen` を使って Rust コアを **WebAssembly** として公開する。
ブラウザは任意のネイティブ機械語を実行中に生成できないが、WASM モジュールとして配布した VM は動かせる。

この移植性は、Julia 本体の代替を意味しない。
研究室の大きな計算は Julia 本体で走らせ、教材、デモ、モバイルアプリ、ブラウザ上の小さな実験は sjulia で走らせる、という分担が自然である。

## 実行パフォーマンス

SubsetJuliaVM の性能は、書き方と実行経路で大きく変わる。
JIT を持つ Julia 本体の置き換えとして全領域で勝つ設計ではない。

以下は 2026 年 7 月 13 日に計測した処理系間比較である。
sjulia VM の数値は prelude/Base cache を埋め込んだ release バイナリでの warm run である。
Mandelbrot の 2 表の checksum 列は、各実行系が同じ計算結果を出していることの確認値である。

表の行が指す実行系は次の通りである。

| 行 | 実行系 |
|---|---|
| Julia upstream | 公式 Julia（JIT あり） |
| sjulia VM typed | 型注釈つきソースを sjulia の Stack VM で実行 |
| sjulia VM untyped | 型注釈を除いた同じソースを sjulia の Stack VM で実行（実行時特殊化の効果を見る） |
| juliars | sjulia の AoT バックエンド。Julia ソースからネイティブバイナリを生成して実行 |
| Python 3.14 (uv) | CPython 3.14（uv で実行環境を固定） |
| Cython (uv) | `cdef` 型注釈と手書き最適化を含む Cython 拡張（追試。coprime π の表のみ） |

### coprime π 推定

互いに素な整数対の確率から π を推定する課題である。
`N=5000` と `N=10000` の 2 ケースで比較する。

| Runtime | N=5000 | N=10000 |
|---|---:|---:|
| Julia upstream | 0.57 s | 2.12 s |
| sjulia VM typed | 2.71 s | 11.21 s |
| sjulia VM untyped | 2.41 s | 9.99 s |
| juliars | 0.44 s | 1.89 s |
| Python 3.14 (uv) | 3.99 s | 16.76 s |
| Cython (uv) | 0.44 s | 1.94 s |

使用したソースファイルは次の通りである。
`N=10000` の版も同じ構造で、`calc_pi(10000)` となっている。

#### untyped / Julia upstream / Python 3.14 (uv) 共通

`benchmarks/calc_pi_n5000.jl` / `calc_pi_n10000.jl`

```julia
# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

result = calc_pi(5000)
println("N=5000: π ≈ ", result)
```

#### sjulia VM typed

`benchmarks/calc_pi_n5000_typed.jl` / `calc_pi_n10000_typed.jl`

```julia
# Estimate π using coprime probability (fully typed)
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a::Int64, b::Int64)::Int64
    while b != 0
        tmp::Int64 = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N::Int64)::Float64
    cnt::Int64 = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob::Float64 = cnt / N / N
    sqrt(6.0 / prob)
end

result::Float64 = calc_pi(5000)
println("N=5000: π ≈ ", result)
```

#### juliars (AoT)

`benchmarks/calc_pi_n5000_aot.jl` / `calc_pi_n10000_aot.jl` は untyped 版と同じ構造で、末尾の `println` を行わず `calc_pi(5000)` の呼び出しで終わる点だけが異なる。

### Mandelbrot — scalar for-loop

1500×1500 の格子、最大反復 500、ComplexF64 の for-loop 版である。

| Runtime | Time | checksum |
|---|---:|---:|
| Julia upstream | 0.52 s | `247910238` |
| sjulia VM typed | 2.36 s | `247910238` |
| sjulia VM untyped | 2.92 s | `247910238` |
| juliars | 0.52 s | `247910238` |
| Python 3.14 (uv) | 21.03 s | `247910238` |

#### ソースコード

##### typed / Julia upstream / juliars (AoT) 共通

`benchmarks/mandelbrot_bench_for.jl`

```julia
function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandel_count(width::Int64, height::Int64, maxiter::Int64)::Int64
    total = 0
    for y in 1:height
        ci = -1.2 + 2.4 * (y - 1) / (height - 1)
        for x in 1:width
            cr = -2.0 + 3.0 * (x - 1) / (width - 1)
            total += mandel_point(cr + ci * im, maxiter)
        end
    end
    total
end

function run_one(w::Int64, h::Int64, m::Int64)
    t0 = time_ns()
    r = mandel_count(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandel_count(200, 200, 100)
run_one(1500, 1500, 500)
```

##### untyped

`benchmarks/mandelbrot_bench_for_untyped.jl` は、上の typed 版から引数・戻り値の型注釈をすべて除いた同一構造のソースである。

### Mandelbrot — broadcast

1700×1360 の格子、最大反復 500、ComplexF64 の broadcast 版である。

| Runtime | Time | checksum |
|---|---:|---:|
| Julia upstream | 0.54 s | `254750243` |
| sjulia VM typed | 2.23 s | `254750243` |
| sjulia VM untyped | 3.88 s | `254750243` |
| juliars | 0.53 s | `254750266` |
| Python 3.14 + NumPy (uv) | 3.94 s | `254750230` |

#### ソースコード

##### typed / Julia upstream 共通

`benchmarks/mandelbrot_bench_broadcast.jl`

```julia
function mandelbrot_escape(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

function mandelbrot_grid(width::Int64, height::Int64, maxiter::Int64)
    xs = range(-2.0, 1.0; length=width)
    ys = range(1.2, -1.2; length=height)
    C = xs' .+ im .* ys
    counts = mandelbrot_escape.(C, maxiter)
    sum(counts)
end

function run_one(w::Int64, h::Int64, m::Int64)
    t0 = time_ns()
    r = mandelbrot_grid(w, h, m)
    t1 = time_ns()
    println(w, "x", h, " maxiter=", m, " total=", r, " t=", (t1 - t0) / 1.0e9)
end

mandelbrot_grid(50, 40, 50)
run_one(1700, 1360, 500)
```

##### untyped

`benchmarks/mandelbrot_bench_broadcast_untyped.jl` は、上の typed 版から型注釈をすべて除いた同一構造のソースである。

### 結果の読み方

これらの結果から読めることは単純である。
型が分かる計算では、sjulia VM は Python の純粋なループより速く、Julia 本体より遅い。
**AoT** 経路はこれらの benchmark では Julia 本体に近いが、対応範囲は標準 VM より狭い。

ただし、untyped でも実行時特殊化が効くシンプルな数値計算では、typed に近い速度やそれ以上に出ることもある（coprime π では untyped が typed を上回った）。
複素数を含む Mandelbrot broadcast も、かつては untyped が ~45 s と typed の約 20 倍遅かったが、Issue #10704 で bulk typed broadcast kernel が呼び出し地点の `Matrix{ComplexF64}` を per-element callee へ伝播するようになり、Issue #10799 で実行時特殊化の ComplexF64 コード生成（`z*z+c`/`abs2(z)`）が静的コンパイラと同じ fusion 可能な命令形へ書き換えられたことで、untyped は今回の測定で 3.88 s（typed の約 1.7 倍）まで縮まった。scalar for-loop 版の untyped も同様に ~32 s から 2.92 s（typed の約 1.2 倍）へ縮んでいる。
残差は specialized body の SROA 形状が compile-time typed 本体ほど完全には fusion されないことにあり（`mandelbrot_escape` のループ本体で 27→14 TypedLoopOp、static は 8）、generic dispatch + boxed `Value` 経路そのものではない。
数値計算では、配列や変数の型が安定しているほど、コンパイラと VM が選べる高速経路が増える。

### Cython 版の追試

同じマシンで Cython 版も追測した。
ソースは `benchmarks/cython/calc_pi.pyx`、`mandelbrot_for.pyx`、`mandelbrot_broadcast.pyx` に置き、`uv` 環境で `cythonize` してネイティブ拡張として実行した。
数値は coprime π の表の Cython 行に載せている（Mandelbrot の Cython 値は最新の再計測に含めていない）。

Cython 版は `cdef` による型注釈と、Mandelbrot では `ComplexF64` を使わない実数分解の手書き最適化を含む。
特に Mandelbrot では、`z = z * z + c` を Julia の `ComplexF64` 型として書くのではなく、実部 `zr` と虚部 `zi` を別々の `double` 変数に分解してから手書きで計算している。
この実数分解は人間による最適化であり、Julia や sjulia がソースから自動で行う抽象化とは異なる。
したがって、これらの数値は「同じ Julia ソースを別の実装で動かす」という公平な言語処理系比較ではなく、Cython における手書き最適化の上限を示す追試である。

したがって、SubsetJuliaVM の実用上の狙いは「どんな Julia コードでも最速にする」ではない。
狙いは、型が見えやすい数値計算を、JIT を置けない場所へ十分な速度で運ぶことである。

## 読み進める入口

実装を追うなら、まず [用語集](GLOSSARY.md)の「全体の地図」を読むとよい。
その後で、構文解析、lowering、Core IR、型推論、バイトコード VM の順に読むと、この記事の流れと対応する。

コード上の入口は、Rust API、parser、lowering、compiler、VM に分かれる。
本文で使った語の細かい意味は用語集へ寄せているため、初めて出てきた語で止まったら、先に用語集を確認すると読みやすい。

SubsetJuliaVM は、Julia 本体を小さくしただけのものではない。
JIT を中心にした Julia 本体とは別の制約から、Rust、バイトコード VM、C ABI、WASM を組み合わせている。

その制約があるから、Julia 風の計算をスマートフォンやブラウザへ持ち出す余地が生まれる。
