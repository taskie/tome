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
tome-cli/     — 統一 CLI（scan / store / sync / diff / restore / tag / verify / gc / init / serve）
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
- **Sonyflake machine_id** — u16 (0–65535) だが PostgreSQL SMALLINT は i16 (-32768–32767)。
  実運用では 0–32767 の範囲で使用。エンティティは `i16` で定義

---

## 実装済み機能

以下は実装済み。設計判断の記録として残す。

### BLAKE3 対応

SHA-256 の代わりに BLAKE3 を選択可能。リポジトリ単位で `repositories.config["digest_algorithm"]` に記録。
`tome scan --digest-algorithm blake3` で指定。treblo クレートにハッシュインフラを集約
（`DigestAlgorithm` enum, `hash_file()`, `Digest256` 型）。

### ChaCha20-Poly1305

AES-256-GCM に加えて ChaCha20-Poly1305 を選択可能。aether 内部は `AeadInner` enum ディスパッチ。
Header `flags` 下位 1 ビットでアルゴリズム識別（0 = AES, 1 = ChaCha）。復号時は自動判別。
`tome store copy --encrypt --cipher chacha20-poly1305` で選択。

### 設定ファイル tome.toml

`~/.config/tome/tome.toml`（グローバル）と `./tome.toml`（プロジェクトローカル）。
優先順位: デフォルト → グローバル → ローカル → 環境変数 → CLI 引数。

### PostgreSQL 中央同期インフラ

`machines` テーブル（migration 010）+ `snapshots.source_machine_id/source_snapshot_id`（migration 011）。
`tome init --server <url>` で machine_id 払い出し。`sync push` / `sync pull` 双方向 + replicas 同期。

---

## リファクタリング方針

### Phase 1: tome-db/src/ops.rs 分割 【優先度: 高】

~1040 行・60+ 関数を機能ドメインごとにモジュール分割する。

```
tome-db/src/ops/
  mod.rs          — pub use で再エクスポート（既存の use パスを維持）
  repository.rs   — get_or_create_repository, *_digest_algorithm
  blob.rs         — get_or_create_blob, find_blob_by_*
  snapshot.rs     — create_snapshot, create_snapshot_with_source, latest_snapshot
  entry.rs        — insert_entry_*, entries_*
  entry_cache.rs  — upsert_cache_*, present_cache_entries
  store.rs        — get_or_create_store, find_store_by_*
  replica.rs      — replica_exists, insert_replica, replicas_*
  sync_peer.rs    — insert_sync_peer, find/list/update_sync_peer
  machine.rs      — register_machine, list_machines, find/update_machine
  tag.rs          — upsert_tag, delete_tags, list_tags
  gc.rs           — unreferenced_blobs, delete_*
```

### Phase 2: tome-server/src/routes.rs 分割 【優先度: 中】

~580 行を構造化。リポジトリ取得 + 404 処理を共通ヘルパー `find_repo_or_404()` に抽出。

### Phase 3: tome-cli コマンドの共通化 【優先度: 中】

store.rs の重複解消（ストア解決・進捗カウンタ）、scan.rs の process_file 分解。

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

### 2層同期

| 層 | コマンド | 転送内容 | 接続先 |
|----|----------|----------|--------|
| メタデータ | `sync push` / `sync pull` | snapshots, entries, blobs (行), replicas | Aurora PostgreSQL |
| blob 実体 | `store push` / `store copy` | 暗号化ファイル | S3 |

### machine_id 払い出し

`POST /api/machines/register` で中央サーバが未使用 machine_id を自動割当。
`tome init --server <url>` で取得し `~/.config/tome/tome.toml` に書き込み。
`machine_id = 0` はローカル専用として予約。

### repository の名前空間

同一 repo 名は中央で統合（案 B）。blob は content-addressed で自然に dedup。
snapshots は `source_machine_id` で識別し、各マシンの parent_id チェーンが独立して共存。

### 日常オペレーション

```bash
tome sync add central "postgres://user:pass@aurora.example.com/tome" --repo default
tome store add central-s3 "s3://my-tome-bucket/store"

tome sync push central             # メタデータ → Aurora
tome store push --store central-s3 # blob 実体 → 暗号化 S3
tome sync pull central             # メタデータ ← Aurora
tome store copy central-s3 local   # blob 実体 ← S3（必要時）
```

### 制約・注意事項

- S3 の blob は **常に暗号化** — 平文 blob を S3 に置かない方針
- ネットワーク越しの大量 insert はトランザクションバッチ化が必要（100件/txn 等）
- Aurora のセキュリティグループで接続元 IP を制限し、SSL 接続を強制すること

---

## 暗号鍵管理方針

### 現状

