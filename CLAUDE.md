# tome — 開発ガイド

> Rust 製ファイル変更追跡システム。ハッシュ（SHA-256 + xxHash64）で変更を検知し、
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
cargo clippy -p tome-core -p tome-db -p tome-store -p tome-server -p tome-cli -p treblo -p treblo-cli -p aether -p aether-cli

# tome-web — フォーマット & lint
cd tome-web && npm run format && npm run lint
```

---

## クレート構成

```
tome-core/    — ハッシュ計算・ID生成・共通モデル
tome-db/      — SeaORM エンティティ + マイグレーション + ops
tome-store/   — ファイルストレージ抽象化（Local / SSH / S3 / 暗号化）
tome-server/  — HTTP API サーバー (axum)
tome-cli/     — 統一 CLI（tome scan / store / sync / serve）
tome-web/     — Next.js 16 Web フロントエンド
```

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

## BLAKE3 導入方針

SHA-256 の代わりに BLAKE3 を選択可能にする。BLAKE3 はデフォルト出力が 32 バイトのため、
`Digest256` 型・DB スキーマ (`VarBinary` / BLOB)・ストレージパス (`objects/xx/yy/hex`)
はすべて **変更不要**。

### 変更箇所

1. **tome-core/src/hash.rs** — ハッシュ計算の抽象化
   - `blake3` クレートを `Cargo.toml` に追加（feature flag `blake3`）
   - `DigestAlgorithm` enum (`Sha256`, `Blake3`) を新設
   - `hash_file()` / `sha256_reader()` 等を `DigestAlgorithm` でディスパッチ
   - 公開 API 例:
     ```rust
     pub enum DigestAlgorithm { Sha256, Blake3 }
     pub fn hash_file(path: &Path, algo: DigestAlgorithm) -> io::Result<FileHash>
     ```
   - `Digest256` 型名は `ContentDigest` 等にリネームしてもよい（BLAKE3 も 32 バイト）

2. **tome-db: `repositories.config`** — リポジトリ単位でアルゴリズム選択
   - 既存の JSON `config` カラムに `"digest_algorithm": "sha256" | "blake3"` を格納
   - デフォルトは `"sha256"`（後方互換）
   - `tome-db/src/ops.rs` の `get_or_create_blob` / scan 系処理で config を参照

3. **tome-cli** — CLI オプション
   - `tome scan --digest-algorithm blake3` でリポジトリ作成時に設定
   - 既存リポジトリはアルゴリズム変更不可（digest の一貫性保持）

4. **tome-core/Cargo.toml** — feature gate
   ```toml
   [features]
   default = []
   blake3 = ["dep:blake3"]

   [dependencies]
   blake3 = { version = "1", optional = true }
   ```

### 変更不要な箇所

| 箇所 | 理由 |
|------|------|
| DB スキーマ (`blobs.digest`) | `VarBinary(None)` — 長さ非固定、32 バイトのまま |
| `entry_cache.digest` | 同上（非正規化コピー） |
| `blob_path()` | hex 文字列ベース — アルゴリズム非依存 |
| `hex_encode()` | バイト列→hex — 汎用 |
| `find_blob_by_hex()` | `prefix_bytes.len() == 32` の判定は BLAKE3 でも同じ |
| HTTP API レスポンス | hex 文字列を返すだけ — 変更不要 |
| `tome-web` | API レスポンスを表示するだけ — 変更不要 |

### 制約・注意事項

- **同一リポジトリ内でアルゴリズム混在は禁止** — digest の比較が壊れる
- **既存リポジトリのアルゴリズム変更は不可** — 全 blob の再ハッシュが必要になるため
- `blake3` クレートは `no_std` 対応・SIMD 自動検出で高速（SHA-256 比 5〜15 倍）
- feature flag でオプショナルにし、BLAKE3 不要な環境ではバイナリサイズを抑える

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

## 設定ファイル (tome.toml) 導入方針 【優先度: 高】

現在すべての設定が CLI 引数・環境変数・ハードコードで管理されている。
`tome.toml` を導入し、繰り返し指定する設定を永続化する。

### 読み込み優先順位（後勝ち）

```
1. デフォルト値（コード内ハードコード）
2. ~/.config/tome/tome.toml   — グローバル設定
3. ./tome.toml                — プロジェクトローカル設定
4. 環境変数 (TOME_*)
5. CLI 引数 (--db, --machine-id, ...)
```

上位ソースが下位を上書きする。部分指定可（未指定キーはフォールバック）。

### 設定ファイルの形式

```toml
# ~/.config/tome/tome.toml (グローバル) または ./tome.toml (プロジェクト)

# --- 基本設定 ---
db = "tome.db"                    # DB パス or URL (現: --db / TOME_DB)
machine_id = 0                    # Sonyflake ID (現: --machine-id / TOME_MACHINE_ID)

# --- scan ---
[scan]
repo = "default"                  # デフォルトリポジトリ名 (現: --repo)
no_ignore = false                 # .gitignore 無視 (現: --no-ignore)

