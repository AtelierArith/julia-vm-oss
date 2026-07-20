---
name: report-issue
description: 作業中に発見したバグや追加機能の必要性を GitHub Issue として起票するスキル。upstream julia では動くが sjulia で動かない / 結果が違うギャップ、バグ修正後の再発防止策を、最小 MWE と julia-vs-sjulia 出力比較付きで起票する。
allowed-tools: Bash(gh:*), Bash(julia:*), Bash(cargo:*), Bash(./target/release/sjulia:*), Bash(target/release/sjulia:*), Read, Glob, Grep
---

# Report Issue Skill

作業中に発見したバグや追加機能の必要性を GitHub Issue として起票するスキル。
このリポジトリ (SubsetJuliaVM / AtelierArith/ailujsoi) の **Issue-Driven /
Unsupported-Feature Discovery Rule**（`AGENTS.md`）を強制する。回避策を入れる前に、
まず Issue を立てるのが原則。

## 使い方

```
/report-issue bug          # sjulia が実行できるが結果が誤り/クラッシュするバグを報告
/report-issue feature      # upstream julia で動くが sjulia が実行できない構文/機能を報告
/report-issue prevention   # バグ修正後の再発防止策を提案・起票
```

## ラベル判定（先に決める）

引数 `bug` / `feature` どちらかを選ぶ前に、sjulia の挙動で判定する。
判断に迷ったら、sjulia が**実行を拒否**するなら `unsupported-feature`、
**実行はするが結果が誤り/クラッシュ**するなら `bug`。

| sjulia の挙動 | 引数 | ラベル |
|--------------|------|--------|
| 構文エラー / "unsupported"・"not implemented" 実行時エラー / 正当な構文への MethodError など**実行できない** | `feature` | `unsupported-feature` |
| **実行はするが出力が upstream と異なる**、またはクラッシュ・既存エラーに当たる | `bug` | `bug` |
| 再発防止策の提案 | `prevention` | `prevention` |

関連: 作業中に偶発的にギャップを踏んだ場合の「即時起票・回避禁止」フローは
`sjulia-report-gap` スキル、修正後の防止策一般化は `sjulia-bug-prevention` スキル
（いずれも `.agents/skills/`）。

## 共通の前処理（すべての引数で必須）

### 1. 重複 Issue を確認する

起票前に既存 Issue と重複していないか必ず検索する。

```bash
gh issue list --state all --search "<キーワード>" --limit 30
# ラベルで絞る場合
gh issue list --state open --label bug --search "<キーワード>" --limit 30
```

重複があればそちらにコメント追記・参照で済ませ、新規作成しない。

### 2. 最小 MWE を upstream julia と sjulia の両方で実行して出力を採取

`bug` / `feature` では、最小再現コードを **upstream julia で動く** ことと
**sjulia で失敗/相違する** ことを実際に走らせて確認し、両者の出力をそのまま Issue に貼る。

```bash
julia --startup-file=no --history-file=no /path/to/mwe.jl   # upstream: 成功するはず
cargo build --release -p subset_julia_vm --bin sjulia --features repl   # base/ を触ったら更新
timeout 180 ./target/release/sjulia /path/to/mwe.jl         # sjulia: 失敗/相違するはず
```

`julia` が PATH に無い場合はその旨を本文に書いて、起票はブロックしない。

## 実行手順

### 引数が "feature"（unsupported-feature）の場合

upstream julia で動くが sjulia が実行できない構文/機能。

1. 共通前処理（重複チェック + julia/sjulia 両方の出力採取）を実施。
2. 以下で Issue を作成。**julia-vs-sjulia 出力比較表は必須**（Discovery Rule）。

