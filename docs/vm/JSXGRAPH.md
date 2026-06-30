# JSXGraph.jl 統合

Issue #6357

## 概要

`sjulia` 上で [JSXGraph](https://jsxgraph.org/) による対話的な幾何作図を行うための Pure-Julia サブセット実装。
Julia 側で `Board`（盤）と `JSXElement`（要素）を組み立て、最終式が `Board` 値を返すと
Rust 側が `application/vnd.jsxgraph+json` artifact を生成する。

## 対応要素

| 要素 | Julia API | 備考 |
|------|-----------|------|
| board | `board(id="box"; xlim, ylim, axis, grid, width, height, ...)` | `boundingbox=[xmin,ymax,xmax,ymin]` を生成 |
| point | `point(x, y; name, color, size, ...)` | parents: `[x, y]` |
| line | `line(a, b; name, color, ...)` | a/b は `JSXElement` 参照可 |
| segment | `segment(a, b; ...)` | parents: `[a, b]` |
| circle | `circle(center, r; ...)` | parents: `[center, r]` |
| polygon | `polygon(p1, p2, p3, ...; ...)` | parents: `[p1, p2, ...]` |
| text | `text(x, y, s; ...)` | parents: `[x, y, s]` |
| functiongraph | `functiongraph(f; a=-5, b=5, n=100, ...)` | Julia 側でサンプリング → `type:"curve"` |
| view3d | `view3d(position, size, ranges; ...)` | `Board` 内に 3D view と nested elements を保持 |
| curve3d | `curve3d(fx, fy, fz, range; ...)` | `fx/fy/fz` 文字列を `JSFunction` として保持 |
| point3d | `point3d(x, y, z; ...)` | `View3D` の子要素 |
| line3d | `line3d(a, b; ...)` | 3D point 参照を `{"ref": id}` で接続 |

## 基本的な使い方

```julia
using JSXGraph
b = board("box"; xlim=(-5, 5), ylim=(-5, 5))
A = point(1, 2; name="A")
B = point(-3, -1; name="B")
l = line(A, B)
push!(b, A, B, l)
html(b)
```

`html(b)` は `b` をそのまま返す。REPL/iOS/Web ホストは最終式の `Board` 値を検出して
`application/vnd.jsxgraph+json` artifact として受け取る。

## 3D と do-block

```julia
using JSXGraph

b = board(; xlim=(-5, 5), ylim=(-5, 5), axis=false) do board_ref
    v = view3d([-4.0, -3.0], [8.0, 8.0],
               Any[Any[-2.0, 2.0], Any[-2.0, 2.0], Any[-2.0, 2.0]])
    c = curve3d("1.8*Math.sin(3*t + Math.PI/2)",
                "1.8*Math.sin(4*t)",
                "1.8*Math.sin(5*t)",
                [0.0, 2*pi])
    push!(v, c)
    push!(board_ref, v)
end

html(b)
```

`board(...) do b ... end` と `view3d(...) do v ... end` は、内側で要素を `push!` して
構築済みの `Board` / `View3D` を返す。`curve3d` の文字列引数は `JSFunction(code, :t)`
に包まれ、artifact では `{"jsfunc": code, "var": "t"}` として出力される。

## Artifact 形式

```json
{
  "options": {
    "boundingbox": [-5, 5, 5, -5],
    "axis": true,
    "grid": false,
    "width": 500,
    "height": 500,
    "showNavigation": false,
    "showCopyright": false
  },
  "elements": [
    {"id": 1, "type": "point", "parents": [1, 2], "attrs": {"name": "A"}},
    {"id": 2, "type": "point", "parents": [-3, -1], "attrs": {"name": "B"}},
    {"id": 3, "type": "line", "parents": [{"ref": 1}, {"ref": 2}], "attrs": {}},
    {
      "id": 4,
      "type": "view3d",
      "parents": [[-4, -3], [8, 8], [[-2, 2], [-2, 2], [-2, 2]]],
      "attrs": {},
      "elements": [
        {
          "id": 5,
          "type": "curve3d",
          "parents": [
            {"jsfunc": "1.8*Math.sin(3*t)", "var": "t"},
            {"jsfunc": "1.8*Math.sin(4*t)", "var": "t"},
            {"jsfunc": "1.8*Math.sin(5*t)", "var": "t"},
            [0, 6.283185307179586]
          ],
          "attrs": {}
        }
      ]
    }
  ]
}
```

- `id` は各 `JSXElement` に一意に割り当てられた整数。
- 要素参照は `{"ref": id}` として表現される。
- `functiongraph` は Julia 側で `[a,b]` 上を `n` 点サンプリングし、`type:"curve"`、
  `parents:[[xs...], [ys...]]` として出力する。
- `View3D` は top-level element として `elements` を内包し、frontend は `view.create(...)`
  で子要素を作成する。
- `JSFunction` は JSON 文字列ではなく `{"jsfunc": code, "var": var}` として表現される。

## 既知の制限

- **JS 関数トランスパイルは行わない**。2D `functiongraph` は数値サンプリングのみ。
  3D `curve3d` は明示的な raw JS 式文字列を `JSFunction` として渡す。
- 3D surface 系 (`surface3d`, `parametricsurface3d`, `functiongraph3d`) は未対応。
- JSXGraph 本体は web/iOS に同梱した 1.12.2 の機能に依存する。

## 関連ファイル

- `subset_julia_vm/packages/JSXGraph/src/{JSXGraph,types,api,elements}.jl`
- `subset_julia_vm/src/plotting/jsxgraph.rs`
- `subset_julia_vm/src/julia/packages/mod.rs`
- `subset_julia_vm/src/repl/completions.rs`
- `subset_julia_vm/tests/fixtures/packages/packages_jsxgraph_*.jl`
- `subset_julia_vm/tests/plot_artifact_mime_tests.rs`
