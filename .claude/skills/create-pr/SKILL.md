---
name: create-pr
description: コードを論理単位でコミットしプルリクエストを作成して．`gh` コマンドを使うこと
allowed-tools: Bash(gh:*), Bash(git:*), Read, Glob, Grep
---

# PR 作成スキル

コードを論理単位でコミットしプルリクエストを作成する。`gh` コマンドを使うこと。

## 注意

- 英語でメッセージを書いて
- main, master ブランチにコミットしてはいけません。必ずブランチを作成してください
- Before creating PR: Verify README.md is accurate (project structure, examples).
- auto merge が有効なリポジトリでは auto merge を有効にして。

## 完了後

作業が終わったら `git checkout main && git pull origin main` で main ブランチに戻ってください。
