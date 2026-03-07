# aether Streaming AEAD 設計

## ヘッダ flags レイアウト（u16）

```
bits [15:12]  version      フォーマットバージョン (0–15)
bits [11:8]   reserved     予約（0 固定、将来利用）
bits [7:4]    chunk_kind   チャンクサイズ選択
bits [3:0]    algorithm    AEAD アルゴリズム
```

後方互換性: 既存ファイル `0x0000`（AES）/ `0x0001`（ChaCha20）は version=0, chunk_kind=0 として正しくパースされる。

## バージョン定義

| version | 意味 |
|---------|------|
| 0 | 現行フォーマット。chunk_kind は無視（常に 8 KiB）。integrity を平文末尾に付加して検証 |
| 1 | Streaming AEAD。STREAM 構成（last-chunk フラグ）、可変チャンクサイズ、ヘッダ AD 認証 |

## algorithm 値

| 値 | アルゴリズム |
|----|-------------|
| 0 | AES-256-GCM |
| 1 | ChaCha20-Poly1305 |
| 2–15 | 予約（AES-256-GCM-SIV, XChaCha20 等） |

## chunk_kind 値

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

## v1 STREAM nonce 構成

```
nonce (12 bytes) = IV ⊕ (0x00{4} ‖ counter_u64_BE)
last chunk:        nonce[0] ^= 0x80
```

- IV: ヘッダに格納されるランダム 12 バイト
- counter: チャンク番号（0 始まり）。bytes [4..12] に XOR
- last-chunk フラグ: byte [0] の最上位ビット。截断攻撃を防止
- counter の最大値は u64 だが、同一 nonce の再利用を避けるため `2^32` チャンクを上限とする運用を推奨

## v1 ヘッダ認証（Associated Data）

第 1 チャンクの AEAD 暗号化で、ヘッダ 32 バイトを Associated Data (AD) として渡す。

```
chunk_0  = AEAD_encrypt(key, nonce_0, plaintext_0, ad = header_bytes[0..32])
chunk_i  = AEAD_encrypt(key, nonce_i, plaintext_i, ad = "")    (i > 0)
```

これにより、ヘッダの改竄（flags 書換え、IV 差替え等）は第 1 チャンクの復号時に検出される。

## v0 → v1 の主な変更点

| 項目 | v0 | v1 |
|------|----|----|
| last-chunk マーカー | なし（integrity suffix で間接的に検出） | nonce bit で明示 |
| integrity フィールド | 平文末尾にも付加し復号後に検証 | ヘッダのみ（パスワード KDF の salt として使用） |
| チャンクサイズ | 固定 8 KiB | chunk_kind で選択可能 |
| ヘッダ認証 | なし（IV/flags 改竄は AEAD 失敗で間接検出） | 第 1 チャンクの AD で明示認証 |
| 平文末尾バッファリング | 必要（integrity 16 バイトを分離） | 不要 |

## `encrypt_bytes` / `decrypt_bytes` との関係

ファイル名暗号化（`encrypt_bytes` / `decrypt_bytes`）はストリーミングフォーマットとは独立。
単一チャンクの AEAD + nonce 付加方式で、v1 の影響を受けない。

## 実装フェーズ

1. **flags パーサーリファクタ** — `HeaderFlags` 構造体を導入し、version / chunk_kind / algorithm を個別にパース。v0 動作は維持
2. **ChunkKind 型** — サイズ計算メソッドを持つ enum。`cipher.rs` の `BUFFER_SIZE` を動的に
3. **v1 STREAM 暗号化** — `CounteredNonce` に `is_last` パラメータ追加、第 1 チャンクの AD、integrity suffix 除去
4. **v1 STREAM 復号** — ヘッダの version で v0/v1 を分岐。v0 は従来ロジック、v1 は新ロジック
5. **テスト** — v0 既存テスト維持 + v1 roundtrip + 各 chunk_kind + 截断検知 + ヘッダ改竄検知
6. **ドキュメント** — ARCHITECTURE.md の aether セクション更新
