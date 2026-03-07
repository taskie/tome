# tome — 開発ガイド

> Rust 製ファイル変更追跡システム。ハッシュ（SHA-256 / BLAKE3 + xxHash64）で変更を検知し、
> スナップショット履歴を SQLite/PostgreSQL に記録する。

- ソフトウェアの利用方法は **[README.md](README.md)** を参照。
- 設計・スキーマ・API の詳細は **[ARCHITECTURE.md](ARCHITECTURE.md)** を参照。
- 残タスク・改善計画は **[TODO.md](TODO.md)** を参照。
- CLAUDE.md （当ファイル）には開発上の規約を記載する。

---

## Git コミットについて

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
tome-web/     — Next.js 16 Web フロントエンド（tome-web/CLAUDE.md 参照）
aether/       — Streaming AEAD 暗号化ライブラリ（aether/CLAUDE.md 参照）
treblo/       — ハッシュアルゴリズム（xxHash64/SHA-256/BLAKE3）・ファイルツリー走査・hex ユーティリティ
```

レガシークレート（ichno / ichnome 等）は `obsolete/` 下にアーカイブ済み。

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
