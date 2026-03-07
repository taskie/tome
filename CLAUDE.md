# tome — 開発ガイド

> Rust 製ファイル変更追跡システム。ハッシュ（SHA-256 / BLAKE3 + xxHash64）で変更を検知し、
> スナップショット履歴を SQLite/PostgreSQL に記録する。

- ソフトウェアの利用方法は **[README.md](README.md)** を参照。
- 設計・スキーマ・API の詳細は **[ARCHITECTURE.md](ARCHITECTURE.md)** を参照。
- CLAUDE.md （当ファイル）には開発上の規約や未実装の要素の詳細を記載する。

---

## Gitコミットについて

- Conventional Commits に従い、コミットは意味的に・領域ごとに分割すること
- コミットメッセージは英語で書くこと
- pre-commit フックを成功させるため、コミット前に以下を実行すること

```bash
# Rust — フォーマット & lint
cargo fmt --all
cargo clippy --all --no-deps -- -D warnings

# tome-web — フォーマット & lint
cd tome-web && npm run format && npm run lint
```

---

## クレート構成

```
tome-core/    — ハッシュ計算（SHA-256 / BLAKE3 + xxHash64）・ID生成・共通モデル
tome-db/      — SeaORM エンティティ + マイグレーション + ops
tome-store/   — ファイルストレージ抽象化（Local / SSH / S3 / 暗号化）
tome-server/  — HTTP API サーバー (axum)
tome-cli/     — 統一 CLI（scan / store / sync / remote / push / pull / diff / restore / tag / verify / gc / init / serve / log / show / status / files / history / repo）
tome-web/     — Next.js 16 Web フロントエンド
aether/       — Streaming AEAD 暗号化ライブラリ（AES-256-GCM / ChaCha20-Poly1305 + Argon2id）
treblo/       — ハッシュアルゴリズム（xxHash64/SHA-256/BLAKE3）・ファイルツリー走査・hex ユーティリティ
```

レガシークレート（ichno / ichnome 等）は `obsolete/` 下にアーカイブ済み。

---

## tome-web 技術メモ

- **`export const dynamic = "force-dynamic"`** — 全ページ必須。tome serve が未起動だとビルドに失敗する
- **Tailwind v4** — `@import "tailwindcss"` のみ。`tailwind.config.ts` 不要。PostCSS: `@tailwindcss/postcss`
- **Node.js 20.9+** — Next.js 16 の要件（mise で Node 24 を使用）
- **`env.local.example`（先頭ドットなし）** — ルートの `.gitignore` が `.env*` をブロックするため
- **API は全てサーバーサイドで呼ぶ** — CORS 不要。`TOME_API_URL` はサーバー環境変数（`NEXT_PUBLIC_` 不要）
- **ESLint** — `eslint@9`（`eslint-plugin-react 7.x` が ESLint 10 非対応のため据え置き）

---

## Rust 実装メモ（落とし穴）

- **SeaORM 1.1 必須** — 1.0.x は sqlx 0.7.4 を引き chrono と非互換
- **Sonyflake start_time** — `2023-09-01 UTC`（= 1_693_526_400 秒）。変更すると既存データと非互換
- **SQLite URL** — `sqlite://path?mode=rwc` 形式（tome-cli/src/main.rs で自動変換）
- **entry_cache の複合 PK** — (repository_id, path) の両フィールドに `#[sea_orm(primary_key, auto_increment = false)]` が必要
- **`Box<dyn Storage>`** — `impl Storage for Box<dyn Storage>` を storage.rs に追加（async_trait との組み合わせ）
- **aws-sdk-s3 v1** — `aws_config::defaults(BehaviorVersion::latest()).load().await` を使用（`load_from_env` は deprecated）
- **sync pull の entry_id** — 0 を使うと FK 制約エラー。`insert_entry_*` の戻り値の `.id` を使うこと
- **Rust edition** — 全 tome-* クレートは edition 2024、rust-version 1.85
- **rustfmt.toml** — stable channel のみのオプションを使用（`merge_imports` 等は削除済み）
- **Sonyflake machine_id** — u16 (0–65535) だが PostgreSQL SMALLINT は i16 (-32768–32767)。
  実運用では 0–32767 の範囲で使用。エンティティは `i16` で定義