```bash
gh issue create --title "Unsupported: <機能の説明>" --body "$(cat <<'EOF'
## Summary

<upstream julia では動くが sjulia が実行できない機能の説明>

## MWE

```julia
# upstream julia で動き、sjulia で失敗する最小再現コード
```

## Output comparison

| Interpreter | Result |
|-------------|--------|
| `julia`     | <期待される出力 / 成功> |
| `sjulia`    | <parse error / unsupported error / MethodError など> |

## Workaround (if any)

```julia
<回避策のコード（あれば）。回避策を入れる場合は docs/vm/WORKAROUNDS.md にも登録すること>
```

## Impact

- [ ] Blocks feature implementation
- [ ] Requires code modification workaround
- [ ] Minor inconvenience

## Context

- Found while: <何をしていて見つけたか>
- sjulia build: `cargo build --release -p subset_julia_vm --bin sjulia --features repl`

## Related Files

- <関連ファイルのパス>
EOF
)" --label "unsupported-feature"
```

### 引数が "bug"（実行はするが結果が誤り）の場合

1. 共通前処理（重複チェック + julia/sjulia 両方の出力採取）を実施。
2. 以下で Issue を作成。誤った出力と期待値の対比を明示する。

```bash
gh issue create --title "Bug: <概要>" --body "$(cat <<'EOF'
## Summary

<バグの説明>

## MWE

```julia
# upstream julia では正しく動き、sjulia で結果が誤る/クラッシュする最小再現コード
```

## Output comparison

| Interpreter | Result |
|-------------|--------|
| `julia`     | <期待される正しい出力> |
| `sjulia`    | <実際の誤った出力 / クラッシュ> |

## Error Message

```
<エラーメッセージ（あれば）>
```

## Expected Behavior

<期待される動作>

## Workarounds

<回避策があれば記載。入れる場合は docs/vm/WORKAROUNDS.md にも登録>

## Related Files

- <関連ファイルのパス>

## Suspected Cause

<原因の推測>
EOF
)" --label "bug"
```

### 引数が "prevention" の場合

1. 直前に修正したバグについて確認。
2. 以下の観点で再発防止策を提案:
   - テストの追加（どのような fixture / 単体テストケースが必要か）
   - コードレビューのチェックポイント
   - ドキュメントの改善
   - 静的解析やリンター（`scripts/check_*.sh` / clippy）の活用
   - 設計の改善
3. 提案内容を Issue として起票:

```bash
gh issue create --title "Prevention: <バグの概要> の再発防止" --body "$(cat <<'EOF'
## 関連バグ

- #<バグの Issue 番号>

## バグの原因分析

<なぜこのバグが発生したかの分析>

## 再発防止策

### 1. テストの追加

<追加すべき fixture / テストケース>

```julia
# 追加すべきテストコード例
```

### 2. コードレビューチェックポイント

- [ ] <チェック項目1>
- [ ] <チェック項目2>

### 3. ドキュメント改善

<改善すべきドキュメント（docs/vm/ など）>

### 4. 静的解析・リンター

<活用可能なツールや設定（scripts/check_*.sh, clippy など）>

### 5. 設計改善

<長期的な設計改善の提案>

## 優先度

- [ ] 高: 即座に対応が必要
- [ ] 中: 次のスプリントで対応
- [ ] 低: バックログに追加

## 実装タスク

- [ ] <タスク1>
- [ ] <タスク2>
EOF
)" --label "prevention"
```

## 注意事項

- **起票前に必ず重複確認**（共通前処理 1）。既存 Issue があればそちらに集約する。
- `bug` / `feature` は**最小限の MWE** を含め、**julia-vs-sjulia の出力比較表**を必ず付ける。
- ラベルは judgement 表に従う（実行できない=`unsupported-feature`、誤出力=`bug`）。
- Issue を立てる前に回避策を入れない。回避策が不可避なら Issue 番号を
  `// Workaround: ... (Issue #NNNN)` コメントに残し、`docs/vm/WORKAROUNDS.md` に登録して
  `bash scripts/check_workarounds_documented.sh` と `bash scripts/check_workarounds_sync.sh` を通す
  （詳細は `sjulia-document-workaround` スキル）。
- 再発防止策は具体的かつ実行可能なものにする。
