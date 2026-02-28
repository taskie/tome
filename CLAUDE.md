# tome — 開発ガイド

> Rust 製ファイル変更追跡システム。ハッシュ（SHA-256 / BLAKE3 + xxHash64）で変更を検知し、
> スナップショット履歴を SQLite/PostgreSQL に記録する。

**著者:** taskie <t@skie.jp>
**ブランチ:** feature/tome（再設計実装中）
**コミットメッセージは英語で書くこと**

設計・スキーマ・API の詳細は **[ARCHITECTURE.md](ARCHITECTURE.md)** を参照。

---

## コミット前に必ず実行すること

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
tome-cli/     — 統一 CLI（scan / store / sync / diff / restore / tag / verify / gc / serve）
tome-web/     — Next.js 16 Web フロントエンド
aether/       — AEAD 暗号化ライブラリ（AES-256-GCM / ChaCha20-Poly1305 + Argon2id）
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

---

## BLAKE3 対応（実装済み）

SHA-256 の代わりに BLAKE3 を選択可能。リポジトリ単位で `repositories.config["digest_algorithm"]` に記録。
`tome scan --digest-algorithm blake3` で新規リポジトリ作成時に指定。既存リポジトリの変更は不可。

実装:
- `tome-core/src/hash.rs`: `DigestAlgorithm` enum (Sha256 | Blake3)、`hash_file()` でディスパッチ
- `tome-db/src/ops.rs`: `get_or_create_blob` / scan 処理で config を参照
- BLAKE3 も 32 バイト出力のため、DB スキーマ・ストレージパス・API は変更なし

---

## ChaCha20-Poly1305（実装済み）

AES-256-GCM に加えて ChaCha20-Poly1305 を選択可能。
両アルゴリズムとも鍵長 32B・nonce 12B・認証タグ 16B で一致するため、Header サイズ (32B) や
チャンク暗号化ロジック、DB スキーマ、Argon2id 鍵導出は変更なし。

- `aether`: `CipherAlgorithm` enum + `AeadInner` enum ディスパッチ（`Box<Aes256Gcm>` / `ChaCha20Poly1305`）
- Header `flags` 下位 1 ビットでアルゴリズム識別（0 = AES, 1 = ChaCha）— 復号時は自動判別
- `Cipher::with_algorithm()`, `with_key_slice_algorithm()`, `with_password_algorithm()` で選択
- `aether-cli`: `--cipher chacha20-poly1305` オプション（デフォルト: aes256gcm）
- `tome-store`: `EncryptedStorage::with_algorithm()` で暗号アルゴリズムを指定
- `tome-cli`: `tome store copy --encrypt --cipher chacha20-poly1305` で選択
- 既存の暗号化ファイル（`flags == 0`）は AES-256-GCM として解釈（後方互換）

---

## 設定ファイル tome.toml（実装済み）

`~/.config/tome/tome.toml`（グローバル）と `./tome.toml`（プロジェクトローカル）を読み込む。
優先順位（後勝ち）: デフォルト → グローバル → ローカル → 環境変数 → CLI 引数。

```toml
db = "tome.db"
machine_id = 0

[scan]
repo = "default"
no_ignore = false

[store]
default_store = "backup"
key_file = "~/.config/tome/keys/main.key"

[serve]
addr = "127.0.0.1:8080"
```

実装: `tome-cli/src/config.rs` (`TomeConfig` 構造体 + `load_config()`)。
設定ファイルは任意 — なくても全機能が動作する。

---

## リファクタリング方針

現状のコードベースは機能的に動作しているが、ファイルの肥大化と定型パターンの重複が
今後の機能追加（tome.toml 導入、BLAKE3、ChaCha20-Poly1305 等）のボトルネックになる。
以下を段階的に実施する。

### 現状の規模

| ファイル | 行数 | 課題 |
|----------|------|------|
| `tome-db/src/ops.rs` | 949 | 53 関数が 1 ファイルに集中。ドメイン境界が不明瞭 |
| `tome-server/src/routes.rs` | 527 | 全ハンドラが 1 ファイル。レスポンス型定義と混在 |
| `tome-cli/src/commands/store.rs` | 380 | push/copy/verify に進捗カウンタ・ストア取得の重複 |
| `tome-cli/src/commands/scan.rs` | 309 | `process_file()` が 64 行で 2 ステージ混在 |

### Phase 1: tome-db/src/ops.rs 分割 【優先度: 高】

949 行・53 関数を機能ドメインごとにモジュール分割する。

