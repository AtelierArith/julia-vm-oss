# NORTH_STAR — 「良い sjulia」を継続計測可能な数値に集約する

Issue #8680 / #8681 / #8682. Status: normative(指標の定義・運用の single
source)。時系列データの置き場所は `benchmarks/results/north_star/`。

## 目的

「良い sjulia」の判断基準を 7±2 個の継続計測可能な指標に集約し、

1. **投資判断**を数字で議論できるようにする(例: Milestone #60 構造負債 vs
   パッケージ対応)
2. **趨勢レベルの後退**(パリティ率低下・起動時間悪化)を受動的に検知する
3. **Milestone / 四半期レビューの完了判定**を主観でなく before/after 比較で行う

upstream Julia の BaseBenchmarks + Nanosoldier(性能回帰追跡)、
PkgEval(エコシステム動作率)に相当する仕組みの sjulia 版である。

## 指標追加・変更の規約

- **7±2 の上限**: 指標は最大 9 個。**1 増やすなら 1 落とす**(または既存 2 個を
  統合する)。落とせない場合は追加しない。ダッシュボードが 10 個を超えた時点で
  誰も見なくなる、が本規約の根拠。
- 各指標は必ず **定義 / 計測コマンド / 単位 / データ源 / 更新頻度 /
  改善方向(↑ or ↓) / 動いたら何を意味するか** の 7 点セットで本ドキュメントに
  記載する。7 点が埋まらない指標は「予約」節に置き、計測対象に含めない。
- 指標の定義変更(計測コマンド・分母の変更)は**時系列の断絶**なので、変更した
  PR で `benchmarks/results/north_star/` の次回レコードに「定義変更」注記を必ず
  残す。旧レコードは書き換えない。
- 時刻・所要時間系の指標は **CI 計測を正**とする。ローカル計測は暫定値
  (provisional)として記録し、ambient load を注記する。件数・比率系
  (通過率・カバレッジ・負債件数)は環境負荷に頑健なのでローカル計測でも正。

### NS-1 / NS-2 単調ラチェット運用規約(Issue #9129)

**NS-1(パリティ fixture 通過率)と NS-2(upstream コーパスパース成功率)は
単調非減少ラチェットとして扱う。**「正しさ・カバレッジ」を測る指標であり、
下降は原則として回帰だからである。運用規則:

- **下げる変更は明示的な理由 + Issue を要求する。** NS-1/NS-2 を下げる PR は
  本文に「なぜ下げるか」と追跡 Issue を書く。理由なしの下降は**マージ前に調査**
  (方向が明確な指標の後退 = 即調査、本ドキュメント末尾「後退の扱い」)。
- **分母を必ず見る。** fixture / コーパス総数の急増を伴う率の低下は「新機能の
  パリティ未達」であり回帰とは区別する(NS-1 の定義参照)。同一コーパス commit
  内での率低下のみをパーサ回帰とみなす(NS-2 は `julia/` サブモジュール commit を
  必ず併記)。真の回帰と分母変動を混同しないため、レコードには常に分子/分母の
  両方を残す。
