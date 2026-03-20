# aether

File encryption/decryption CLI using streaming AEAD.
Supports XChaCha20-Poly1305, ChaCha20-Poly1305, and AES-256-GCM with streaming processing for files of any size.

## Installation

```bash
cargo install --path .
```

## Usage

### Encrypt with a key file

```bash
# Encrypt (output: file.txt.aet)
aether -k keyfile file.txt

# Decrypt
aether -d -k keyfile file.txt.aet

# Write to stdout
aether -c -k keyfile file.txt
aether -c -d -k keyfile file.txt.aet

# Read from stdin
echo "hello" | aether -c -k keyfile > out.aet
cat out.aet | aether -c -d -k keyfile
```

### Encrypt with a password

```bash
# Prompt for password (Argon2id KDF)
aether --password-prompt file.txt

# Read password from an environment variable
PASS=secret aether -P PASS file.txt

# Decrypt
aether -d --password-prompt file.txt.aet
```

### Encrypt with a key from an environment variable

```bash
# Read a Base64-encoded 32-byte key from an environment variable
MY_KEY=<base64key> aether -K MY_KEY file.txt
```

## Options

| Option | Description |
|---|---|
| `-k, --key-file <PATH>` | Key file path (`-` for stdin). Env: `AETHER_KEY_FILE` |
| `-K, --key-env <NAME>` | Environment variable name holding a Base64-encoded key |
| `-p, --password-prompt` | Prompt for a password (Argon2id KDF) |
| `-P, --password-env <NAME>` | Environment variable name holding a password |
| `-d, --decrypt` | Decrypt mode |
| `-c, --stdout` | Write output to stdout |
| `-o, --output <PATH>` | Output file path |
| `--cipher <ALGO>` | Cipher algorithm: `xchacha20-poly1305` (default) / `chacha20-poly1305` / `aes256gcm` |
| `--format-version <N>` | Format version: `0` (legacy) / `1` (streaming AEAD, default) |
| `--chunk-kind <N>` | Chunk size for v1: `0`=8 KiB … `7`=1 MiB (default) … `15`=256 MiB |
| `-j, --jobs <N>` | Parallel worker jobs (`0` = auto-detect, omit for serial) |
| `-i, --info` | Display encrypted file metadata (no key required) |

## File Extension

Encrypted files get a `.aet` suffix. It is stripped automatically on decryption.

```
file.txt      →  file.txt.aet   (encrypt)
file.txt.aet  →  file.txt       (decrypt)
```

## Key Size

Key files must be exactly 32 bytes (256 bits).

```bash
# Generate a random key
dd if=/dev/urandom of=keyfile bs=32 count=1
```

## License

Apache License 2.0
