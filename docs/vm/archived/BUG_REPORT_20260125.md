# バグレポート

> **Archive note (2026-06-11):** This preserves an older fixed-bug writeup
> from January 2026. Current implementation history lives in `docs/vm/STATUS.md`
> and `docs/vm/DONE.md`; open and newly discovered bugs should be tracked as
> GitHub Issues.

**最終更新**: 2026-01-25

> 修正済みのバグとその原因を記録し、将来の参考とする。

---

## Issue #1330: `@show` マクロがユーザー定義関数で `Nothing` を返す

**状態**: ✅ 修正済み (PR #1345)

### 症状

```julia
f(x) = 2x + 1
result = @show f(3)  # "f(3) = 7" と表示されるが、result は Nothing
```

`@show` マクロは値を表示した後、その値を返すべきだが、ユーザー定義関数を引数に取ると `Nothing` を返していた。

### 原因

**マクロローカル変数のクォート展開時の不正な処理**

`@show` マクロの定義：
```julia
macro show(ex)
    expr_str = string(ex)   # マクロ展開時に "f(3)" を計算
    :(_do_show($expr_str, $(esc(ex))))  # クォート内で $expr_str を使用
end
```

問題は、マクロ本体で定義されたローカル変数 `expr_str` が、クォート展開時に正しく置換されていなかったこと。

#### 修正前の展開結果

```julia
begin
    expr_str = "f(3)"           # 実行時の代入として追加されてしまう
    _do_show(expr_str, f(3))    # expr_str は変数参照のまま
end
```

この結果、ブロック全体の戻り値が `_do_show` の戻り値ではなく、ブロック自体の評価結果（`Nothing`）になっていた。

#### 修正後の展開結果

```julia
_do_show("f(3)", f(3))  # 文字列リテラルが直接埋め込まれる
```

### 技術的詳細

**問題のあったコード**: `subset_julia_vm_lowering/src/lowering/stmt/macros.rs`

複数文を持つマクロ本体を展開する際：
1. 代入文 `Stmt::Assign` が `expanded_stmts` に追加されていた
2. クォート展開時、`$expr_str` は `Expr::Var("expr_str")` として処理され、ローカル変数の値が代入されなかった

**修正内容**:
1. マクロローカル変数の代入を `HashMap<String, Expr>` で追跡
2. 代入文は展開後のコードに追加せず、値のみを保存
3. クォート展開時に `substitute_local_bindings_in_constructor()` でローカル変数を値に置換

### 関連ファイル

- `subset_julia_vm_lowering/src/lowering/stmt/macros.rs` - マクロ展開ロジック
- `subset_julia_vm_lowering/src/lowering/expr/quote/main.rs` - クォート展開ロジック
- `subset_julia_vm/src/julia/base/macros.jl` - `@show` マクロ定義

### 教訓

1. **マクロ展開とランタイムの区別**: マクロ本体の代入はコンパイル時に評価されるべきで、ランタイムコードに含めてはならない
2. **クォート内の変数参照**: `$var` は、マクロパラメータだけでなくマクロローカル変数も正しく解決する必要がある
3. **Julia のマクロセマンティクス**: Julia では `$` による補間はマクロ展開時に評価される

---

## Issue #1447: try ブロック内からの return が Nothing を返す

**状態**: ✅ 修正済み (2026-01-24)

### 症状

```julia
function test()
    try
        return 42
    catch
        return -1
    end
end

result = test()  # 42 を期待するが Nothing が返る
```

try ブロック内から `return` すると、値が正しく返されず `Nothing` になっていた。

### 原因

**抽象インタプリタの `Stmt::Try` 処理欠落**

- 抽象インタプリタ（型推論エンジン）が `Stmt::Try` を `_ => Continue` でフォールスルーしていた
- 関数の戻り型が実際の return 型ではなく `Nothing` と推論されていた
- 結果として、関数呼び出し後の値が `Nothing` として扱われていた

### 修正内容

1. `Stmt::Try` の型推論を追加（try/catch/else/finally 各ブロックの解析）
2. try ブロック内の return 文から正しく型が伝播するように修正
3. `pop_handlers_for_return` ヘルパーで例外ハンドラのクリーンアップを追加

### 関連ファイル

- `subset_julia_vm_compile/src/compile/abstract_interp/engine/mod.rs` - Try 文の型推論
- `subset_julia_vm_vm/src/vm/mod.rs` - `pop_handlers_for_return` ヘルパー
- `subset_julia_vm_vm/src/vm/exec/return_ops.rs` - return 時のハンドラクリーンアップ

### 教訓

1. **型推論の網羅性**: 新しい文（statement）を追加したら、抽象インタプリタでも対応が必要
2. **例外ハンドラのライフサイクル**: return 時に例外ハンドラスタックを適切にクリーンアップする必要がある

---
