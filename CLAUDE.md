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

## PostgreSQL 中央同期方針

複数マシンがそれぞれローカル SQLite を持ちつつ、中央の Aurora (PostgreSQL) + 暗号化 S3
ストアに push/pull してスナップショット履歴とファイル実体を共有する。

### 前提構成

```
                        ┌─────────────────────────────────────────┐
                        │           AWS Central                   │
                        │                                         │
                        │  ┌──────────────────┐  ┌────────────┐  │
                        │  │ Aurora PostgreSQL │  │ S3 Bucket  │  │
                        │  │ (metadata)       │  │ (encrypted │  │
                        │  │                  │  │  blobs)    │  │
                        │  └──────────────────┘  └────────────┘  │
                        └────────▲──────┬─────────────▲────┬─────┘
                  push/pull meta │      │ meta   blob │    │ blob
               ┌─────────────────┘      └──────┐     │    │
[Machine A]    │                                │     │    │   [Machine B]
 SQLite ───────┘                                └─────│────│──── SQLite
 local store                                          │    │    local store
               push/pull blob ────────────────────────┘    │
                                          pull blob ───────┘
```

- **Aurora PostgreSQL**: スナップショット / エントリ / blob メタデータの集約ハブ
- **暗号化 S3**: blob 実体（aether で暗号化済み）の共有ストア
- **各マシン**: ローカル SQLite + ローカル store でスキャン・管理

### 現状

- `sync pull` は peer の DB に **直接接続** し、snapshots / entries / blobs メタデータを丸コピー
- peer_url は `sqlite:///path` または `postgres://...` — SeaORM は両方対応済み
  （ワークスペースで `sqlx-sqlite` + `sqlx-postgres` を有効化済み）
- `store push` でローカルファイル → S3 にアップロード（`EncryptedStorage` 経由で暗号化）
- `store copy` で store 間の blob コピー（暗号化オプション付き）
- Sonyflake ID (machine_id 0–65535) でレコード ID を生成 → マシンごとに異なる ID
- `sync_peers.last_snapshot_id` で増分メタデータ同期を追跡
- `replicas` テーブルで blob の store 内の所在を管理

### 設計方針

#### 1. 2層同期: メタデータ + blob 実体

中央同期は **メタデータ同期**（DB ↔ Aurora）と **blob 同期**（local store ↔ S3）の
2 層で構成する。既存の `sync` と `store` コマンドの責務を活かす:

| 層 | コマンド | 転送内容 | 接続先 |
|----|----------|----------|--------|
| メタデータ | `sync push` / `sync pull` | snapshots, entries, blobs (行) | Aurora PostgreSQL |
| blob 実体 | `store push` / `store copy` | 暗号化ファイル | S3 |

一括操作のショートカットとして `tome sync full push` / `tome sync full pull` を
将来的に提供する（内部で両方を実行）。

#### 2. machine_id によるレコード ID 分離

- Sonyflake の `machine_id` (16 bit, 0–65535) でマシンごとに一意な ID 空間を確保
- 異なるマシンから push された ID は**衝突しない**（Sonyflake の保証）
- 中央同期を使う場合は machine_id の**重複を防ぐ仕組みが必須**

**machine_id 払い出し API（tome-server）:**

中央の Aurora に接続する tome-server に machine_id 払い出しエンドポイントを設ける:

```
POST /api/machines/register
  Request:  { "name": "machine-a", "description": "Dev laptop" }
  Response: { "machine_id": 1, "name": "machine-a", "created_at": "..." }

GET  /api/machines
  Response: [{ "machine_id": 1, "name": "machine-a", ... }, ...]
```

**スキーマ:**

```sql
CREATE TABLE machines (
    machine_id  SMALLINT PRIMARY KEY,  -- 0–65535 (Sonyflake の上限)
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ
);
```

**運用フロー:**

1. 新しいマシンで `tome init --server https://central.example.com` を実行
2. CLI が `POST /api/machines/register` を呼び、未使用の machine_id を取得
3. 取得した machine_id を `~/.config/tome/tome.toml` に自動書き込み
4. 以降の Sonyflake ID 生成は払い出された machine_id を使用

**払い出しロジック:**
- サーバ側で `SELECT COALESCE(MAX(machine_id), 0) + 1` または空き番号を探索
- 同時登録の競合は DB の UNIQUE 制約 + リトライで対処
- `tome.toml` に既に `machine_id` が設定済みの場合は払い出しをスキップし、
  サーバに登録のみ行う（`PUT /api/machines/{id}` で名前を紐付け）