```
tome-db/src/ops/
  mod.rs          — pub use で再エクスポート（既存の use パスを維持）
  repository.rs   — get_or_create_repository, *_digest_algorithm (4 関数)
  blob.rs         — get_or_create_blob, find_blob_by_*, blobs_by_ids (5 関数)
  snapshot.rs     — create_snapshot, latest_snapshot, update_snapshot_metadata, list/all (5 関数)
  entry.rs        — insert_entry_*, entries_*, entries_by_prefix (6 関数)
  entry_cache.rs  — load_entry_cache, upsert_cache_*, list/present_cache_entries (6 関数)
  store.rs        — get_or_create_store, find_store_by_name, list_stores (3 関数)
  replica.rs      — replica_exists, insert_replica, replicas_* (7 関数)
  sync_peer.rs    — insert_sync_peer, find/list/update_sync_peer (4 関数)
  tag.rs          — upsert_tag, delete_tags, list_tags, search_blobs_by_tag (4 関数)
  diff.rs         — path_history, entries_for_blob, diff 系 (3 関数)
  gc.rs           — blob_ids_in_snapshots, unreferenced_blobs, delete_* (6 関数)
```

**手順:**
1. `ops.rs` → `ops/mod.rs` にリネーム
2. ドメインごとにファイルを切り出し、`mod.rs` で `pub use` 再エクスポート
3. 外部クレート（tome-cli, tome-server）の `use tome_db::ops::*` は変更不要

### Phase 2: tome-server/src/routes.rs 分割 【優先度: 中】

527 行を構造化する。

```
tome-server/src/
  routes/
    mod.rs           — ルーター定義 (pub fn router())
    responses.rs     — レスポンス型 + From impl（BlobResponse, EntryResponse 等）
    repositories.rs  — list_repositories, get_repository, list_snapshots, get_latest
    snapshots.rs     — list_entries
    blobs.rs         — get_blob, list_blob_entries
    files.rs         — list_files
    diff.rs          — diff_snapshots, diff_repos
    history.rs       — path_history
```

**重複解消:**
- リポジトリ取得 + 404 処理を共通ヘルパー `find_repo_or_404()` に抽出（6 箇所で重複）
- `hex_encode` + レスポンス変換を `responses.rs` の `From` impl に集約

### Phase 3: tome-cli コマンドの共通化 【優先度: 中】

**3a. store.rs の重複解消**

push/copy/verify で繰り返される以下のパターンを抽出:

```rust
// 共通: ストア名→Model 解決
async fn resolve_store(db: &DatabaseConnection, name: &str) -> Result<store::Model>

// 共通: 進捗付きバッチ処理
struct BatchProgress { ok: u64, failed: u64, skipped: u64 }
impl BatchProgress {
    fn summary(&self, verb: &str) -> String  // "done: 42 copied, 0 failed, 3 skipped"
}
```

**3b. scan.rs の process_file 分解**

64 行の `process_file()` を 2 つのヘルパーに分割:

```rust
/// mtime + size で変更なしを高速判定
fn is_unchanged(meta: &FileMeta, cached: &CacheEntry) -> bool

/// ハッシュ計算 + DB 登録
async fn record_change(ctx: &mut ScanContext, path: &str, hash: FileHash) -> Result<()>
```

### Phase 4: aether の AEAD 抽象化 【完了済み — ChaCha20-Poly1305 導入時に実施】

`Cipher` 内部を `AeadInner` enum ディスパッチに変更済み。テストも各アルゴリズムでパラメタライズ済み。

### Phase 5: レガシークレート整理 【完了済み — 一部】

ichno*/ichnome*/optional_derive は `obsolete/` に移動し、ワークスペースから除外済み。
残り: treblo/treblo-cli/aether-cli の役割を確認し、不要なら同様に整理。

### 実施順序とガイドライン

1. **Phase 1 → Phase 2** を先に実施（ops.rs, routes.rs の分割は他タスクの前提）
2. **Phase 3** は tome.toml 導入時にあわせて実施（config 受け渡しの変更と同時）
3. 各 Phase は **動作を変えない純粋なリファクタリング** — `cargo test` がパスし続けること
4. 1 Phase = 1 PR。Phase 内でもファイル単位でコミットを分ける
5. `pub use` 再エクスポートで **外部 API を維持** — 他クレートの `use` 文を壊さない

---

## 残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | リファクタリング Phase 1–3 — 上記のリファクタリング方針を参照 |
| 中 | Watch モード（`tome watch`） — inotify/fsevents で監視し自動スナップショット |
| 中 | ChaCha20-Poly1305 導入（aether）— 実装済み |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | PostgreSQL 中央同期 — 複数マシンが一つの PostgreSQL に push/pull（現在は SQLite↔SQLite のみ） |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
