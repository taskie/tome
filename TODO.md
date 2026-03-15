# tome — 残タスク

> **方針: 個人ツールとしての完成度向上。認証・RBAC はスコープ外（外部インフラで代替）。**

## 残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | スナップショット参照記法 — `@latest` / `@latest~N` / `@YYYY-MM-DD[Thh:mm]` / i64 直指定。`diff`, `show`, `restore` に適用 |
| 高 | `tome log` — スナップショット一覧（`--limit`, `--oneline`, `--after`, `--before`） |
| 高 | `tome show <ref>` — スナップショット詳細（diff + metadata）。参照記法を使用 |
| 高 | `tome files` — 追跡中ファイル一覧（entry_cache）（`--prefix`, `--format`, `--include-deleted`） |
| 高 | `tome history <path>` — ファイル変更履歴（`ops::path_history_with_blobs`） |
| 高 | `tome status` — 前回スキャンからの変更を read-only 検出。`scan.rs` の判定フェーズを分離（`--hash` で full digest） |
| 中 | `tome repo list/rm/rename` — リポジトリ管理サブコマンド（`rm` は `--force` 必須、cascade 削除） |
| 中 | `sync` → `remote` リネーム — `tome remote add/rm/list/set` を新設、`sync add/rm/list/set` は非推奨警告つきで残す |
| 中 | `tome tag rm` 追加 — `tag delete` をエイリアスとして残す（`store rm`, `remote rm` と統一） |
| 中 | `.tomeignore` サポート — `ignore::WalkBuilder::add_custom_ignore_filename(".tomeignore")` を追加 |
| 中 | プログレス表示 — `indicatif` クレートで stderr にバー表示（`--quiet` / `--verbose` で制御） |
| 中 | 並列ハッシュ計算 — `tokio::task::spawn_blocking` + `--jobs N`（デフォルト: num_cpus）。DB 書き込みは逐次 |
| 中 | `store push` / `store copy` 並列化 — `tokio::sync::Semaphore` で同時接続数を制限（`--jobs N`）。スキーム別デフォルト: `file://`=4, `ssh://`=4, `s3://`=8 |
| 中 | `store push` のバッチクエリ化 — N+1 クエリ（blob ごとに `replica_exists`）を `blobs_missing_in_store` 1クエリに削減 |
| 中 | ストアへの暗号化設定紐付け — `tome store add s3 <url> --encrypt --key-source pass://tome/key --cipher aes256gcm` で `stores.config` に保存し、`store push` / `store copy` で自動適用。`store set --no-encrypt` で無効化。`store list` で暗号化状態を表示 |
| 中 | `store push` の直接リモート対応 — ローカルストアを中間バッファとせずスキャン元から任意ストアに直接 push。暗号化ストアにも対応（ストア設定を自動参照） |
| 中 | `--format json` — `log`, `show`, `files`, `history`, `diff`, `status`, `repo list`, `store list`, `remote list`, `store push` に追加。`store push --format json` は `{pushed, skipped, errors, bytes_transferred, duration_ms}` を出力 |
| 中 | `verify` 統合 — `tome verify --store <name>` / `--all` を追加（`tome store verify` はエイリアスとして残す） |
| 中 | entry_cache 再構築 — `tome cache rebuild` + sync pull 後の自動再構築オプション |
| 中 | sync push 時のコンフリクト検知 — 中央 DB の分岐を検出し警告 |
| 中 | sync フィルタ — `--include` / `--exclude` でパスを絞った選択的同期 |
| 中 | 重複レポート（`tome dedup`）— blob の content-addressing を活かしリポジトリ横断で重複ファイルを報告 |
| 中 | Webhook / 通知 — `--exec "cmd {pushed} {errors}"` でコマンド実行（先行実装）、または `tome.toml` の `[hooks] after_scan` / `after_push` で Slack / Discord / 汎用 HTTP POST |
| 中 | `tome restore --check` — 復元前に blob の replica 存在確認（store の到達可能性チェック） |
| 低 | `--repo` デフォルト一貫化 — 全コマンドで `tome.toml` の `[scan] repo` をデフォルト値として参照 |
| 低 | 終了コード整理 — 部分失敗(exit 1) / 致命的エラー(exit 2) を区別。cron 自動化向け |
| 低 | 鍵ローテーション — aether Header 拡張 + `store reencrypt` コマンド |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
| 低 | README: 鍵管理ガイダンス（暗号鍵は暗号化バックアップと別の物理場所に保管）と `store push` / `store copy` の冪等性を明記 |

各タスクは「機能実装 → テスト追加 → ドキュメント更新」の粒度でコミットする。
完了後に `cargo fmt --all -- --check && cargo clippy --all --no-deps -- -D warnings && cargo test --all` を確認。

## AWS デプロイ（Lambda + DynamoDB + S3）

Lambda コードは実装済み（`tome-server --features lambda,dynamodb`）。
DynamoDB をメタデータバックエンドとして使用。詳細は [docs/arch/lambda-deployment.md](docs/arch/lambda-deployment.md) を参照。

## 実装済み（未ドキュメント）

- aether モジュール分割 — `AetherError`（thiserror）、`zeroize` による鍵ゼロクリア、fallible API
- `GET /diff` 削除ファイル対応
- `path_history` API の digest 欠落修正（blob JOIN）
- `verify` — missing ファイルも異常終了（Err）として報告
- GC FK 制約修正 — entry_cache を先にクリアしてから entries を削除
- エラーハンドリング改善（AppError 構造化、Mutex パニック除去、Context 付与）
- tome-server Lambda 対応 — `cargo lambda build --release --features lambda --bin tome-lambda`
- 宣言型 DDL — `tome-db/schema.sql` に PostgreSQL DDL を出力、DSQL 抽象化層を削除
- AWS IAM SigV4 認証 — sync push/pull で Lambda Function URL に署名付きリクエスト
- Lambda 起動時マイグレーション無効化 — `connection::connect()` で接続のみ（psqldef でスキーマ適用）
- `MetadataStore` トレイト抽象化 — `tome-db/src/store_trait.rs`（~40 メソッド）、`SeaOrmStore` 実装
- tome-server ルート全体を `dyn MetadataStore` に移行（sea-orm 直接依存を削除）
- `tome-dynamo` クレート — DynamoDB シングルテーブル設計による `MetadataStore` 実装
- Lambda DynamoDB 対応 — `--features dynamodb` + `TOME_DB=dynamodb://<table>` で DynamoDB バックエンドを選択