- **機械検知と自動起票。** `scripts/north_star_report.sh --check-regression` が
  前回 TSV 行と比較し、NS-1 パリティ率低下・NS-2 パース率低下(同一コーパス
  commit のとき)・NS-7 workaround 増で **exit 3**。nightly `north-star` job は
  この exit で落ち、失敗通知 Issue を自動起票する(#8682/#8633)。この nightly
  ラチェットは**運用ルールとして正式**であり、下降検知を握り潰さない。
- **大型 PR は事前影響を宣言する。** NS 系に触れる大型 PR は PR テンプレの
  「NS 影響」欄に予想影響(NS-1〜NS-7)を書く。実測が予想と食い違ったら
  マージ前に理由を addendum する。

## 指標一覧(初版セット: 2026-07-02 確定)

| # | 指標 | 単位 | 改善方向 | 更新頻度 | データ源 |
|---|------|------|---------|---------|----------|
| NS-1 | パリティ fixture 通過率 | % | ↑ | nightly | `scripts/fixture_julia_parity.sh` 全 fixture 掃引 |
| NS-2 | upstream コーパスパース成功率 | % | ↑ | nightly | `scripts/parser_corpus_sweep.sh` (#8614) |
| NS-3 | iOS サンプル動作率 | % + infra % | sample ↑ / infra ↓ | **手動・月次**(macOS) | `scripts/ios_samples_e2e.py` report |
| NS-4 | 代表ベンチ upstream 比 | 倍率 (×) | ↓ | nightly(CI 計測) | `benchmarks/run_benchmarks.sh` |
| NS-5 | cold 起動時間(CLI キャッシュ埋め込み) | 秒 | ↓ | nightly(CI 計測) | キャッシュ埋め込み `sjulia` の空プログラム実行 |
| NS-6 | full nextest 通過数・所要時間 | 件 / 分 | 通過↑ / 時間↓ | main push 毎(CI) | `main-full.yml`(`scripts/test_with_cache.sh`) |
| NS-7 | 負債バロメータ(workaround 数 + 構造負債棚卸し) | 件 | ↓ | nightly | `docs/vm/WORKAROUNDS.md` + `scripts/check_structural_debt_inventory.sh` |

### NS-1 パリティ fixture 通過率

- **定義**: `subset_julia_vm/tests/fixtures/**/*.jl` の全 fixture のうち、
  `scripts/fixture_julia_parity.sh <fixture>` が exit 0(sjulia と upstream
  julia の pass/fail 集計が一致)になる割合。比較相手の julia は
  `PARITY_TARGET` 系列(`docs/vm/PARITY_TARGET.md`)。
- **計測コマンド**:

  ```bash
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
  find subset_julia_vm/tests/fixtures -name '*.jl' | sort | \
    xargs -P "$(nproc)" -I{} bash -c \
      'bash scripts/fixture_julia_parity.sh "{}" >/dev/null 2>&1 && echo "PASS {}" || echo "FAIL {}"' \
    | tee /tmp/parity_sweep.txt | grep -c '^PASS'
  ```

  (#8682 以降は `scripts/north_star_report.sh` に委譲。)
- **単位**: %(PASS fixture 数 / 全 fixture 数)。分母(全 fixture 数)も併記する。
- **改善方向**: ↑。100% にはならない — 意図的に upstream と挙動が異なる
  fixture(`@assert` ベースの sjulia 固有仕様、`fixture_julia_parity.sh` の
  SCOPE 注記参照)は恒常的に FAIL 側に入る。**絶対値でなく趨勢を見る**。
- **動いたら**: 低下 = upstream 一致性の後退(正しさの回帰)。上昇 = パリティ
  修正またはパリティ準拠の新 fixture 追加。fixture 総数の急増を伴う率の低下は
  「新機能のパリティ未達」であり後退とは区別する(分母を必ず見る)。

### NS-2 upstream コーパスパース成功率

- **定義**: `julia/` サブモジュール(`base/` `stdlib/` `test/` の `*.jl`)を
  sjulia パーサでパースして(パースのみ、lowering/VM 実行なし)、エラー 0 件で
  パースできたファイルの割合。詳細と初回ベースラインは
  `docs/vm/PARSER_CORPUS_BASELINE.md`(#8614 / #8635)。
- **計測コマンド**: `bash scripts/parser_corpus_sweep.sh`(stderr にサマリ、
  TSV は `target/parser_corpus/sweep.tsv`)。
- **単位**: %(クリーンにパースできたファイル数 / 掃引ファイル数)。パニック数
  (常に 0 であるべき)も併記する。
- **改善方向**: ↑。
- **注意**: サブモジュール commit が変わると分母・分子とも変わる。時系列
  レコードには**必ずサブモジュール commit を併記**し、commit が異なる区間の
  率は直接比較しない(PARSER_CORPUS_BASELINE.md の 2f3128cdb ベースラインは
  1.14-DEV コーパス、以降は v1.12.6 コーパス)。
- **動いたら**: 低下 = パーサ回帰(同一 commit 内なら即調査)。上昇 = 構文
  カバレッジの拡大。パニック > 0 は率と無関係に個別 `bug` Issue(#8635 規約)。

### NS-3 iOS サンプル動作率

- **定義**: `SubsetJuliaVMApp` の `samples.json` 全サンプルを
  `scripts/ios_samples_e2e.py` で実行し、`sample_pass` と分類された割合。
  `infra_failure` は分母から外して別率で記録する。AX wedge / REPL 未リセット /
  relaunch 漏れはサンプル失敗ではなくハーネス失敗として扱う。ミニ PkgEval に相当。
- **計測コマンド**(macOS + シミュレータ必須):

  ```bash
  ./build.sh   # Rust/VM 変更を含める場合は xcframework 再構築
  uv run scripts/ios_samples_e2e.py --out-dir /tmp/e2e --launch
  python3 scripts/ios_e2e_report.py --summary /tmp/e2e/report.txt
  bash scripts/north_star_report.sh --skip ns1,ns2,ns4,ns5,ns6 \
    --ns3-report /tmp/e2e/report.txt
  ```

- **単位**: sample %(`sample_pass / (sample_pass + sample_fail)`) と
  infra %(`infra_failure / 全試行`)。`sample_fail` と `infra_failure` は必ず
  分けて併記する。
- **更新頻度**: **手動・月次**。macOS + iOS シミュレータ + Accessibility 権限が
  必要で Linux CI では実行できないため、nightly には入れない。
  `.github/workflows/platform-builds.yml` の `workflow_dispatch` で
  `run_ios_samples_e2e=true` を指定すると、macOS runner 上で手動月次掃引し、
  screenshot/report/North Star record を artifact に残す。**黙って欠測に
  しない** — 月次レコードに載らない月は「未計測」と明記する(#8682)。
- **改善方向**: sample rate ↑、infra rate ↓。
- **動いたら**: 低下 = ユーザー可視の後退(サンプルは公開デモそのもの)。
  infra rate の上昇は E2E ハーネス側の問題(AX wedge / Reset / relaunch)として
  サンプル互換性とは別に調査する。
- **最新計測 (2026-07-07, commit 01c538d4d, iOS simulator iPad A16 / iOS 26.5)**:
  **sample 100.0% (38/38), infra 0.0%**(method: `xctest-editor-vmbridge`)。
  macOS ホストの AX/CGEvent ドライバが loginwindow でブロックされ正準の
  `ios_samples_e2e.py` が実行不可だったため、全 `samples.json` サンプルを Editor-mode の
  実 FFI(`VMBridge.execute` = Editor の Run ボタンと同じ compile-and-run 経路)へ通す
  XCTest スイープ `NS3SampleSweepTests` で代替計測した(#9096 項目4、AX-blocked→XCTest
  代替)。この代替は各サンプルを必ず実行するので **infra failure が構造的に 0** になり、
  AX ハーネスの best-effort な出力エラー検知は含まない(pass = 「VM エラーなく実行完了」)。
  AX が利用可能な環境では次回、正準の `ios_samples_e2e.py` で再計測して差分を確認する。

### NS-4 代表ベンチ upstream 比

- **定義**: 代表ベンチマーク(`benchmarks/calc_pi_benchmark.jl`)の
  `sjulia_embedded_cli` 中央値実時間 ÷ `julia_cli` 中央値実時間(#8458 の
  3 層計測の上位 2 層)。`sjulia_vm_bytecode` 層の倍率も併記する。
- **計測コマンド**: `RUNS=5 ./benchmarks/run_benchmarks.sh`(2 段キャッシュ
  埋め込みビルドを内包。結果は `benchmarks/results/reproducible_*/summary.md`)。
- **単位**: 倍率(1.0 = upstream と同速、10× = 10 倍遅い)。
- **更新頻度**: nightly(**CI 計測を正**とする。ローカル値は provisional)。
- **改善方向**: ↓。
- **動いたら**: 上昇 = VM 実行性能の回帰(設計原則 6「VM Performance
  Priority」への逆行)。ノイズ余裕として **前回比 +20% を超えたら**趨勢後退と
  みなす(#8682 の機械判定閾値)。改善したら
  `benchmarks/results/` に個別ベンチレポートを残す既存文化はそのまま。
- **Criterion 単機能回帰ゲート** (Issue #9003): NS-4 の upstream 比計測とは
  別に、dispatch / broadcast / string / Int128 / SSA / register-VM を
  カバーする 8 本の Criterion ベンチを nightly ゲートでチェックする。
  しきい値: actual ≤ baseline × 1.20(同じ +20% ノイズ余裕)。
  ベースラインは `benchmarks/baselines/multi_bench_nightly_thresholds.json`、
  結果は `benchmarks/results/perf_gate.tsv` に蓄積される。
  詳細は `nightly-gates.yml` の `perf-gate` ジョブを参照。

### NS-5 cold 起動時間(CLI キャッシュ埋め込み)

- **定義**: プレリュード/Base キャッシュを埋め込んだ `target/release/sjulia`
  (CLAUDE.md の **2 段ビルド手順**の 2 回目のビルド産物であること)で
  空プログラム(`println("ok")` 1 行)を実行したときのプロセス実時間の中央値
  (11 回計測、初回 1 回は捨てる)。iOS 実機/シミュレータの cold 起動の
  Linux で測れる代理指標(prelude/Base ロード + VM 初期化コストを支配項として
  共有する)。
- **計測コマンド**:

  ```bash
  # CLAUDE.md「Precompiled cache build」の 2 段ビルド後:
  f=$(mktemp --suffix=.jl); echo 'println("ok")' > "$f"
  for i in $(seq 12); do /usr/bin/time -p ./target/release/sjulia "$f" 2>&1 >/dev/null | awk '/^real/{print $2}'; done
  # 初回を捨てて中央値を取る
  ```

- **単位**: 秒。
- **更新頻度**: nightly(**CI 計測を正**とする)。**iOS アプリ実機 cold 起動**は
  macOS 依存のため NS-3 と同じく**手動・月次**で別行として記録する
  (未計測の月は「未計測」と明記)。
- **改善方向**: ↓。
- **動いたら**: 上昇 = キャッシュロード or VM 初期化の肥大(体感の入口の悪化)。
  キャッシュスキーマ変更(CACHE_VERSION バンプ)直後の一時上昇は既知要因として
  注記する。ノイズ余裕: 前回比 +20% 超で趨勢後退(#8682)。

### NS-6 full nextest 通過数・所要時間

- **定義**: `main-full.yml`(main への push 毎に
  `scripts/test_with_cache.sh --no-fail-fast` = 埋め込み Base キャッシュ付き
  `cargo nextest run --release`)の (a) テスト通過数/失敗数、(b) ジョブ実時間
  (分)。
- **計測コマンド**: CI が正。手動確認は
  `gh run list --workflow main-full.yml --json conclusion,createdAt,updatedAt`。
  ローカルでは `timeout 1800 cargo nextest run --release --no-fail-fast` の
  末尾サマリ(provisional)。
- **単位**: 件(passed / failed / skipped)、分。
- **更新頻度**: main push 毎(CI)。north_star レコードには nightly 時点の
  直近 main-full 実績を転記する。
- **改善方向**: 通過数 ↑(失敗 0 が正常)、所要時間 ↓。
- **動いたら**: 失敗 > 0 は main-full が Issue を自動起票する(既存機構)。
  所要時間の趨勢増は開発速度の悪化 — テスト追加による自然増と、ビルド/実行の
  回帰を分けて解釈する(テスト数も併記するのはそのため)。

### NS-7 負債バロメータ

- **定義**: (a) open workaround 数 = `docs/vm/WORKAROUNDS.md` Summary Table の
  W-ID 行数(Resolved は含まない)、(b) 構造負債棚卸し =
  `scripts/check_structural_debt_inventory.sh` が出力する各カテゴリの現在値、
  (c) **触点数バロメータ**(Issue #10817)= 融合命令の組合せ増殖を表す
  `Instr` / `TypedLoopOp` の variant 数、(d) **構文あたりの意味論実装系統数**
  (Issue #10817)= 1 つの Julia 構文の意味論を独自に実装している経路の数
  (静的コンパイラ / runtime specializer / typed-loop 認識器・実行器 / AoT /
  レジスタ VM のうちいくつが独立実装かの手動棚卸し)。(単位が混在するため
  **合算せず**カテゴリ別に趨勢を見る)。
  Issue #10817 の結論(親分析 #10452)は「workspace Rust ~51万行は目標設定に
  対して防御可能だが、成長の**質**(1 機能あたりの触点数)は LOC と別に
  ラチェットすべき」であり、(c)(d) は LOC ではなく触点数・実装系統数そのもの
  を対象にする(a)(b) の直接の拡張である。
- **計測コマンド**:

  ```bash
  grep -cE '^\| W-[0-9]+' docs/vm/WORKAROUNDS.md   # (a) Summary Table の open W-ID 数
  bash scripts/check_structural_debt_inventory.sh  # (b) Current inventory 一覧
  bash scripts/loc_report.sh                       # (c) Instr / TypedLoopOp variant 数(+ 領域別 LOC 参考値)
  bash scripts/loc_report.sh --variants-only        # (c) 同上、variant 数のみの高速パス — nightly はこちらを使う(Issue #10899)
  ```

  (c) は Issue #10899 により `scripts/north_star_report.sh` の nightly 収集に
  組み込み済み(`--variants-only` fast path、`Instr`/`TypedLoopOp` の
  enum-variant 抽出のみ実行しフル LOC 掃引はスキップする)。dated markdown
  レコードと `north_star.tsv` の `ns7c_instr_variants` /
  `ns7c_typed_loop_variants` 列に毎回記録されるが、**`--check-regression` の
  ハード回帰判定には含めない**(下記「動いたら」参照)。フル出力
  (LOC 内訳込み)は引き続き四半期レビューで手動実行する。

  (d) は機械計測できない(「同じ意味論を独立実装しているか」の判定は構造的
  類似性の解釈を要する)。四半期レビュー時に手動で棚卸しし、(c) の
  `scripts/loc_report.sh` スナップショットと同じ STATUS.md サブセクションに
  記録する。現在値(Issue #10817 実測、2026-07-12、代表構文ベース)は
  「3〜5 実装系統/構文、新構文追加あたり 6〜8 箇所の登録」— 代表例は
  #10452 の class 3(意味論の多重実装)、#10814(typed-loop bail 保護の
  denylist 拡散)、#10815(AoT differential lane 不在)。
- **単位**: 件(カテゴリ別)。(c) は variant 数、(d) は実装系統数・登録箇所数。
- **更新頻度**: nightly((a)(b)(c) — (c) は Issue #10899 以降
  `scripts/north_star_report.sh` 経由)。四半期手動((d) — `scripts/loc_report.sh`
  フル出力の推奨実行頻度に合わせる。CLAUDE.md 冒頭の Hard Rules ではなく、この
  カテゴリの運用は本ドキュメントが正)。
- **改善方向**: ↓(すべてのカテゴリ)。ただし (c)(d) は「値そのものを減らす」
  より「新機能追加のたびに増えないこと」を優先して見る — fused op の追加自体は
  計測ゲートで正当化された正当な最適化であり得るため、絶対値の減少を目的化
  しない(Issue #10817「必要」側の弁護 2 参照)。
- **動いたら**: (a)(b) は既存規約のまま。(c) の増加は融合命令の組合せ増殖
  (Issue #10817「妥当でない」側の懸念 2)を示す — 新 variant が下記の
  fused-op 前提条件(#10814 のメタデータ導出)を経由して追加されたか確認する。
  nightly レコードは (c) を毎回記録するが `--check-regression` は増加を
  exit 3 にしない(record-only; Issue #10899 決定 — fused-op 追加自体は
  正当な最適化であり得るため、増加それ自体を機械的な失敗にしない。確認は
  PR レビュー時の人間判断に委ねる)。
  (d) の増加は同一構文への並列実装系統が増えたことを意味し、#9089
  (SharedFunctionPlan)/ #10461(single semantic resolver)/ #10463
  (iterator trait algebra)の統合エピックで巻き戻すべき対象として扱う。
  Milestone「アーキテクチャ負債・本家 Julia 構造パリティ監査」の
  before/after はこの指標で判定する(Issue #10817、親分析 #10452)。
- **#8704 判定**: iOS の runaway containment (StackOverflow / cancel /
  OutOfMemory) は「負債件数」ではなくホスト安全性の二値ゲートとして扱う。
  したがって NS-7 へ新カテゴリは追加しない。継続監視は NS-3 の iOS E2E
  artifact/report と、関連する `bug` / `safety` Issue の有無で行う。

### Fused-op / `Instr`・`TypedLoopOp` variant 追加の前提条件(Issue #10814、NS-7 (c) の運用規約)

Issue #10817(本節)の「妥当でない」側の懸念 2 — 融合命令は op × 型 ×
オペランド形の組合せで variant 数が線形〜積で増える — への対応方針。新しい
`TypedLoopOp` / `Instr` variant を追加するときは、手書きの網羅 `match`
(bail 可能 op × バッファ外副作用 op の組合せを個別 op 名で拒否する
`matches!` denylist など)を1つずつ拡張するのではなく、**#10814 で着地した
命令メタデータの導出構造**(`TypedLoopOp::effects()` の `bail_capable` /
`out_of_buffer_effect` と既存の `stack_effect` / `jump_target` を exhaustive match で強制し、
安全性チェックをその分類から機械的に導出する)へ**新 variant を登録するだけ**
で済む形を先に整えること。個別 op 名を列挙する手書きガードは、新 op の作者が
自分の追加した op しか守らないという構造的な脆さを持つ(#10814 の Evidence
— #10504 が既存 bail 可能 op × 副作用 op の未ガード組合せを実例として提示)。

この規約は #10452(週次バグ根因分析、class 3「意味論の多重実装」)・#9089
(SharedFunctionPlan)・#10461(single semantic resolver)が目指す「最適化
fast path は共通の意味論解決の後段に位置し、独自に意味論を再実装しない」
原則の**命令メタデータレベルでの適用**である。実装チェックリストは
`docs/vm/CHECKLISTS.md`「Fused-op / New `Instr` and `TypedLoopOp` Variant
Checklist (Issue #10814)」を参照(VM Instruction Routing Changes #3275 の隣)。

## 予約(依存 Issue が閉じたら追加する)

追加時は冒頭の「1 増やすなら 1 落とす」規約に従う(現行 7 個 + 予約 1 個 =
8 個は 7±2 の範囲内なので、下記 1 件はそのまま追加してよい)。

- **Rust 意味論比率**(#8648): Rust 側に実装された Julia 意味論の棚卸しが
  済んだら、「Pure Julia First からの乖離度」を件数または行数比で NS-7 の
  カテゴリとして追加する。追加手順: (1) #8648 の成果物の計測コマンドを NS-7 の
  計測コマンド欄に追記、(2) `scripts/north_star_report.sh` に収集を追加、
  (3) 追加した nightly レコードに「定義変更」注記。

## 時系列レコードの置き場所と形式

- ディレクトリ: `benchmarks/results/north_star/`
- ファイル名: `YYYY-MM-DD.md`(日付入り markdown、1 計測 1 ファイル)
- 各レコードに必ず含める: 計測日、git commit、julia バイナリバージョン、
  `julia/` サブモジュール commit(NS-2 用)、計測環境(CI or ローカル +
  ambient load)、全指標の値(未計測の指標は「未計測」と明記)
- 機械比較用の 1 行 TSV(`benchmarks/results/north_star/north_star.tsv` に
  追記)は `scripts/north_star_report.sh` が生成する(#8682)。#8723 以降、
  NS-3 は `ns3_rate` に加えて `ns3_infra_rate` を記録する。

## 自動計測(`scripts/north_star_report.sh`, #8682)

- **収集**: 各指標の収集を既存スクリプトに委譲する — NS-1 =
  `fixture_julia_parity.sh` 全 fixture 掃引、NS-2 = `parser_corpus_sweep.sh`、
  NS-4 = `benchmarks/run_benchmarks.sh`、NS-5 = キャッシュ版 sjulia の空
  プログラム実行、NS-6 = `gh run list --workflow main-full.yml` の直近実績、
  NS-7 = `WORKAROUNDS.md` の W-ID 数 + `check_structural_debt_inventory.sh`
  + NS-7 (c)(`scripts/loc_report.sh --variants-only`)。
  手動実行と CI で同一の markdown + TSV を出力する。NS-3 は macOS manual
  report を `--ns3-report` で取り込み、sample rate と infra rate を分離する。
  NS-7 (c)(触点数)は **本スクリプトの nightly 収集対象に追加済み**
  (Issue #10899)— `scripts/loc_report.sh` の `--variants-only` fast path
  (フル LOC 掃引をスキップし `Instr`/`TypedLoopOp` の enum-variant 抽出だけを
  再利用する)を毎回呼び出し、dated markdown レコードと `north_star.tsv` の
  `ns7c_instr_variants` / `ns7c_typed_loop_variants` 列に記録する。ただし
  fused-op variant 増は正当な最適化でもあり得るため(Issue #10817)、
  **`--check-regression` はこの増加をハード回帰として扱わず記録のみ**
  とする — 新 variant が #10814 のメタデータ導出前提を経由したかは PR
  レビューで確認する運用(NS-7 (c)「動いたら」参照)。`--skip ns7c` で
  この収集だけ無効化できる(他指標同様の記法。計測コストは低いため通常は
  不要)。NS-7 (d)(実装系統数)は引き続き**手動棚卸しのみ**(機械計測不可
  — 「同じ意味論を独立実装しているか」の判定は構造的類似性の解釈を要する
  ため)。本スクリプトは (d) について STATUS.md への手動記録を促す
  pointer 行を出力するだけで、値自体は算出しない。四半期レビュー時に
  `scripts/loc_report.sh`(フル出力、LOC 内訳込み)を手動実行し、
  STATUS.md のスナップショットに記録する運用は変わらない。
- **手動実行**: `bash scripts/north_star_report.sh`(全指標)。共有ホストで
  時間系がノイズを拾う場合は `--skip ns4,ns5,ns6` として件数系のみ取り、
  時間系は CI に委ねる。iOS の NS-3 は macOS 実測 report を
  `--ns3-report /path/to/report.txt` で転記する(黙って欠測にしない)。
  旧形式の `--ns3 PASS/TOTAL` は残すが infra rate が `n/a` になるため、新規
  レコードでは `--ns3-report` を使う。
- **趨勢後退の機械判定**: `--check-regression` で前回 TSV 行と比較し、方向が
  明確な指標(NS-1 パリティ率低下、NS-2 パース率低下(同一コーパス commit の
  ときのみ)、NS-7 workaround 数増)で **exit 3**。性能系(NS-4/NS-5)は前回比
  **+20% 超**で exit 3(ノイズ余裕)。CI ではこの exit で job が落ち、
  失敗通知 Issue が起票される。NS-7 の構造負債ラチェットは
  `check_structural_debt_inventory.sh` 自身が baseline 超過で exit 1 にするため
  本スクリプトは現在値の記録のみ行う。
- **nightly 組み込み**: `.github/workflows/nightly-gates.yml`(#8633)に
  `north-star` job を追加し、Linux で収集可能な NS-1/2/4/5/7 と NS-6 転記を
  毎晩実行して日付レコード + TSV 行を main に push する。iOS 系(NS-3・iOS
  実機 cold 起動)は macOS runner 依存のため **CI では欠測**とし、本ドキュメント
  の規約どおり **手動・月次**で埋める(未計測の月は「未計測」と明記)。

## 依存 Issue が閉じたときの列追加手順

予約指標や新カテゴリを時系列に足すときは以下を順に行う(冒頭「1 増やすなら
1 落とす」規約を満たすこと):

1. NORTH_STAR.md に指標の 7 点セット(または NS-7 の新カテゴリ)を追記。
2. `scripts/north_star_report.sh` に収集ロジックと TSV 列を追加
   (TSV はヘッダ付き追記なので、既存行は末尾 `n/a` として後方互換に扱う)。
3. 追加後の最初の nightly レコード markdown に「定義変更」注記を残す
   (時系列の断絶を明示。旧レコードは書き換えない)。

具体的な予約: **コーパス成功率**(#8614)は既に NS-2 として実装済み。
**Rust 意味論比率**(#8648)が閉じたら上記手順で NS-7 のカテゴリとして追加する。

## Milestone / 四半期レビューでの使い方

- **投資判断**: NS-1/NS-2(正しさ・カバレッジ)が横ばいで NS-7 が増勢なら
  構造負債(Milestone #60 系)へ、NS-3(ユーザー価値)が頭打ちなら
  パッケージ/サンプル対応へ投資する — という形で、指標の趨勢を根拠に議論する。
- **Milestone 完了判定**: Milestone 開始時点の north_star レコードを before、
  クローズ時点のレコードを after として比較を貼る。Milestone #60 の before は
  2026-07-02 レコード(#8681 の初回ベースライン)。
- **後退の扱い**: 方向が明確な指標(NS-1/NS-2/NS-7)は前回比悪化 = 即調査。
  性能系(NS-4/NS-5)はノイズ余裕(前回比 +20%)を超えたら調査。NS-6 の失敗は
  既存の main-full 自動 Issue に委譲。
- **指標が動かない場合**: 3 ヶ月動かない指標は「測る意味があるか」を四半期
  レビューで再評価し、落とす候補にする(7±2 規約の維持)。

## 関連

- 親 Issue: #8680(North Star 指標の定義)
- #8681(本ドキュメント + 初回ベースライン)/ #8682(`north_star_report.sh`
  自動化・nightly 組み込み・趨勢監視)
- `docs/vm/PARITY_TARGET.md`(NS-1 の比較対象バージョン規約)
- `docs/vm/PARSER_CORPUS_BASELINE.md`(NS-2 の初回ベースラインと解釈)
- `benchmarks/README.md`(NS-4 の 3 層計測の方法論、#8458)
- `docs/vm/CODE_AUDITS.md`(NS-7 のラチェット audit 群)
