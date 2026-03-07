# tome — 残タスク

> **方針: 個人ツールとしての完成度向上。認証・RBAC はスコープ外（外部インフラで代替）。**

## 改善計画フェーズ（`tmp/tome-improvement-plan.md` より）

各フェーズは「機能実装 → テスト追加 → ドキュメント更新」の粒度でコミットする。
完了後に `cargo fmt --all -- --check && cargo clippy --all --no-deps -- -D warnings && cargo test --all` を確認。

| Phase | 優先度 | 内容 |
|-------|--------|------|
| **1** | 高 | `ScanMetadata` 型安全化 — `tome-core` に構造体定義、`serde_json::json!` を置き換え |
| **1** | 高 | 空スナップショット抑制 — added+modified+deleted=0 なら自動削除（`--allow-empty` で従来動作維持） |
| **1** | 高 | scan_root 永続化 — `repositories.config["scan_root"]` に保存、`--path` 省略時に自動参照 |
| **2** | 高 | スナップショット参照記法 — `@latest` / `@latest~N` / `@YYYY-MM-DD[Thh:mm]` / i64 直指定。`diff`, `show`, `restore` に適用 |
| **3** | 高 | `tome log` — スナップショット一覧（`--limit`, `--oneline`, `--after`, `--before`） |
| **3** | 高 | `tome show <ref>` — スナップショット詳細（diff + metadata）。Phase 2 の参照記法を使用 |
| **3** | 高 | `tome files` — 追跡中ファイル一覧（entry_cache）（`--prefix`, `--format`, `--include-deleted`） |
| **3** | 高 | `tome history <path>` — ファイル変更履歴（`ops::path_history_with_blobs`） |
| **4** | 高 | `tome status` — 前回スキャンからの変更を read-only 検出。`scan.rs` の判定フェーズを分離（`--hash` で full digest） |
| **5** | 中 | `tome repo list/rm/rename` — リポジトリ管理サブコマンド（`rm` は `--force` 必須、cascade 削除） |
| **5** | 中 | `sync` → `remote` リネーム — `tome remote add/rm/list/set` を新設、`sync add/rm/list/set` は非推奨警告つきで残す |
| **5** | 中 | `tome tag rm` 追加 — `tag delete` をエイリアスとして残す（`store rm`, `remote rm` と統一） |
| **6** | 中 | `.tomeignore` サポート — `ignore::WalkBuilder::add_custom_ignore_filename(".tomeignore")` を追加 |
| **6** | 中 | プログレス表示 — `indicatif` クレートで stderr にバー表示（`--quiet` / `--verbose` で制御） |
| **7** | 中 | 並列ハッシュ計算 — `tokio::task::spawn_blocking` + `--jobs N`（デフォルト: num_cpus）。DB 書き込みは逐次 |
| **7** | 中 | `store push` / `store copy` 並列化 — `tokio::sync::Semaphore` で同時接続数を制限（`--jobs N`） |
| **8** | 中 | `--format json` — `log`, `show`, `files`, `history`, `diff`, `status`, `repo list`, `store list`, `remote list` に追加 |
| **8** | 中 | `verify` 統合 — `tome verify --store <name>` / `--all` を追加（`tome store verify` はエイリアスとして残す） |
| **8** | 低 | `--repo` デフォルト一貫化 — 全コマンドで `tome.toml` の `[scan] repo` をデフォルト値として参照 |

## AWS デプロイ（Lambda + DSQL + S3）

**アーキテクチャ**: tome-server を AWS Lambda で動かし、DB に Aurora DSQL、ストレージに S3 を使用。
クライアント（tome-cli）は既存の `https://` HTTP sync モードをそのまま利用。Lambda は IAM ロールで DSQL に接続。

**コスト試算**: Lambda 無料枠 + DSQL + S3 で ~$2–3/月。

**DSQL 制限**:

| 制限 | tome への影響 |
|------|--------------|
| FK 非強制 | GC/sync の削除順序はアプリ側で既に正しい → 変更不要 |
| JSONB 非対応 | `repositories.config` が JSONB。`json` 型への変更が必要（DSQL 使用時） |
| トリガー・シーケンス非対応 | tome-db は使用していない → 変更不要 |

**ビルド & デプロイ**:

```bash
# ビルド（要: cargo install cargo-lambda）
cargo lambda build --release --features lambda --bin tome-lambda

# Lambda 関数として作成（初回）
cargo lambda deploy tome-lambda \
  --runtime provided.al2023 \
  --memory-size 256 \
  --timeout 30

# 環境変数を設定
aws lambda update-function-configuration \
  --function-name tome-lambda \
  --environment "Variables={TOME_DB=postgres://admin:<iam-token>@<cluster-endpoint>:5432/postgres?sslmode=require}"
```

IAM トークンは `aws dsql generate-db-connect-admin-auth-token` で生成（有効期限最大 1 週間）。

## その他の残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | Watch モード（`tome watch`）— inotify/fanotify/kqueue でバックグラウンド監視し自動スナップショット |
| 中 | entry_cache 再構築 — `tome cache rebuild` + sync pull 後の自動再構築オプション |
| 中 | sync push 時のコンフリクト検知 — 中央 DB の分岐を検出し警告 |
| 中 | sync フィルタ — `--include` / `--exclude` でパスを絞った選択的同期 |
| 中 | 重複レポート（`tome dedup`）— blob の content-addressing を活かしリポジトリ横断で重複ファイルを報告 |
| 中 | Webhook / 通知 — スキャン完了・変更検知時に変更サマリを POST（Slack, Discord, 汎用 HTTP） |
| 中 | `tome restore --check` — 復元前に blob の replica 存在確認（store の到達可能性チェック） |
| 低 | 鍵ローテーション — aether Header 拡張 + `store reencrypt` コマンド |
| 低 | 外部シークレットマネージャ統合 — `key_source = "aws-secrets-manager://..."` / `"vault://..."` / `"env://"` |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |

## 実装済み

- `tome push` / `tome pull` — scan + store push + sync push / sync pull + store copy の統合コマンド
- HTTP sync API（`/sync/push`, `/sync/pull`）— DB 直接接続と HTTP の二重モード対応
- `tome restore` — snapshot ID + store 指定からファイルを宛先ディレクトリに復元
- aether モジュール分割 — `AetherError`（thiserror）、`zeroize` による鍵ゼロクリア、fallible API
- `GET /diff` 削除ファイル対応
- `path_history` API の digest 欠落修正（blob JOIN）
- `verify` — missing ファイルも異常終了（Err）として報告
- GC FK 制約修正 — entry_cache を先にクリアしてから entries を削除
- エラーハンドリング改善（AppError 構造化、Mutex パニック除去、Context 付与）
- `store set` / `store rm` — ストア登録の更新・削除（`--force` で replica 付きも削除可）
- `sync set` / `sync rm` — sync peer の更新・削除
- tome-server Lambda 対応 — `cargo lambda build --features lambda --bin tome-lambda`
