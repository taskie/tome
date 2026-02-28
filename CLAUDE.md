# tome — 開発ガイド

> Rust 製ファイル変更追跡システム。ハッシュ（SHA-256 + xxHash64）で変更を検知し、
> スナップショット履歴を SQLite/PostgreSQL に記録する。

**著者:** taskie <t@skie.jp>
**ブランチ:** feature/tome（再設計実装中）
**コミットメッセージは英語で書くこと**

### コミット前に必ず実行すること

```bash
# Rust — フォーマット & lint
cargo fmt --all
cargo clippy -p tome-core -p tome-db -p tome-store -p tome-server -p tome-cli

# tome-web — フォーマット & lint
cd tome-web && npm run format && npm run lint
```

---

## 1. クレート構成

```
tome-core/    — ハッシュ計算・ID生成・共通モデル
tome-db/      — SeaORM エンティティ + マイグレーション + ops
tome-store/   — ファイルストレージ抽象化（Local / SSH / S3 / 暗号化）
tome-server/  — HTTP API サーバー (axum)
tome-cli/     — 統一 CLI（tome scan / store / sync / serve）
tome-web/     — Next.js 15 Web フロントエンド
```

tome-sync は独立クレートとして切り出さず、`tome-cli/src/commands/sync.rs` に実装。

---

## 2. 設計原則

1. **Single Source of Truth** — 各情報は一箇所のみ。キャッシュは名前で明示（`entry_cache`）
2. **ローカルファースト** — SQLite を第一級市民。サーバー DB は同期先のひとつ
3. **イベントソーシング** — 変更は snapshot として記録。現在状態は snapshot からの導出
4. **ストレージ管理の内部化** — ファイル実体の所在を replicas テーブルで把握
5. **暗号化の組み込み** — aether (AES-256-GCM + Argon2id) を store 層に統合

---

## 3. DB スキーマ

9 テーブル、全 ID は Sonyflake (i64)、日時は `DateTimeWithTimeZone`。

| テーブル | 概要 |
|---------|------|
| repositories | スキャン対象の名前空間（`default` 等） |
| blobs | コンテンツアドレッサブルなファイル指紋（digest=SHA-256, fast_digest=xxHash64） |
| snapshots | スキャン実行イベント（Git コミット相当、parent_id で連鎖） |
| entries | スナップショット内のファイル状態（status: 0=deleted, 1=present） |
| entry_cache | 各パスの最新状態キャッシュ（PK: repository_id + path） |
| stores | ストレージ定義（url: `file:///`, `ssh://`, `s3://`） |
| replicas | blob がどの store にあるかの所在管理 |
| tags | blob へのキーバリュー属性 |
| sync_peers | 同期ピア（url + last_snapshot_id） |

---

## 4. ハッシュ戦略

変更検知は三段階フィルタで高速化:

```
mtime/size 比較 → xxHash64 比較 → SHA-256 比較
```

各段階で変化がなければ後続のハッシュ計算をスキップする。
ハッシュは `tome-core/src/hash.rs` の `hash_file()` で一回のファイル読み込みで両方計算。

---

## 5. 暗号化

aether クレート (AES-256-GCM + Argon2id) を `EncryptedStorage<S>` でラップする。

```
~/.config/tome/keys/
  <key_id>.key       — 32 バイトのバイナリ鍵
```

`tome store copy --encrypt --key-file <path>` でコピー時に暗号化。
`EncryptedStorage` は `tome-store/src/encrypted.rs` に実装済み。

---

## 6. ストレージのファイルパス

blob の保存パスは `blob_path()` で決定（`tome-store/src/storage.rs`）:

