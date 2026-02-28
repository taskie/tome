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

## リファクタリング方針（実装済み）

### Phase 1: tome-db/src/ops.rs 分割 ✅

~1040 行・60+ 関数を `tome-db/src/ops/` 以下 11 モジュールに分割済み。
`mod.rs` の `pub use` で既存の `use tome_db::ops::*` パスを維持。

### Phase 2: tome-server/src/routes.rs 分割 ✅

~670 行を `tome-server/src/routes/` 以下 6 モジュールに分割済み。
`find_repo_or_404()` ヘルパーでリポジトリ取得 + 404 処理を共通化。

### Phase 3: tome-cli コマンドの共通化 ✅

`helpers.rs` に `resolve_store()` / `resolve_scan_root()` を抽出し、
store.rs（4 箇所）と verify.rs（1 箇所）の重複を解消。

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

`POST /machines` で中央サーバが未使用 machine_id を自動割当。
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

### 現状の課題と拡張計画

#### 課題 1: ワークフローが多段

現在のフルサイクルは `scan → store push → sync push` の 3 コマンド。
毎回手動で実行するのは煩雑であり、順序を間違えると replica 情報が欠落する。

**拡張: `tome push` 統合コマンド**

```bash
tome push central                  # scan + store push + sync push を一括実行
tome push central --scan-only      # scan + sync push（blob 転送はスキップ）
tome push central --no-scan        # store push + sync push（再スキャンなし）
```

内部フロー:
1. `scan` — ローカル SQLite にスナップショット記録
2. `store push` — 新規 blob を S3 にアップロード
3. `sync push` — メタデータ（snapshot, entry, blob, replica）を Aurora に push

逆方向も同様に `tome pull central` で `sync pull + store copy`（必要な blob のみ）。

#### 課題 2: DB 直接接続の前提

`sync push/pull` は peer の DB に SeaORM で直接接続する。PostgreSQL をインターネットに
公開するのはセキュリティリスクが高く、VPN/SSHトンネル必須の運用になる。

**拡張: HTTP sync API**

`tome serve` に sync エンドポイントを追加し、DB 直接接続を不要にする:

```
POST /sync/push    — クライアントが snapshot + entries + blobs + replicas を JSON で送信
GET  /sync/pull    ?repo=<name>&after=<snapshot_id>  — 差分取得
POST /sync/register-machine  — machine_id 払い出し（既存 POST /machines の統合）
```

これにより `sync push/pull` の接続先を DB URL と HTTP URL の両方に対応させる:

```bash
# DB 直接接続（LAN / VPN 内）
tome sync add central "postgres://user:pass@aurora/tome"

# HTTP 経由（インターネット越し・Cloudflare Access 等で保護）
tome sync add central "https://tome.example.com"
```

`sync.rs` 内の分岐: URL スキームが `http://` / `https://` なら HTTP クライアント、
それ以外なら従来の DB 直接接続を使用。

#### 課題 3: entry_cache の不整合

`sync pull` は entries のみ取り込み、entry_cache を部分的にしか更新しない。
pull 後にファイル一覧画面（entry_cache 依存）が正しく表示されないケースがある。

**拡張: entry_cache 再構築コマンド**

```bash
tome cache rebuild [--repo <name>]
```

全 entry を走査し、各パスの最新状態を entry_cache に再投入する。
`sync pull` 完了時にも自動実行するオプション `--rebuild-cache` を追加。

ops に `rebuild_entry_cache(db, repository_id)` を追加:
1. 全 snapshot を古い順に走査
2. 各 entry でパスの最新状態を HashMap に蓄積
3. entry_cache テーブルを UPSERT

#### 課題 4: コンフリクト検知

複数マシンが同一 repo に `sync push` すると、中央 DB 上の snapshot parent_id チェーンが
分岐する。現状は last-writer-wins で暗黙的にマージされ、ユーザに通知されない。

**拡張: 分岐検知と警告**

`sync push` 時に中央の `latest_snapshot` を取得し、ローカルの `last_snapshot_id`
（前回 push 時に記録）と比較。不一致なら他マシンが push 済みであることを検知:

```
warning: central has diverged — snapshot <id> was pushed by machine 3 since your last sync.
         Your push will create a branched history.
         Run `tome sync pull central` first to incorporate remote changes.
```