- 暗号鍵は 32 バイトのバイナリファイル（`~/.config/tome/keys/*.key`）
- `tome.toml` の `store.key_file` でパスを指定
- 鍵は各マシンのローカルファイルシステムに置く（帯域外で配布）
- 中央（Aurora/S3）には鍵を保存しない（知識分離）

### 課題

中央集権構成でマシン台数が増えると以下の問題が生じる:

1. **鍵配布の手間** — 新しいマシンに手動で鍵ファイルをコピーする運用は煩雑
2. **鍵ローテーション** — 全マシンの鍵を同時に更新する仕組みがない
3. **監査** — どのマシンがいつ鍵にアクセスしたか追跡できない
4. **失効** — マシンの撤去時に鍵アクセスを即座に取り消せない

### 方針: 外部シークレットマネージャとの統合

`tome.toml` に `store.key_source` フィールドを新設し、鍵の取得元を指定する:

```toml
[store]
# 方式 1: ローカルファイル（現行・デフォルト）
key_file = "~/.config/tome/keys/central.key"

# 方式 2: AWS Secrets Manager
key_source = "aws-secrets-manager://tome/encryption-key"

# 方式 3: HashiCorp Vault
key_source = "vault://secret/data/tome/encryption-key"

# 方式 4: 環境変数（CI/CD 向け）
key_source = "env://TOME_ENCRYPTION_KEY"
```

### AWS Secrets Manager 連携

```
tome-store/src/key.rs (新規)
  pub async fn resolve_key(config: &StoreConfig) -> Result<[u8; 32]>
    // key_file があればローカルファイルから読む（既存動作）
    // key_source があれば URL スキームで分岐
```

**実装案:**
- `aws-sdk-secretsmanager` クレートで `GetSecretValue` API を呼ぶ
- シークレットの値は Base64 エンコードした 32 バイト鍵
- IAM ポリシーでマシン（EC2 Instance Role / ECS Task Role）ごとにアクセス制御
- 鍵ローテーション: Secrets Manager のバージョニング機能を利用。
  複数バージョンを保持し、復号時は Header のバージョンヒントで旧鍵も試行

**Secrets Manager のメリット:**
- IAM で「このマシンだけがこの鍵を取得できる」を宣言的に管理
- CloudTrail で鍵アクセスの全ログが残る（監査）
- マシン撤去時は IAM ポリシーを外すだけで即座に失効
- Secrets Manager のローテーション Lambda で自動鍵更新が可能

### HashiCorp Vault 連携

**実装案:**
- `reqwest` で Vault HTTP API (`GET /v1/secret/data/tome/encryption-key`) を呼ぶ
- 認証: AppRole（サーバ向け）/ Token（開発向け）
- Vault Token は `VAULT_TOKEN` 環境変数 or `~/.vault-token` から取得
- Vault のリース機構で一時的な鍵アクセスを制御

### 環境変数方式 (CI/CD)

```bash
export TOME_ENCRYPTION_KEY=$(base64 -d < /path/to/key)
```

- GitHub Actions / GitLab CI のシークレット機能と連携
- `key_source = "env://TOME_ENCRYPTION_KEY"` で環境変数名を指定
- 値は Base64 エンコードされた 32 バイト（decode して `[u8; 32]` に変換）

### 鍵ローテーション戦略

1. **新規暗号化は常に最新鍵** — `store push` / `store copy --encrypt` は最新鍵を使用
2. **復号時は鍵バージョンを自動判別** — aether Header に鍵バージョンヒント（2 バイト）を追加検討。
   当面は「最新鍵で復号を試行し、失敗したら旧鍵を順に試す」フォールバック方式
3. **旧鍵の blob 再暗号化** — `tome store reencrypt <store>` コマンドを将来追加
   （全 blob を旧鍵で復号→新鍵で暗号化→replicas 更新）

### 実装ステップ

1. `tome-store/src/key.rs` に `resolve_key()` を実装（`key_file` / `env://` の 2 方式から開始）
2. `StoreConfig` に `key_source: Option<String>` を追加
3. `EncryptedStorage` の構築時に `resolve_key()` を呼ぶよう変更
4. AWS Secrets Manager 連携を追加（feature flag `secrets-manager`）
5. HashiCorp Vault 連携を追加（feature flag `vault`）
6. 鍵ローテーション対応（Header 拡張 + `store reencrypt` コマンド）

---

## Web UI 認可方針

### 現状

- tome-server は **認証・認可なし** — 全エンドポイントが無制限にアクセス可能
- ローカル開発（`127.0.0.1:8080`）前提のため問題なかったが、
  中央 Aurora 構成では tome-server がインターネットに露出する可能性がある

### 脅威モデル