```
objects/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

---

## 7. HTTP API エンドポイント

`tome serve` が提供する REST API:

```
GET /health
GET /repositories
GET /repositories/{name}
GET /repositories/{name}/snapshots
GET /repositories/{name}/latest
GET /repositories/{name}/files        ?prefix= &include_deleted= &page= &per_page=
GET /repositories/{name}/diff         ?snapshot1= &snapshot2= &prefix=
GET /repositories/{name}/history      ?path=
GET /snapshots/{id}/entries           ?prefix=
GET /blobs/{digest}
GET /blobs/{digest}/entries
```

- digest はバイナリで保存し、API レスポンスでは hex 文字列に変換して返す
- `tome-server/src/server.rs` に axum ルーター実装

---

## 8. tome-web（Web フロントエンド）

Next.js 16 + TypeScript + Tailwind CSS v4 + App Router（Server Components のみ）。

### ディレクトリ構成

```
tome-web/
  src/
    lib/
      api.ts       — fetch ベース API クライアント（TOME_API_URL 環境変数）
      types.ts     — Repository / Snapshot / SnapshotMetadata / Entry 型定義
    app/
      layout.tsx   — ルートレイアウト（モノスペースフォント、ヘッダーナビ）
      page.tsx     — リポジトリ一覧（/）
      not-found.tsx
      repositories/[name]/page.tsx        — スナップショット一覧
      repositories/[name]/files/page.tsx  — 最新ファイル一覧（entry_cache）
      repositories/[name]/diff/page.tsx   — スナップショット間 diff
      repositories/[name]/history/page.tsx — パス履歴
      snapshots/[id]/page.tsx             — エントリ一覧
      blobs/[digest]/page.tsx             — blob 詳細
      globals.css  — Tailwind v4 (@import "tailwindcss")
  eslint.config.mjs  — ESLint flat config (eslint-config-next 16)
  .prettierrc.json   — Prettier 設定（printWidth: 120）
  env.local.example  — TOME_API_URL=http://localhost:8080
  .nvmrc             — 24
```

### 技術的注意事項

- **`export const dynamic = "force-dynamic"`** — ビルド時 SSG を防ぐ。tome serve が起動していないとビルドに失敗するため全ページに必須
- **Tailwind v4** — `@import "tailwindcss"` のみで動作。`tailwind.config.ts` は不要。PostCSS プラグインは `@tailwindcss/postcss`
- **Node.js 20.9+** — Next.js 16 の要件（mise で Node 24 を使用）
- **`env.local.example`（先頭ドットなし）** — ルートの `.gitignore` が `.env*` をブロックするため `.env.local.example` にできない
- **API は全てサーバーサイドで呼ぶ** — CORS 不要。`TOME_API_URL` はサーバー環境変数（`NEXT_PUBLIC_` 不要）
- **ESLint** — `eslint@9`（`eslint-plugin-react 7.x` が ESLint 10 非対応のため据え置き）
- **Prettier** — `npm run format` でフォーマット、`npm run lint` で ESLint + Prettier チェック

---

## 9. 技術的注意事項

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

## 10. 残タスク

| 優先度 | 内容 |
|--------|------|
| 高 | `tome restore` — スナップショット + store からファイルを復元（store download + entries_in_snapshot の組み合わせ） |
| 高 | `tome diff` CLI 版 — 2スナップショット間の差分を CLI で表示（`--stat` / `--name-only`）。API は既存 |
| 中 | `tome tag` — blob への key=value タグ付与・検索。tags テーブルは定義済み、CLI/API が未実装 |
| 中 | `tome verify`（ローカル） — スキャン済みファイルを再ハッシュし entry_cache と比較。ビットロット検知 |
| 中 | Watch モード（`tome watch`） — inotify/fsevents で監視し自動スナップショット |
| 中 | スナップショット注釈 — `tome scan --message "..."` で snapshots.message に手動メッセージを付与 |
| 低 | `tome gc` — 未参照 blob/replica の検出・削除。保持ポリシー（N世代 or N日）付き |
| 低 | 重複レポート — blob の content-addressing を活かしリポジトリ横断でファイル重複を報告 |
| 低 | PostgreSQL 中央同期 — 複数マシンが一つの PostgreSQL に push/pull（現在は SQLite↔SQLite のみ） |
| 低 | Webhook / 通知 — スキャン完了時に変更サマリを POST（Slack, Discord 等） |
| 低 | Git 互換 tree hash の統合（repository.config で opt-in） |