**フォールバック:**
- tome-server が利用できない環境では手動で `machine_id` を `tome.toml` に設定
- `machine_id = 0`（デフォルト）はローカル専用モードとして予約し、
  中央同期時に 0 のままだと警告を出す

#### 3. sync push の新設

現在は `sync pull` のみ。中央 PG パターンでは **push** も必要:

```
tome sync push <peer-name>
```

実装は pull の逆方向:
1. ローカル DB から `last_synced_snapshot_id` 以降の snapshots を取得
2. 各 snapshot の entries + blobs メタデータを Aurora に `get_or_create_blob` / `insert_entry_*` で書き込み
3. Aurora 側にも `sync_peers` レコードを作成し、push 元マシンの進捗を管理

#### 4. blob 実体の同期フロー

**push 側（Machine A → S3）:**
1. `tome sync push central` — メタデータを Aurora に push
2. `tome store push --store central-s3` — 新規 blob をローカルファイルから暗号化 S3 へアップロード
   - `EncryptedStorage<S3Storage>` で透過的に暗号化（AES-256-GCM or ChaCha20-Poly1305）
   - `replicas` テーブルに S3 内パスを記録

**pull 側（S3 → Machine B）:**
1. `tome sync pull central` — メタデータを Aurora から pull → blobs 行がローカル SQLite に入る
2. blob 実体が必要な場合: `store copy central-s3 local` で S3 → ローカル store にダウンロード・復号
   - `EncryptedStorage` の `download` が自動復号
   - replicas テーブルでローカル store にも記録

**replica 管理:**
- Aurora 上の `replicas` テーブルに S3 内のパスが記録される
- `sync push` でメタデータを push する際、ローカルの `replicas` も Aurora に push する
  → pull 側は Aurora の replicas を見て S3 内の blob パスを解決

#### 5. 暗号鍵の管理

- 暗号鍵は中央には保存しない（知識分離: Aurora/S3 だけでは復号不可）
- 共有アクセスが必要なマシン間では鍵ファイルを帯域外（USB、秘密管理ツール等）で配布
- `tome.toml` の `store.key_file` で鍵パスを設定（既存の仕組み）

```toml
# tome.toml
[store]
default_store = "central-s3"
key_file = "~/.config/tome/keys/central.key"
cipher = "chacha20-poly1305"
```

#### 6. repository の名前空間

同じ repo 名 `"default"` が複数マシンから push されたとき:

- **案 A: マシン修飾名** — `"default@machine-a"` のように中央側で名前を修飾
- **案 B: 同一 repo に統合** — blob は content-addressed なので重複排除は自然。
  snapshots は全マシン分が並ぶ（`snapshot.machine_id` で識別）
- **推奨: 案 B** — blob の dedup が活きる。snapshot に `machine_id` カラムを追加
  （または `snapshot.config` JSON に `"source_machine_id"` を記録）

#### 7. スキーマ変更

| 変更 | 内容 |
|------|------|
| `snapshots.source_machine_id` | push 元のマシン ID（nullable — ローカル scan は null） |
| `snapshots.source_snapshot_id` | push 元での元 snapshot ID（nullable — provenance 追跡用） |

既存テーブル・マイグレーションに追加マイグレーションとして実装（破壊的変更なし）。

#### 8. 競合解決

- **blob**: content-addressed → 競合なし（同一 digest = 同一 blob）
- **replica**: blob_id + store_id で一意 → 同一 blob を同一 S3 に重複登録しない
- **entry_cache**: repository ごと・マシンごとにスコープ。Aurora には push しない（ローカル SQLite のみ）
- **snapshot 順序**: 各マシンの snapshot は独立した parent_id チェーンを持つ。
  Aurora では複数チェーンが共存する（マージは行わない）

#### 9. 接続 URL の例

```toml
# tome.toml (Machine A)
machine_id = 1
db = "tome.db"

[store]
default_store = "central-s3"
key_file = "~/.config/tome/keys/central.key"
cipher = "chacha20-poly1305"
```

```bash
# 中央 Aurora を sync peer として登録
tome sync add central "postgres://user:pass@aurora.ap-northeast-1.rds.amazonaws.com/tome" --repo default

# 暗号化 S3 を store として登録
tome store add central-s3 "s3://my-tome-bucket/store"

# 日常の同期オペレーション
tome sync push central          # メタデータ → Aurora
tome store push --store central-s3  # blob 実体 → 暗号化 S3
tome sync pull central          # メタデータ ← Aurora
tome store copy central-s3 local    # blob 実体 ← S3（必要時）
```

