# Storage

## Supported URL Schemes

| Scheme | Example |
|--------|---------|
| Local filesystem | `file:///mnt/backup` |
| SSH / SFTP | `ssh://user@host/path` |
| Amazon S3 | `s3://bucket/prefix` |

## Blob Path Layout

Blobs are stored at a content-addressed path (see `tome-store/src/storage.rs::blob_path()`):

```
objects/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

Example: digest `deadbeef1234…` → `objects/de/ad/deadbeef1234…`
