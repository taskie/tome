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
aether/       — AES-256-GCM 暗号化ライブラリ
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

## ChaCha20-Poly1305 導入方針

AES-256-GCM に加えて ChaCha20-Poly1305 を選択可能にする。
両アルゴリズムとも鍵長 32 バイト・nonce 12 バイト・認証タグ 16 バイトで一致するため、
`aether` の内部フォーマット（Header 構造・チャンク分割・CounteredNonce）は **ほぼ変更不要**。

### 現状の構造（aether クレート）

- `Cipher` 構造体が `Aes256Gcm` を直接保持
- ファイルフォーマット: `[Header 32B][encrypted chunks...]`
- Header: `magic(2B) + flags(2B) + iv/nonce(12B) + integrity(16B)`
- `flags` フィールドは現在 **常に 0** — アルゴリズム識別に利用可能

### 変更箇所

1. **aether/Cargo.toml** — 依存追加
   ```toml
   chacha20poly1305 = "0.10"   # aes-gcm と同じ RustCrypto AEAD API
   ```

2. **aether/src/lib.rs** — AEAD 抽象化
   - `CipherAlgorithm` enum を新設:
     ```rust
     pub enum CipherAlgorithm { Aes256Gcm, ChaCha20Poly1305 }
     ```
   - `Cipher` 内部を `Box<dyn AeadInPlace>` または enum ディスパッチに変更:
     ```rust
     enum AeadInner { Aes(Aes256Gcm), ChaCha(ChaCha20Poly1305) }
     ```
   - `chacha20poly1305` クレートは `aes_gcm` と同じ `aead` trait
     (`Aead`, `AeadCore`, `KeyInit`) を実装するため、差し替えは機械的
   - `Key<Aes256Gcm>` → `&[u8; 32]` に一般化（両方 32 バイト鍵）

3. **aether Header `flags`** — アルゴリズム識別
   - `flags` の下位 1 ビットで暗号アルゴリズムを記録:
     - `0b0000_0000_0000_0000` = AES-256-GCM（既存データと後方互換）
     - `0b0000_0000_0000_0001` = ChaCha20-Poly1305
   - 復号時は `flags` を読みアルゴリズムを自動判別 → 鍵さえあれば透過的に復号

4. **tome-store/src/encrypted.rs** — `EncryptedStorage` 拡張
   - `EncryptedStorage` にアルゴリズム選択フィールドを追加:
     ```rust
     pub struct EncryptedStorage<S> {
         inner: S,
         key: [u8; 32],
         algorithm: CipherAlgorithm,  // 新規
     }
     ```
   - 暗号化時にアルゴリズムを `Cipher` に渡す
   - 復号時は Header の `flags` から自動判別するため指定不要

5. **tome-cli** — CLI オプション
   - `tome store copy --encrypt --cipher chacha20` で選択
   - デフォルトは `aes256gcm`（後方互換）

6. **stores.config** — ストア単位のデフォルト暗号（オプショナル）
   - `stores.config` JSON に `"cipher": "chacha20-poly1305"` を格納可
   - CLI の `--cipher` が明示されればそちらが優先

### 変更不要な箇所

| 箇所 | 理由 |
|------|------|
| Header サイズ (32B) | nonce(12B)・integrity(16B) は両アルゴリズムで同一 |
| `CounteredNonce` | nonce 12 バイト — 両方同じ |
| チャンク暗号化ロジック | AEAD trait が共通。認証タグ 16B も同一 |
| `BUFFER_SIZE` / チャンクサイズ | アルゴリズム非依存 |
| `replicas.encrypted` | bool のまま（アルゴリズムは blob 内 Header に記録） |
| DB スキーマ | 変更なし |
| Argon2id 鍵導出 | 出力 32 バイト — 両アルゴリズムで共用可 |

### 制約・注意事項

- **復号はアルゴリズム自動判別** — Header `flags` を読むだけなので、混在保存でも問題なし
- **既存の暗号化ファイル** — `flags == 0` は AES-256-GCM として解釈（後方互換）
- ChaCha20-Poly1305 は AES-NI 非搭載環境（ARM 等）で AES-256-GCM より高速
- AES-NI 搭載環境では AES-256-GCM の方が高速なため、デフォルトは AES-256-GCM を維持
- `chacha20poly1305` クレートは `aes-gcm` と同じ RustCrypto プロジェクト — API 互換

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

### Phase 4: aether の AEAD 抽象化 【優先度: 低 — ChaCha20-Poly1305 導入時に実施】

ChaCha20-Poly1305 導入方針（上記セクション参照）と同時に実施。
`Cipher` 内部を enum ディスパッチに変更する際に、テストも各アルゴリズムでパラメタライズ。

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
| 中 | ChaCha20-Poly1305 導入（aether）— 上記の ChaCha20-Poly1305 導入方針を参照 |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | PostgreSQL 中央同期 — 複数マシンが一つの PostgreSQL に push/pull（現在は SQLite↔SQLite のみ） |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