#### 10. 実装ステップ（詳細）

##### Step 1: `machines` テーブル（マイグレーション + エンティティ）

**ファイル追加:**
- `tome-db/src/migration/m20230901_000010_create_machines.rs`
- `tome-db/src/entities/machine.rs`

**マイグレーション DDL:**
```sql
CREATE TABLE machines (
    machine_id  SMALLINT NOT NULL PRIMARY KEY,  -- 0–65535
    name        TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    last_seen_at TIMESTAMPTZ NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```
- `machine_id` は Sonyflake の u16 に対応 → SeaORM 上は `i16` (SmallInteger)
- SeaORM マイグレーションでは `ColumnDef::new(...).small_integer().not_null().primary_key()`
- `mod.rs` に `m20230901_000010_create_machines` を追加、Migrator vec に追加
- `entities/mod.rs` に `pub mod machine;` と `pub use machine::Entity as Machine;` 追加

**エンティティ Model:**
```rust
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub machine_id: i16,  // 0–65535 (i16 は -32768..32767 だが実用上 0..32767 で十分)
    pub name: String,
    pub description: String,
    pub last_seen_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}
```
注意: Sonyflake の machine_id は u16 (0–65535) だが、PostgreSQL の SMALLINT は -32768–32767。
実運用では 0–32767 の範囲で使う（十分な台数）。エンティティは `i16` で定義。

##### Step 2: `snapshots` に `source_machine_id` / `source_snapshot_id` カラム追加

**ファイル追加:**
- `tome-db/src/migration/m20230901_000011_add_snapshot_source.rs`

**マイグレーション DDL (ALTER TABLE):**
```sql
ALTER TABLE snapshots ADD COLUMN source_machine_id SMALLINT NULL;
ALTER TABLE snapshots ADD COLUMN source_snapshot_id BIGINT NULL;
```
- SeaORM マイグレーションでは `manager.alter_table(...)` を使う
- SQLite の ALTER TABLE ADD COLUMN は DEFAULT なしでも NULL カラムは追加可能
- `entities/snapshot.rs` の Model に 2 フィールドを追加:
  ```rust
  pub source_machine_id: Option<i16>,
  pub source_snapshot_id: Option<i64>,
  ```
- `create_snapshot()` (ops.rs) の ActiveModel にも追加（デフォルト None）

##### Step 3: tome-server に machine_id 払い出し API を追加

**変更ファイル:**
- `tome-server/src/routes.rs` — API ハンドラ追加
- `tome-server/src/server.rs` — ルート追加
- `tome-db/src/ops.rs` — DB 操作関数追加

**新規 API エンドポイント:**
```
POST /api/machines/register   → register_machine()
GET  /api/machines            → list_machines()
PUT  /api/machines/{id}       → update_machine()
```

**ops.rs に追加する関数:**
```rust
pub async fn register_machine(db, name, description) -> Result<machine::Model>
  // SELECT COALESCE(MAX(machine_id), 0) + 1 で次の ID を決定
  // INSERT — UNIQUE 制約違反時はリトライ
pub async fn list_machines(db) -> Result<Vec<machine::Model>>
pub async fn find_machine_by_id(db, machine_id) -> Result<Option<machine::Model>>
pub async fn update_machine_last_seen(db, machine_id) -> Result<()>
```

**routes.rs に追加するハンドラ:**
```rust
#[derive(Deserialize)]
pub struct RegisterMachineRequest { name: String, description: Option<String> }

#[derive(Serialize)]
pub struct MachineResponse { machine_id: i16, name: String, ... }

pub async fn register_machine(db: Db, Json(req): Json<RegisterMachineRequest>) -> AppResult<Json<MachineResponse>>
pub async fn list_machines(db: Db) -> AppResult<Json<Vec<MachineResponse>>>
```

**server.rs のルート追加:**
```rust
.route("/api/machines", get(routes::list_machines))
.route("/api/machines/register", post(routes::register_machine))
.route("/api/machines/{id}", put(routes::update_machine))
```
- 既存ルートは `/repositories/...` パターン。新規は `/api/machines/...` で API バージョン意識

##### Step 4: `tome init` サブコマンド（CLI）

**変更ファイル:**
- `tome-cli/src/commands/mod.rs` — `pub mod init;` 追加
- `tome-cli/src/commands/init.rs` — 新規作成
- `tome-cli/src/main.rs` — `Init` サブコマンド追加