- **GC の削除順序** — `entry_cache → entries → snapshots` の順で削除しないと FK 制約エラーになる

---

## aether Streaming AEAD 設計

### ヘッダ flags レイアウト（u16）

```
bits [15:12]  version      フォーマットバージョン (0–15)
bits [11:8]   reserved     予約（0 固定、将来利用）
bits [7:4]    chunk_kind   チャンクサイズ選択
bits [3:0]    algorithm    AEAD アルゴリズム
```

後方互換性: 既存ファイル `0x0000`（AES）/ `0x0001`（ChaCha20）は version=0, chunk_kind=0 として正しくパースされる。

### バージョン定義

| version | 意味 |
|---------|------|
| 0 | 現行フォーマット。chunk_kind は無視（常に 8 KiB）。integrity を平文末尾に付加して検証 |
| 1 | Streaming AEAD。STREAM 構成（last-chunk フラグ）、可変チャンクサイズ、ヘッダ AD 認証 |

### algorithm 値

| 値 | アルゴリズム |
|----|-------------|
| 0 | AES-256-GCM |
| 1 | ChaCha20-Poly1305 |
| 2–15 | 予約（AES-256-GCM-SIV, XChaCha20 等） |

### chunk_kind 値

暗号文チャンクサイズ = `8 KiB × 2^chunk_kind`。平文 = 暗号文 − 16 バイト tag。
v0 では chunk_kind は無視され常に 8 KiB。v1 以降で有効。

| chunk_kind | 暗号文チャンクサイズ | 備考 |
|------------|---------------------|------|
| 0 | 8 KiB | デフォルト（v0 互換） |
| 1 | 16 KiB | |
| 2 | 32 KiB | |
| 3 | 64 KiB | |
| 4 | 128 KiB | |
| 5 | 256 KiB | |
| 6 | 512 KiB | |
| 7 | 1 MiB | v1 デフォルト |
| 8 | 2 MiB | |
| 9 | 4 MiB | |
| 10 | 8 MiB | |
| 11 | 16 MiB | |
| 12 | 32 MiB | |
| 13 | 64 MiB | 大規模バックアップ向け |
| 14 | 128 MiB | メモリ使用量に注意 |
| 15 | 256 MiB | メモリ使用量に注意 |

### v1 STREAM nonce 構成

```
nonce (12 bytes) = IV ⊕ (0x00{4} ‖ counter_u64_BE)
last chunk:        nonce[0] ^= 0x80
```

- IV: ヘッダに格納されるランダム 12 バイト
- counter: チャンク番号（0 始まり）。bytes [4..12] に XOR
- last-chunk フラグ: byte [0] の最上位ビット。截断攻撃を防止
- counter の最大値は u64 だが、同一 nonce の再利用を避けるため `2^32` チャンクを上限とする運用を推奨

### v1 ヘッダ認証（Associated Data）

第 1 チャンクの AEAD 暗号化で、ヘッダ 32 バイトを Associated Data (AD) として渡す。

```
chunk_0  = AEAD_encrypt(key, nonce_0, plaintext_0, ad = header_bytes[0..32])
chunk_i  = AEAD_encrypt(key, nonce_i, plaintext_i, ad = "")    (i > 0)
```

これにより、ヘッダの改竄（flags 書換え、IV 差替え等）は第 1 チャンクの復号時に検出される。

### v0 → v1 の主な変更点