snapshot.metadata に `source_machine_id` があるので、誰が push したか表示可能。
マージ戦略の実装は当面不要 — 警告のみで十分（ファイル単位の衝突はない設計）。

#### 課題 5: 復元パスの欠如

中央 DB に履歴があり S3 に blob 実体があるが、`tome restore` が未実装。

**拡張: `tome restore`**

```bash
# 最新の状態に復元
tome restore --repo myproject /path/to/dest

# 特定スナップショットの状態に復元
tome restore --snapshot <id> /path/to/dest

# 特定ファイルのみ復元
tome restore --snapshot <id> --path "docs/readme.md" /path/to/dest

# 復元前の到達可能性チェック（ダウンロードなし）
tome restore --check --snapshot <id>
```

内部フロー:
1. snapshot の entries を取得（status=1 のみ）
2. 各 entry の blob_id → replicas テーブルで store を特定
3. store 優先順位: ローカル file:// > SSH > S3（レイテンシ順）
4. `storage.download()` で blob を取得し、entry.path に配置
5. 暗号化 replica の場合は `EncryptedStorage` 経由で復号

#### 課題 6: 選択的同期

全ファイルの同期は帯域・ストレージを圧迫する。特定のパスプレフィックスのみ
同期したいケースがある（例: `docs/` のみ、`*.log` を除外）。

**拡張: sync フィルタ**

```bash
tome sync add central <url> --repo default --include "src/**" --exclude "*.log"
```

sync_peers.config にフィルタルールを保存:
```json
{
  "peer_repo": "default",
  "include": ["src/**"],
  "exclude": ["*.log"]
}
```

`sync push/pull` 時に `entries_in_snapshot` の結果をフィルタルールでふるいにかける。
glob マッチは `ignore` クレートの `OverrideBuilder` を流用。

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

## セキュリティ方針

tome は**ローカルファーストの個人ツール**であり、tome-server / tome-web にアプリ層の認証は実装しない。
アクセス制御は外部インフラに委ねる。

| 運用形態 | 推奨手段 |
|----------|---------|
| 個人ローカル | `127.0.0.1` バインド（デフォルト）— 外部からアクセス不可 |
| LAN 共有 | ファイアウォールで LAN 内 IP のみ許可 |
| リモートアクセス | WireGuard / Tailscale 等の VPN 経由 |
| クラウド公開 | Cloudflare Access / AWS ALB + OIDC でインフラ層保護 |

アプリ層に認証・RBAC を実装することは現時点でスコープ外とする。

---

## 残タスク

> **方針: 個人ツールとしての完成度向上。認証・RBAC はスコープ外（外部インフラで代替）。**

| 優先度 | 内容 |
|--------|------|
| 高 | `tome push` / `tome pull` 統合コマンド — scan + store push + sync push を一括実行 |
| 高 | `tome restore` — snapshot + replica 情報から store 経由でファイルを復元 |
| 高 | `GET /diff` 削除ファイル除外バグ修正 — `blob_id = NULL` のエントリを diff 結果に含める |
| 高 | `path_history` API の digest 欠落修正 — `From<entry::Model>` が blob を JOIN せず `digest: null` を返す |
| 高 | Watch モード（`tome watch`）— inotify/fanotify/kqueue でバックグラウンド監視し自動スナップショット |
| 中 | HTTP sync API — `tome serve` に `/sync/push`, `/sync/pull` を追加し DB 直接接続を不要にする |
| 中 | entry_cache 再構築 — `tome cache rebuild` + sync pull 後の自動再構築オプション |
| 中 | sync push 時のコンフリクト検知 — 中央 DB の分岐を検出し警告 |
| 中 | sync フィルタ — `--include` / `--exclude` でパスを絞った選択的同期 |
| 中 | 重複レポート（`tome dedup`）— blob の content-addressing を活かしリポジトリ横断で重複ファイルを報告 |
| 中 | Webhook / 通知 — スキャン完了・変更検知時に変更サマリを POST（Slack, Discord, 汎用 HTTP） |
| 中 | `tome restore --check` — 復元前に blob の replica 存在確認（store の到達可能性チェック） |
| 低 | 鍵ローテーション — Header 拡張 + `store reencrypt` コマンド |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