**CLI インターフェース:**
```
tome init --server <url> [--name <machine-name>]
```

**処理フロー:**
1. `--server` の URL に `POST /api/machines/register` を送信
2. レスポンスの `machine_id` を取得
3. `~/.config/tome/tome.toml` に `machine_id = <value>` を書き込み
   - ファイルが既にある場合は `machine_id` 行のみ追加/更新
4. 設定済みの場合（`tome.toml` に machine_id あり）は確認プロンプト or `--force`

**依存追加:**
- `tome-cli/Cargo.toml` に `reqwest` (HTTP クライアント) — `features = ["json", "rustls-tls"]`
- ワークスペース `Cargo.toml` にも `reqwest` を追加

##### Step 5: `sync push` サブコマンド

**変更ファイル:**
- `tome-cli/src/commands/sync.rs` — `Push` サブコマンド追加

**SyncCommands enum に追加:**
```rust
Push(SyncPushArgs),
```

**SyncPushArgs:**
```rust
pub struct SyncPushArgs {
    pub name: String,
    #[arg(long, default_value = "default")]
    pub repo: String,
}
```

**sync_push() 実装（pull の逆方向）:**
```rust
async fn sync_push(local_db, args) -> Result<()> {
    // 1. ローカル repo と peer を解決
    // 2. peer DB (Aurora) に接続
    // 3. ローカルの新規 snapshots を取得（peer.last_snapshot_id 以降）
    // 4. 各 snapshot について:
    //    a. blob メタデータを peer DB に get_or_create_blob
    //    b. snapshot を peer DB に create_snapshot (source_machine_id, source_snapshot_id 付き)
    //    c. entries を peer DB に insert_entry_present / insert_entry_deleted
    // 5. replicas も peer DB に同期（ローカルの replicas を peer に insert_replica）
    // 6. peer の sync_peers 進捗を更新
}
```

**ops.rs に追加する関数:**
```rust
pub async fn create_snapshot_with_source(db, repository_id, parent_id, message, source_machine_id, source_snapshot_id) -> Result<snapshot::Model>
pub async fn all_replicas_for_blobs(db, blob_ids: &[i64]) -> Result<Vec<replica::Model>>
```

##### Step 6: replicas の双方向同期

**push 時:**
- ローカルの replicas のうち、push 対象 blob に紐づくものを Aurora にも insert
- `replica_exists()` で重複チェック済み

**pull 時:**
- Aurora 上の replicas をローカルに取り込む
- `sync_pull()` の既存ループ内に replicas コピーを追加:
  ```rust
  // blob を取り込んだ後、その blob の replicas も Aurora から取得してローカルに insert
  let remote_replicas = ops::replicas_for_blob(&peer_db, remote_blob.id).await?;
  for r in remote_replicas {
      if !ops::replica_exists(local_db, local_blob.id, r.store_id).await? {
          // store が存在しなければ get_or_create_store で作成
          ops::insert_replica(local_db, local_blob.id, store.id, &r.path, r.encrypted).await?;
      }
  }
  ```

**ops.rs に追加する関数:**
```rust
pub async fn replicas_for_blob(db, blob_id: i64) -> Result<Vec<replica::Model>>
```

#### 11. 制約・注意事項

- SeaORM のマイグレーションは SQLite / PostgreSQL 両方で動作するが、
  一部 DDL（`ALTER TABLE ADD COLUMN` の `DEFAULT` 挙動等）に差異あり — テスト必須
- PostgreSQL の `BIGINT` と SQLite の `INTEGER` はビット幅が異なるが、
  SeaORM は `i64` で統一しているため実用上問題なし
- S3 の blob は **常に暗号化** — 平文 blob を S3 に置かない方針
  （ローカル store は暗号化任意）
- ネットワーク越しの大量 insert はトランザクションバッチ化が必要（100件/txn 等）
- S3 のアップロード/ダウンロードには AWS 認証が必要
  （IAM Role / 環境変数 / `~/.aws/credentials` — `aws-config` クレートの既定チェーン）
- Aurora のセキュリティグループで接続元 IP を制限し、SSL 接続を強制すること

---

## 残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | リファクタリング Phase 1–3 — 上記のリファクタリング方針を参照 |
| 中 | Watch モード（`tome watch`） — inotify/fsevents で監視し自動スナップショット |
| 中 | ChaCha20-Poly1305 導入（aether）— 実装済み |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | PostgreSQL 中央同期 — 上記の PostgreSQL 中央同期方針を参照 |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