| 項目 | v0 | v1 |
|------|----|----|
| last-chunk マーカー | なし（integrity suffix で間接的に検出） | nonce bit で明示 |
| integrity フィールド | 平文末尾にも付加し復号後に検証 | ヘッダのみ（パスワード KDF の salt として使用） |
| チャンクサイズ | 固定 8 KiB | chunk_kind で選択可能 |
| ヘッダ認証 | なし（IV/flags 改竄は AEAD 失敗で間接検出） | 第 1 チャンクの AD で明示認証 |
| 平文末尾バッファリング | 必要（integrity 16 バイトを分離） | 不要 |

### `encrypt_bytes` / `decrypt_bytes` との関係

ファイル名暗号化（`encrypt_bytes` / `decrypt_bytes`）はストリーミングフォーマットとは独立。
単一チャンクの AEAD + nonce 付加方式で、v1 の影響を受けない。

### 実装フェーズ

1. **flags パーサーリファクタ** — `HeaderFlags` 構造体を導入し、version / chunk_kind / algorithm を個別にパース。v0 動作は維持
2. **ChunkKind 型** — サイズ計算メソッドを持つ enum。`cipher.rs` の `BUFFER_SIZE` を動的に
3. **v1 STREAM 暗号化** — `CounteredNonce` に `is_last` パラメータ追加、第 1 チャンクの AD、integrity suffix 除去
4. **v1 STREAM 復号** — ヘッダの version で v0/v1 を分岐。v0 は従来ロジック、v1 は新ロジック
5. **テスト** — v0 既存テスト維持 + v1 roundtrip + 各 chunk_kind + 截断検知 + ヘッダ改竄検知
6. **ドキュメント** — ARCHITECTURE.md の aether セクション更新

---

## エラーハンドリング規約

| クレート | エラー型 | 方式 |
|----------|----------|------|
| tome-core | `CoreError` enum | thiserror + `type Result<T>` |
| tome-db | なし | anyhow::Result 直接使用 |
| tome-store | `StoreError` enum | thiserror + `type Result<T>` |
| tome-cli | なし | anyhow::Result 直接使用 |
| tome-server | `AppError` wrapper | anyhow → axum IntoResponse |

- **ライブラリクレート**（core/db/store）: thiserror の enum を使い、`type Result<T>` を定義
- **アプリケーションクレート**（cli/server）: anyhow を使い、`.context()` で文脈を付与
- **`unwrap()` / `expect()` は原則禁止**: テストコード以外では `?` + `map_err` を使う
  - 安全であることが自明な場合のみ許容（コメント必須: `// safe: ...`）
- **HTTP レスポンス**: `AppError::not_found()` / `bad_request()` を使い、Internal は詳細を隠蔽

---

## テスト方針

### 結合テスト（tome-cli/tests/）

`tome-cli/tests/` 下の結合テストは **README.md に記載されたユースケース・CLI リファレンスと対応** させる。

- **README.md のユースケースやコマンド例を追加・変更した場合、対応する結合テストを `tome-cli/tests/` に追加・更新すること**
- テストファイルはコマンド単位で分割: `scan.rs`, `diff.rs`, `verify.rs`, `store.rs`, `restore.rs`, `tag.rs`, `sync.rs`, `gc.rs`, `push.rs`
- 共通ヘルパーは `common/mod.rs` の `Env` 構造体に集約。新コマンドを追加したら対応するヘルパーメソッドも追加する
- サブコマンド追加時（例: `store set`, `sync rm`）は正常系・異常系（存在しないリソース、引数不足）の両方をテストする
- テスト内で `.git/` を `mkdir` する必要がある場合がある（`ignore` クレートが `.gitignore` 認識に `.git/` を要求するため）

### 単体テスト

- `tome-core/src/lib.rs`: ハッシュ計算・ID 生成のユニットテスト
- `tome-store/src/lib.rs`: ストレージ操作のユニットテスト

---

## 残タスク

> **方針: 個人ツールとしての完成度向上。認証・RBAC はスコープ外（外部インフラで代替）。**

### 改善計画フェーズ（`tmp/tome-improvement-plan.md` より）

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

### AWS デプロイ（Lambda + DSQL + S3）

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

### その他の残タスク

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

### 実装済み

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
