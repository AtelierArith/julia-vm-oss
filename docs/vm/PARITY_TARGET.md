# PARITY_TARGET — どの Julia と一致すべきか (Issue #8644 / #8666)

Status: normative. sjulia の「出力が公式 Julia と一致する」という正義における
**比較対象バージョンの single source** を定める。ここに書かれた対象と、実際に
比較実行される julia バイナリ・`julia/` サブモジュールが食い違う場合、それは
修正すべきドリフトであり、Issue 化の対象である。

## パリティ対象バージョン

- **対象系列: Julia 1.12.x の最新 patch リリース**(2026-07-02 時点で v1.12.6)。
- 機械可読な正準値はリポジトリルートの **`PARITY_TARGET` ファイル**
  (先頭の非コメント行が `MAJOR.MINOR` 系列)。検証スクリプト・CI はこの
  ファイルを読む(#8667)。人間向けの方針・背景は本ドキュメント。
- patch リリース(1.12.6 → 1.12.7 など)への追従は随時。minor 系列の乗り換え
  (1.12 → 1.13 など)は Milestone を切って計画的に行う(#8668 のチェックリスト
  参照)。

## 正準表現の三層と各自の役割

| 層 | 正準 | 役割 |
|---|---|---|
| `PARITY_TARGET`(ルートファイル) | 対象系列 `1.12` | 機械可読な single source。スクリプト・CI が照合に使う |
| `julia/` サブモジュール | 対象系列の最新 patch のリリースタグ commit | 設計原則 1「実装で迷ったら upstream を読む」の参照先。**ソースコードとしての正準表現** |
| 開発者/CI の `julia` バイナリ | 対象系列の最新 patch | `fixture_julia_parity.sh` 等の**実行比較**の相手 |

三層は常に同じ系列を指していなければならない。サブモジュールと
`PARITY_TARGET` は**同一 PR で**更新する(手順: `docs/vm/CHECKLISTS.md` の
「julia/ サブモジュール更新」チェックリスト、#8668)。

## `version.jl` の `VERSION` との関係(意図的な非互換)

`subset_julia_vm/src/julia/base/version.jl` の `const VERSION = VersionNumber(...)`
は **SubsetJuliaVM 自身のバージョン**(例: v"0.9.5")であり、パリティ対象の
Julia バージョンでは**ない**。upstream では `VERSION` が Julia 本体のバージョン
(v"1.12.6")を返すため、これは意図的な divergence である:

- sjulia は Julia 1.12.6 の再実装ではなく「strict subset」なので、`VERSION` で
  自身を名乗る。
- 帰結として、`VERSION >= v"1.9"` のようにバージョンで機能分岐する upstream
  由来のコードはそのままでは動かない。移植時はこの分岐を対象系列
  (1.12.x)の側に固定して取り込む。
- fixture が `VERSION` の値そのものを比較することは禁止(パリティ検証が
  恒久的に失敗するため)。

## 何を「一致」とみなすか

- fixture の expected 出力・エラーメッセージ・表示形式(`show` / `repr`)・
  プロモーション規則は、**対象系列の最新 patch** の `julia --startup-file=no`
  の出力を正とする。
- upstream の系列内 patch で挙動が変わった場合(稀)は、新しい patch の挙動を
  正として expected を更新し、追従 Issue を残す。
- upstream の **次期系列**(1.13+/DEV)で入った挙動変更は、対象系列に来るまで
  取り込まない。`julia/` サブモジュールが DEV commit を指しているときに DEV の
  挙動を移植してしまうのが典型的な事故(下記ドリフト 1)。

## 既知ドリフト(2026-07-02 時点)

| # | ドリフト | 追跡 |
|---|---|---|
| 1 | `julia/` サブモジュール: **解消** ✅ — v1.12.6 (`15346901f0`) へ更新(#8668 初回運用で実施) |
| 2 | `InexactError` のフィールド構成: **解消** ✅ — upstream 1.12 系の `(func::Symbol, args)` に追従し、`fieldnames` / `e.args` が一致(#8732) |
| 3 | `VERSION` 定数が v"0.9.x"(SubsetJuliaVM 自身)を返す。上記のとおり**意図的な divergence** として本ドキュメントで明文化 | 本ドキュメント(追跡 Issue 不要) |

解消済み(履歴は git / Issue 参照): CI `setup-julia` の `version: '1'` 浮動と、
パリティスクリプトのバージョン照合欠如は #8667 で解消 —
`scripts/parity_julia_version.sh` が `PARITY_TARGET` と照合し(警告 /
juliaup `julia +X.Y` 自動選択 / `--strict`・`SJULIA_PARITY_STRICT=1` で
exit 非ゼロ)、CI は `PARITY_TARGET` から setup-julia のバージョンを読む。

新しいドリフトを見つけたら: `bug` / `unsupported-feature` Issue を作成し
(`sjulia-report-gap` skill)、この表に追記する。解消されたら行を削除してよい
(履歴は git にある)。

## 関連

- 親 Issue: #8644(パリティ対象バージョンの明文化とサブモジュール更新プロセス)
- #8666(本ドキュメント)/ #8667(スクリプト・CI のバージョン照合)/
  #8668(サブモジュール更新チェックリストと初回運用)
- `REPOSITORY_RULES.md`「Upstream Parity Target」節(規範のポインタ)
- `docs/vm/CHECKLISTS.md`「julia/ サブモジュール更新」(#8668 で追加)