# --- store ---
[store]
default_store = "backup"          # デフォルトストア名
key_file = "~/.config/tome/keys/main.key"  # 暗号化鍵 (現: --key-file)
cipher = "aes256gcm"              # 暗号アルゴリズム (将来: ChaCha20-Poly1305 方針参照)

# --- serve ---
[serve]
addr = "127.0.0.1:8080"          # リッスンアドレス (現: --addr)
```

### 変更箇所

1. **tome-cli/Cargo.toml** — 依存追加
   ```toml
   toml = "0.8"
   serde = { version = "1", features = ["derive"] }
   dirs = "6"        # XDG 準拠パス解決（現在の手動 HOME 参照を置換）
   ```

2. **tome-cli/src/config.rs** — 新規: 設定読み込みロジック
   - `TomeConfig` 構造体を `#[derive(Default, Deserialize)]` で定義
   - 読み込み関数:
     ```rust
     pub fn load_config() -> TomeConfig {
         let mut config = TomeConfig::default();
         // 1. ~/.config/tome/tome.toml
         if let Some(global) = dirs::config_dir() {
             merge_file(&mut config, global.join("tome/tome.toml"));
         }
         // 2. ./tome.toml
         merge_file(&mut config, PathBuf::from("tome.toml"));
         config
     }
     ```
   - `merge_file()` は TOML をパースし、`None` でないフィールドのみ上書き
   - 各フィールドは `Option<T>` で定義し、未指定を明示的に区別

3. **tome-cli/src/main.rs** — clap との統合
   - `main()` 冒頭で `load_config()` を呼び出し
   - clap の `default_value` を設定ファイル値で動的に差し替え:
     ```rust
     let config = config::load_config();
     let cli = Cli::parse();
     // CLI > env > config > default の順で解決
     let db = cli.db_or(config.db.as_deref().unwrap_or("tome.db"));
     ```
   - 環境変数は clap の `env = "TOME_*"` がそのまま処理

4. **tome-store/src/factory.rs** — `key_dir()` 改善
   - `dirs::config_dir()` を使用（`HOME` 直参照を廃止）
   - `tome.toml` の `store.key_file` をデフォルトパスとして参照

### 変更不要な箇所

| 箇所 | 理由 |
|------|------|
| tome-core | 設定に依存しない純粋な計算ライブラリ |
| tome-db | DB 接続 URL は tome-cli から渡される |
| tome-server | addr は tome-cli から渡される |
| tome-store | factory 以外は URL/key を引数で受け取るだけ |
| tome-web | 独立した Next.js アプリ（TOME_API_URL で設定済み） |

### 設計判断

- **toml クレート直接利用** — figment / config クレートは多機能すぎる。
  TOML パース + 手動マージで十分（設定項目が少ない）
- **`./tome.toml`（ドットなし）** — `.gitignore` が `.*` をブロックする環境との互換。
  DB ファイルも `tome.db` でドットなし命名に統一済み
- **`~` 展開** — `key_file` 等のパスで `~` を `dirs::home_dir()` に展開する
- **XDG 準拠** — `dirs` クレートで `$XDG_CONFIG_HOME` を尊重
  （未設定時は `~/.config`）
- **サブコマンド固有設定** — `[scan]`, `[store]`, `[serve]` セクションで分離。
  将来のサブコマンド追加時もトップレベルが汚れない

### 制約・注意事項

- **tome.toml に機密情報を書かない** — DB パスワード・暗号化鍵はファイルパス参照のみ
- **設定ファイルは任意** — なくても全機能が動く（既存動作を壊さない）
- **`--config <PATH>` オプション** は将来追加可（初期実装では不要）

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

### Phase 5: レガシークレート整理 【優先度: 低】

以下は tome-* 系への移行完了後に削除またはアーカイブ:

```
ichno/          — 旧 SQLite 実装
ichno_cli/      — 旧 CLI
ichnome/        — 旧 PostgreSQL 実装
ichnome_cli/    — 旧 CLI
ichnome_web/    — 旧 Web (Rust)
ichnome_web_front/ — 旧フロントエンド
treblo/         — 旧ライブラリ
treblo_cli/     — 旧 CLI
optional_derive/ — proc macro（tome-* で未使用なら削除）
aether_cli/     — aether の CLI ラッパー（tome-cli に統合済みか確認）
```

**手順:** `Cargo.toml` の `[workspace] members` から除外 → 別ブランチにアーカイブ

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
| 中 | Watch モード（`tome watch`） — inotify/fsevents で監視し自動スナップショット |
| 中 | ChaCha20-Poly1305 導入（aether）— 上記の ChaCha20-Poly1305 導入方針を参照 |
| 中 | ChaCha20-Poly1305 導入 — 上記の ChaCha20-Poly1305 導入方針を参照 |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | PostgreSQL 中央同期 — 複数マシンが一つの PostgreSQL に push/pull（現在は SQLite↔SQLite のみ） |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