| 脅威 | 影響 | 現状の対策 |
|------|------|------------|
| メタデータの不正閲覧 | ファイルパス・ハッシュ・スナップショット履歴が漏洩 | なし |
| machine_id の不正登録 | 他マシンの ID 空間を奪取し、ID 衝突を引き起こす | なし |
| メタデータの改ざん | 偽の snapshot/entry を push し、他マシンの pull を汚染 | なし |
| blob 実体へのアクセス | S3 は暗号化済みだが、メタデータ経由でパスが判明 | aether 暗号化 |

### 方針: 多層防御

#### 層 1: ネットワーク制限（最優先）

- Aurora: セキュリティグループで接続元 IP を制限
- tome-server: VPN / VPC 内でのみ公開（パブリック Internet に直接公開しない）
- S3: バケットポリシーで VPC エンドポイント経由のみ許可

#### 層 2: API 認証（トークンベース）

tome-server に Bearer Token 認証を導入する:

```
Authorization: Bearer <token>
```

**設計:**
- `machines` テーブルに `api_token_hash` カラムを追加
  （bcrypt or SHA-256 ハッシュ。平文トークンは DB に保存しない）
- `tome init --server <url>` 時にトークンを発行し、
  `~/.config/tome/tome.toml` に `server.token` として保存
- Axum の middleware (extractor) で全 `/api/*` エンドポイントを保護

```toml
# tome.toml
[server]
url = "https://central.example.com"
token = "tm_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

**実装案:**

```rust
// tome-server/src/auth.rs (新規)
pub struct AuthenticatedMachine {
    pub machine_id: i16,
    pub name: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedMachine
where
    S: Send + Sync,
    DatabaseConnection: FromRef<S>,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Authorization ヘッダから Bearer token を抽出
        // 2. DB で token_hash を照合
        // 3. machine レコードを返す（last_seen_at を更新）
    }
}
```

**エンドポイント保護レベル:**

| エンドポイント | 認証 | 理由 |
|---------------|------|------|
| `GET /health` | 不要 | ヘルスチェック |
| `GET /repositories/*` | 必要 | メタデータ閲覧 |
| `GET /blobs/*` | 必要 | メタデータ閲覧 |
| `POST /api/machines/register` | 特別 | 下記参照 |
| `PUT /api/machines/*` | 必要 | マシン更新 |

**machine 登録の認証:**
- 初回登録（`POST /api/machines/register`）は「登録用シークレット」で保護
- `tome-server` 起動時に `--registration-secret` or 環境変数 `TOME_REGISTRATION_SECRET` で指定
- `tome init --server <url> --secret <registration-secret>` で初回登録

#### 層 3: Web UI 認証（OAuth2 / OIDC）

tome-web (Next.js) に認証を追加する。中央公開時に必要:

**案 A: NextAuth.js + GitHub OAuth**
- tome-web のサーバーサイドで GitHub OAuth2 を処理
- 許可するユーザ/org を `tome-web/env.local` で設定
- tome-server API 呼び出しは tome-web のサーバーサイドから行い、
  クライアントブラウザは直接 tome-server にアクセスしない（現行設計を維持）

**案 B: Cloudflare Access / AWS ALB 認証**
- インフラ層で認証を処理（アプリ改修不要）
- Cloudflare Zero Trust or ALB + Cognito で OIDC 認証
- 認証済みユーザのみ tome-web / tome-server にアクセス可能

**推奨: 案 A + 案 B の併用**
- 案 B でネットワーク層の保護（全体）
- 案 A で細粒度のアクセス制御（将来: リポジトリ単位の閲覧権限等）

#### 層 4: RBAC（将来）

リポジトリ単位の権限管理:

```
machines_roles テーブル:
  machine_id, repository_id, role (reader / writer / admin)
```

- `reader`: sync pull / store copy のみ
- `writer`: scan + sync push + store push
- `admin`: 全操作 + gc + リポジトリ削除

### 実装ステップ

1. **層 1: ネットワーク制限** — ドキュメントとデプロイガイドで対応（コード変更なし）
2. **層 2: API トークン認証** — `machines.api_token_hash` + Axum middleware
3. **層 2: 登録用シークレット** — `POST /api/machines/register` の保護
4. **層 3: Web UI 認証** — NextAuth.js + GitHub OAuth（tome-web 側のみ）
5. **層 4: RBAC** — `machines_roles` テーブル + 権限チェック middleware

---

## 残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | リファクタリング Phase 1–3 — 上記のリファクタリング方針を参照 |
| 高 | API トークン認証（層 2）— machines.api_token_hash + Axum middleware |
| 中 | 暗号鍵管理 — key_source による外部シークレットマネージャ統合 |
| 中 | Web UI 認証（層 3）— NextAuth.js + GitHub OAuth |
| 中 | Watch モード（`tome watch`） — inotify/fsevents で監視し自動スナップショット |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | RBAC（層 4）— リポジトリ単位の権限管理 |
| 低 | 鍵ローテーション — Header 拡張 + `store reencrypt` コマンド |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
