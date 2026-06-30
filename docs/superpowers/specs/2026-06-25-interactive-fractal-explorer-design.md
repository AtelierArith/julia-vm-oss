# Interactive fractal explorer (`@manipulate`) — design

- **Date**: 2026-06-25
- **Status**: Approved (brainstorming) — pending implementation plan
- **Owner / branch**: `feat/ifs-fractals-explorer` (off `main`)
- **Drives**: replace the `barnsley_fern` iOS/Web/mobile sample with a
  dropdown-driven IFS fractal explorer; file + (attempt to) fix two upstream
  parity gaps discovered while building it.

## 1. 動機 / ゴール

現状、`barnsley_fern.jl` は **単一フラクタル**（Barnsley fern を chaos game で描く
散布図）のサンプル。一方リポジトリには Interact.jl の MVP `@manipulate`（Issue
#7275）があり、離散選択を **1 つの静的 Plotly dropdown 図**にまとめて表示できる。

このサンプルを **複数の IFS フラクタルを dropdown で切り替えられる**形に置き換える。
Barnsley fern はその選択肢の 1 つになる。ユーザは dropdown から
Fern / Sierpinski 三角形 / Heighway ドラゴンを選んで、それぞれの chaos game の
結果を同じ図の中で見比べられる。

成果物（ユーザ決定）:

- `barnsley_fern` サンプルを **置き換える**（新規追加ではない）。
- dropdown のフラクタル集合 = **Fern + Sierpinski 三角形 + Heighway ドラゴン**。
- 登録先 = **iOS + Web + Flutter/Android** の全プラットフォーム。
- 途中で見つけた 2 つの sjulia gap は **起票し、本作業内で修正も試みる**。

## 2. 実現可能性（実機検証済み）

`target/release/sjulia`（本日ビルド）で proto を実行し、以下を確認済み:

```julia
using Interact, Plots, Distributions, Random
struct Affine
    W::Matrix{Float64}
    b::Vector{Float64}
end
(a::Affine)(x) = a.W * x + a.b
m = @manipulate for fractal = [:fern, :sierpinski, :dragon]
    Random.seed!(42)
    if fractal == :fern
        maps = ( … 4 写像 … ); picker = Categorical([0.01,0.85,0.07,0.07]); ttl = "Barnsley Fern"
    elseif fractal == :sierpinski
        maps = ( … 3 写像 … ); picker = Categorical([1/3,1/3,1/3]); ttl = "Sierpinski Triangle"
    else
        maps = ( … 2 写像 … ); picker = Categorical([0.5,0.5]); ttl = "Heighway Dragon"
    end
    n = 5000; xs = zeros(n); ys = zeros(n); p = [0.0,0.0]
    for i in 1:n
        idx = rand(picker); p = maps[idx](p); xs[i] = p[1]; ys[i] = p[2]
    end
    scatter(xs, ys; aspect_ratio = :equal, title = ttl)
end
```

→ `typeof(m) == Interact.Manipulate`、`length(m.plots) == 3`、
`m.labels == ["fern","sierpinski","dragon"]`、`m.control == :dropdown`。exit 0。
CLI 直実行では artifact は stdout に出ない（既存 `interact_manipulate.jl` と同じ
挙動＝正常）。dropdown JSON は FFI/表示経路（`plotting/plotly.rs::generate_plotly_manipulate_json`）
で生成され、iOS/Web で描画される。

### IFS の定義（標準値）

- **Barnsley fern**（4 写像、prob `[0.01,0.85,0.07,0.07]`）— 既存サンプルの値を流用。
- **Sierpinski 三角形**（3 写像、各 scale 0.5、頂点 `(0,0),(1,0),(0.5,0.866)` へ、
  prob `[1/3,1/3,1/3]`）。
- **Heighway ドラゴン**（2 写像、`W=[0.5 -0.5;0.5 0.5] b=[0,0]` と
  `W=[-0.5 -0.5;0.5 -0.5] b=[1,0]`、prob `[0.5,0.5]`）。

## 3. 発見した 2 つの gap（upstream julia 1.12.6 では動作 / sjulia で失敗）

リポジトリ方針（CLAUDE.md「Unsupported-Feature Discovery Rule」/ `sjulia-report-gap`）
に従い、いずれも Issue 起票が必須。根本原因まで特定済み。

### Gap A — `@manipulate` 本体内のタプル分割代入

```julia
@manipulate for k = 1:3
    a, b = f(k)     # ← Runtime error: ErrorException: Unknown function: =
    scatter(a, b)
end
```

- 最小再現: `a, b = f(k)` のような **分割代入を含むブロック**が `@manipulate` 本体に
  あると失敗。単一代入・ネスト for・添字代入は動く。
- 原因: マクロが本体ブロックを `push!(_interact_plots, <body>)` の **引数位置**に
  splice し、その中の `a,b=…` が `=` 呼び出しとして誤 lowering される
  (`lowering/macro_runtime.rs:353` 周辺の判定が関与)。
- 分類: `bug`（マクロ展開/lowering の不具合。構文自体は upstream で有効）。

### Gap B — 関数戻り値 / struct フィールド経由の分布への `rand`

```julia
function picker_for(name); name==:a ? Categorical([0.5,0.5]) : Categorical([1/3,1/3,1/3]); end
pk = picker_for(:b)
rand(pk)   # ← Type error: rand expected an RNG or non-negative integer dimension, got StructRef
```

- 最小再現: `Categorical(...)` を **関数戻り値 / struct フィールド**として受け取った値に
  `rand` すると失敗（`typeof` は `Categorical{Float64}` で正しい）。
  **直接ローカル束縛** `picker = Categorical([...])` なら同じ `rand(picker)` が動く。
