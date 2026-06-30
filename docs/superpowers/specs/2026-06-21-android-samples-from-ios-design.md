# Android サンプルコードを iOS 例に合わせる設計

## 背景

`SubsetJuliaVMApp`（iOS）には 25 件の Julia コードサンプルが `Resources/Samples/` 以下で管理されている。一方、既存の Flutter モバイルアプリ（Android/iOS 両対応、`mobile/` 以下）は独自の 57 件のサンプルを `mobile/assets/samples/` で保持しており、iOS アプリのサンプル例と一致していない。

本設計では、Flutter モバイルアプリのサンプルを iOS アプリと同じ 25 件に統一し、Android ユーザーにも iOS と同等の学習・デモ用 Julia コードを提供する。

## 目標

- Flutter モバイルアプリの Julia サンプルコードを iOS アプリの 25 件と完全に一致させる
- サンプルメタデータ形式も iOS に合わせ、`folder` + `id` 形式を採用する
- Dart 側の読み込みロジックを iOS 形式に対応させる
- 既存の Flutter 独自サンプル `.jl` ファイルは削除する

## 変更対象ファイル

| ファイル/ディレクトリ | 変更内容 |
|---|---|
| `mobile/assets/samples/samples.json` | iOS 版 `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` で完全に置き換え |
| `mobile/assets/samples/beginner/*.jl` | iOS beginner サンプルで置き換え（`hello_world.jl`, `memo.jl`） |
| `mobile/assets/samples/intermediate/*.jl` | iOS intermediate サンプル 16 件で置き換え |
| `mobile/assets/samples/advanced/*.jl` | iOS advanced サンプル 7 件で置き換え |
| `mobile/lib/models/code_sample.dart` | `loadSamples()` / `fromJson()` を iOS の `folder` + `id` 形式に対応 |
| `mobile/pubspec.yaml` | アセットディレクトリ登録を確認・必要に応じて調整 |

## 削除対象

- 既存の `mobile/assets/samples/beginner/` 内の Flutter 独自 `.jl` ファイル
- 既存の `mobile/assets/samples/intermediate/` 内の Flutter 独自 `.jl` ファイル
- 既存の `mobile/assets/samples/advanced/` 内の Flutter 独自 `.jl` ファイル

## 採用する iOS サンプル一覧

`samples.json` に記載の 25 件。`.jl` ファイルは `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/<folder>/<id>.jl` からコピーする。

### beginner
- `hello_world`
- `memo`

### intermediate
- `fizzbuzz`
- `matrix_multiplication`
- `plotting_2d`
- `plotting_3d`
- `sinc_surface`
- `plots_animation`
- `barnsley_fern`
- `multiple_dispatch`
- `fibonacci`
- `is_prime`
- `structs`
- `operator_overloading`
- `modules`
- `mandelbrot_heatmap`
- `jsxgraph_demo`
- `apollonian_gasket`

### advanced
- `mandelbrot_set`
- `coprime_pi_estimation`
- `primes_package`
- `symbolics_package`
- `meta_parse_eval`
- `user_defined_macros`
- `matrix_decompositions`
- `distributions_package`

## Dart 側の変更詳細

### `CodeSample.fromJson`

- `id` を `json['id']` から取得（必須）
- `folder` を `json['folder']` から取得（必須）
- `name`, `category`, `description`, `difficulty`, `tags` は既存のマッピングを維持

### `CodeSample.loadSamples`

現在の読み込みパス:
```
assets/samples/samples.json
assets/samples/$difficulty/$filename
```

変更後の読み込みパス:
```
assets/samples/samples.json
assets/samples/<folder>/<id>.jl
```

つまり `filename` キーを参照するのをやめ、`folder` + `id` を使って `.jl` ファイルを特定する。

### フォールバックサンプル

JSON または `.jl` 読み込み失敗時の `fallbackSamples` も、iOS の beginner サンプル内容に更新する。

## データフロー

```
HomeView._initializeApp()
  → EditorState.initialize()
    → CodeSample.loadSamples()
      → rootBundle.loadString('assets/samples/samples.json')
      → json.decode()
      → 各エントリについて rootBundle.loadString('assets/samples/<folder>/<id>.jl')
      → List<CodeSample> として返却
    → EditorState.samples に設定
  → EditorView ドロップダウンに表示
```

## エラー処理

- JSON または `.jl` ファイルの読み込みに失敗した場合、既存の try/catch で `fallbackSamples` を返す動作を維持
- 個別の `.jl` ファイルが見つからない場合はスキップ（既存ロジックを維持）
- フォールバック内容を iOS の beginner サンプルに更新し、ユーザーが最低限の例を見られるようにする

## 検証

- `flutter build apk` が成功すること
- `flutter build appbundle` が成功すること（可能であれば）
- `flutter test` が成功すること（テストが存在する場合）
- アプリ起動後、サンプルドロップダウンに 25 件が表示されること
- 各サンプルを選択した際に対応する Julia コードがエディタに読み込まれること

## 実装後の影響

- Flutter モバイルアプリのサンプルが iOS と一致し、ドキュメント・メンテナンスの対象が一元化される
- 今後 iOS サンプルが更新された場合、`mobile/assets/samples/` へ手動で同期する運用とする
- 既存の Flutter 独自サンプルは削除されるため、それらに依存していたテストやドキュメントがあれば合わせて修正する
