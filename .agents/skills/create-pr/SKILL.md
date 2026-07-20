---
name: create-pr
description: コミットして PR を作成・マージするよう依頼されたとき (/create-pr)。軽量フロー。多ファイル・多関心の大きな変更の仕上げには sjulia-finish-branch を使う。
allowed-tools: Bash(gh:*), Bash(git:*), Bash(cargo:*), Bash(rustfmt:*), Read, Glob, Grep
---

# PR 作成スキル

コードを論理単位でコミットし、`gh` で PR を作成して main へマージする軽量フロー。

## 手順(この順番で実行する)

```bash
# 0. 現在地の確認 — 共有 worktree では他エージェントが HEAD を切り替えることがある
git branch --show-current
#    → main なら: git pull && git checkout -b <type>/<topic>   (feat/ fix/ docs/ chore/)
#    → 自分の作業ブランチなら: そのまま続行
#    → 見覚えのないブランチなら: STOP。自分のブランチに戻ってから続行(他人の ref は触らない)

# 1. 差分の確認と論理単位の特定(分割規約: sjulia-logical-commits)
git status && git diff --stat

# 2. Rust を触った場合のみ: pre-PR ゲート
cargo fmt --check                        # clippy 通過 ≠ fmt-clean。失敗したら
rustfmt --edition 2021 <自分が触った.rsのみ>  # 他人のファイルは整形しない
cargo clippy --all-targets -- -D warnings

# 3. 論理単位ごとにコミット(コミット直前にも git branch --show-current を再確認)
git add <named files>                    # ファイル名を明示。git add . / -A は禁止
git commit -m "..."

# 4. push → draft PR → lead certification/merge
git push -u origin <branch>
gh pr create --draft --title "..." --body "..."  # Summary / Test plan / Linked Issue #NNNN
bash scripts/premerge_gate.sh --pr <N>    # lead only: ready transition + regular merge
```

## 禁止事項

- **main / master への直接コミット禁止** — 必ずブランチを作成する
- **実装 agent は ready 化・merge 禁止** — review / gate が未完の間は draft を維持し、
  lead の `premerge_gate.sh --pr <N>` に exact-main/exact-head 認証と merge を委ねる
- **`git add .` / `git add -A` 禁止** — 自分が変更していないファイルが working
  tree にあってもコミットに含めない
- **`git stash` 禁止** — stash は全セッション共有(他エージェントの WIP を pop
  してしまう)。退避が必要なら一時ブランチにコミットする
- 他人の ref / 自分が触っていないファイルを checkout・reset しない

## 注意

- コミットメッセージ・PR タイトル・本文は英語で書く
- 関連 Issue があれば PR 本文に `#NNNN` でリンクする(Discovery Rule で起票した
  Issue は必ず参照する)
- マージが競合したら `git fetch origin main && git rebase origin/main`
  (詰まったら `git rebase --autostash origin/main`)で解消し
  `--force-with-lease` で push してから再マージ
- Post-PR: 機能・バグ修正の PR なら `docs/vm/` の STATUS.md / DONE.md /
  UNIMPLEMENTED.md 更新を忘れない(詳細: `sjulia-dev/pr-flow.md`)

## 完了後

1. `git checkout main && git pull origin main` で main に戻る
2. マージ済みローカルブランチを `git branch -d <branch>` で削除する
3. ポストモーテムを実施する(`sjulia-postmortem`): memory 記録 +
   予防/フォローアップ Issue 起票