- 原因: vended な値は推論上 `Any`/StructRef になり、`rand(x)` 呼び出しが
  **pure-Julia の `rand(d::Distribution)`（`Distributions.jl:153`）へ dispatch されず**、
  Rust builtin `rand`（`vm/exec/rng.rs:273` `rng_value_to_dim`）に落ちて
  「StructRef は次元数でない」と error。
- 修正パターン: **builtin が非RNG・非整数（struct/Any）引数を受けたら、ユーザ定義
  `rand` メソッドへ defer する**。先行例 #6657（getindex/first/last(::Any)）/
  #6610（haskey/isempty/empty!(::Any)）/ #6638（iterate(::Any)）と同型。
- 分類: `bug`（dispatch 不具合。同じ構文がローカル束縛では動く非対称性）。

## 4. 作業項目（各々独立した logical commit / PR）

`sjulia-logical-commits` 準拠で 1 PR = 1 まとまり。順序と依存:

| # | 内容 | 依存 | リスク |
|---|------|------|--------|
| **W0** | Gap A・Gap B を `bug` Issue 起票（MWE + julia/sjulia 出力表）。`report-issue` スキル使用 | — | 低 |
| **W1** | **Gap B 修正**: builtin `rand` 単一引数が struct/Any のときユーザ `rand(d::Distribution)` へ defer。回帰 fixture（vended Distribution への `rand`）+ parity | W0 | 中 |
| **W2** | **Gap A 修正**: 引数位置ブロック内の分割代入 lowering。回帰 fixture（`@manipulate` 本体で `a,b=f(x)`）+ parity。**詰まれば Issue 起票のみで打ち切り**（サンプルは非依存） | W0 | 中 |
| **W3** | **サンプル置き換え**（主成果物）。下記方針で W1/W2 非依存 | — | 低 |

W0 → (W1 ∥ W2) → W3 だが、**W3 は W1/W2 の着地に依存しない**（直接ローカル束縛
パターンで今日のビルドで動く）。各 PR は独立にマージ可能。

## 5. 設計判断

### 5.1 サンプルのコードパターン → 直接ローカル束縛（採用）

フラクタル定義（maps と picker）を **`@manipulate` 本体内の `if/elseif` 分岐で直接
ローカル束縛**する（§2 の proto3 形）。

- 利点: W1/W2 未着地でも **今日のビルドで確実に動く**（両 gap を踏まない）。教材
  としても全定義が 1 か所に集まり読みやすい。サンプルを VM 修正に結合させない。
- 代替（非採用）: W1/W2 着地後に `Dict` ルックアップ + `Categorical(sys.probs)` の
  クリーン形へ。エレガントだが Gap B 修正の着地に結合しリスク増。

### 5.2 サンプル id → 改名（採用）

内容が複数フラクタルになるため `barnsley_fern` → **`ifs_fractals`**
（name: "Interactive fractals (@manipulate)"）へ改名。id `barnsley_fern` のままだと
ミスリード。iOS/Web/mobile/tests/Swift の参照を更新する。

### 5.3 点数

各フラクタル `n = 5000`（既存 barnsley と同じ）。3 図合算 = 15000 点を 1 つの
Plotly 図に載せる。iOS で重い場合は n を下げる（描画確認時に判断）。

## 6. テスト / 触る場所（W3）

- `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/ifs_fractals.jl`
  （旧 `barnsley_fern.jl` を置換・改名）
- `SubsetJuliaVMApp/.../Resources/Samples/samples.json`（id/name/description/tags）
- `SubsetJuliaVMApp/.../Models/CodeSamples+Intermediate.swift`（barnsley フォール
  バックがあれば更新／無ければ不要 — 実装時に確認）
- `web/samples_ir.js`（`barnsley_fern` エントリを差し替え）
- `mobile/assets/samples/intermediate/ifs_fractals.jl` + `mobile/assets/samples/samples.json`
- `SubsetJuliaVMApp/.../SubsetJuliaVMAppTests/SampleCodeTests.swift`（サンプルが
  エラーなく artifact を生成することを検証）
- W1/W2 用の Rust fixture（`subset_julia_vm/tests/fixtures/packages/` or `metaprogramming/`）
  + `manifest.toml` + `bash scripts/check_fixture_test_names.sh`
- docs: `docs/vm/STATUS.md` / `docs/vm/DONE.md`（日次ヘッダ #NNNN サブセクション）

## 7. 検証ゲート

- `target/release/sjulia` でサンプルが exit 0（`Interact.Manipulate` を構築）。
- `bash scripts/fixture_julia_parity.sh` で各回帰 fixture が julia と一致。
- 該当カテゴリ nextest → pre-PR で `timeout 1800 cargo nextest run --release`。
- `cargo clippy --all-targets -- -D warnings` / `cargo fmt --check`（自分のファイル）。
- iOS / Web ビルド（サンプルが読み込まれること）。

## 8. 非ゴール / 先送り

- 真のリアクティブ `@manipulate`（live 再評価・双方向 FFI）= Interact MVP の
  Phase 2（Issue #7275）のまま。本作業は静的 dropdown のみ。
- Sierpinski カーペット等の追加フラクタルや slider 化は対象外。
- Gap A/B の包括的な dispatch リファクタリングは対象外（最小の defer 修正に留める）。

## 9. 未解決 / 実装時に確定する点

- `CodeSamples+Intermediate.swift` に barnsley のフォールバックが存在するか
  （存在すれば更新、しなければスキップ）。
- 3 図合算の Plotly 図の iOS 描画負荷（必要なら n を調整）。
- W2(Gap A) の lowering 修正が予算内で収まるか（収まらなければ Issue のみ）。
