# Web Playground

`web/` は SubsetJuliaVM の静的ブラウザ Playground です。`web/pkg` の WASM package を読み込み、Julia source を編集し、`run_from_source` で実行し、VM が返した Plotly artifact を描画します。

## ファイル

```text
web/
  index.html          page shell と script 読み込み順
  app.js              Playground state、Run、Share、出力描画、サンプル選択 dropdown
  julia-language.js   Monaco Editor 用 Julia syntax 定義・補完
  samples_ir.js       sample source。iOS アプリのサンプルと同一内容
  styles.css          layout / theme（モバイルファースト）
  plotly.min.js       2D/3D plot 用の local Plotly bundle
  jsxgraph.min.js     JSXGraph board 描画用の local bundle（iOS と同一 1.12.2, Issue #7286）
  server.py           WASM MIME type 対応の local static server
  test.html           browser sample runner
  test-runner.js      test.html 用 Playwright wrapper
  pkg/                wasm-pack 生成物
```

Rust や WASM API を変更したら `pkg/` を再生成します。

## ビルドと起動

リポジトリルートから:

```bash
scripts/wasm_build_with_cache.sh
cd web
python3 server.py
```

起動後:

```text
http://localhost:8080
```

Base cache を埋め込まない通常 build:

```bash
cd subset_julia_vm_web
wasm-pack build --target web --profile web-release --out-dir ../web/pkg
cd ../web
python3 server.py
```

## 実行時の挙動

Playground は editor 内容を常に次で実行します。

```javascript
wasm.run_from_source(code, BigInt(42));
```

結果は text output、numeric result、error、Plotly graph、または JSXGraph board として表示します。`using Plots; plot(sin)` は `artifact_mime = "application/vnd.plotly+json"` を返し `plotly.min.js` で描画されます。`using JSXGraph; html(board)` は `artifact_mime = "application/vnd.jsxgraph+json"` を返し、`app.js::renderJsxgraph` が `jsxgraph.min.js` でインタラクティブな board を描画します(Issue #7286)。

startup 中、`app.js` は warmup として次を一度実行します。warmup が終わるまで Run button は無効です。

```julia
using Plots
plot(sin)
```

これは初回の user plot 実行と同じ WASM compile path を温めるためです。画面の出力は更新しません。Run button を有効化する前に warmup を完了させることで、最初の user execution が cold path を踏むのを避けます (Issue #6127)。

## モバイル UI

- 狭い画面では **Edit / Output** のタブ切り替えになります。
- エディタはスマートフォン入力に安定した `<textarea>` を使用します。
- サンプル選択は画面上部の dropdown（`<select>`、カテゴリ別 optgroup）です。
- デスクトップではエディタと出力が左右に並びます（768px 以上）。

## Samples

sample は iOS アプリ (`SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/`) と同一の `samples.json` + `.jl` ファイルから `web/samples_ir.js` に変換されます。

```javascript
{
  id: "hello_world",
  name: "Hello World",
  category: "Basic",
  description: "The classic first program...",
  difficulty: "Beginner",
  tags: ["print", "string"],
  folder: "beginner",
  code: `...`,
  ir: null,
  webUnsupported: false
}
```

Playground は `code` field を使います。`ir` field は historical な名前の残りです。iOS 専用パッケージに依存する sample（JSXGraph、Distributions、Symbolics、Primes）は `webUnsupported: true` として listing されますが、web 上では実行せず代替メッセージを表示します。

`samples_ir.js` を変更したら、`app.js` の import query を上げます。

```javascript
import { samplesIR } from './samples_ir.js?v=28';
```

## Plotly / JSXGraph

plot/board 描画は local file の `plotly.min.js` と `jsxgraph.min.js` に依存します(CDN 非依存)。

どちらも UMD ラッパを持ち、Monaco の RequireJS loader(`define.amd` を定義)が**先に**読み込まれると、グローバル `Plotly` / `JXG` を設定せず AMD module として登録してしまい、プロット/ボードが無言で描画されなくなります(Issue #7286)。そのため `index.html` では両スクリプトを **Monaco loader より前**(`<head>` 内)に読み込みます。順序を変えないでください。

Plotly version を変える場合:

1. `web/plotly.min.js` を置き換える。
2. `index.html` の `plotly.min.js?v=...` query を更新する。
3. browser で `using Plots; plot(sin)` と 3D sample を確認する。

JSXGraph version を変える場合:

1. `web/jsxgraph.min.js` を置き換える(iOS の `SubsetJuliaVMApp/.../Resources/jsxgraph.min.js` と揃える)。
2. `index.html` の `jsxgraph.min.js?v=...` query を更新する。
3. browser で `jsxgraph_demo` / `apollonian_gasket` sample を確認する。

## 検証

Rust 側の WASM binding test:

```bash
timeout 1800 cargo nextest run --release -p subset_julia_vm_web
```

browser sample runner:

```bash
cd web
python3 server.py
node test-runner.js --server-url=http://localhost:8080
```

Playwright runner にはローカル環境で使える browser installation が必要です。

## よくある失敗

`WASM module not loaded` は `web/pkg` がない、または古い状態です。`wasm-pack` で再ビルドします。

`Plotly.js not loaded` は `app.js` が artifact を描画する前に `plotly.min.js` が実行されていない状態です。`index.html` の script order と browser console を確認します。

初回実行が遅い場合は、embedded Base cache が使われていないか、startup warmup が失敗している可能性があります。`scripts/wasm_build_with_cache.sh` は Base compile cost を避けるための helper で、`run_from_source` の cold path 全体を単独で消すものではありません。
