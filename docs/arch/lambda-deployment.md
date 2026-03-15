# AWS Lambda Deployment

`tome-server` can run as an AWS Lambda function using the `lambda` feature flag. The Lambda binary (`tome-lambda`) wraps the same axum router via `lambda_http`.

Migrations are **not** executed on Lambda startup. The schema must be applied beforehand using `psqldef` or similar tools. The Lambda binary uses `connection::connect()` (connect-only) instead of `connection::open()` (connect + migrate).

## Build

```bash
# Requires: cargo install cargo-lambda
cargo lambda build --release --features lambda --bin tome-lambda

# With DynamoDB backend
cargo lambda build --release --features lambda,dynamodb --bin tome-lambda --arm64

# Deploy (initial)
cargo lambda deploy tome-lambda \
  --runtime provided.al2023 \
  --memory-size 256 \
  --timeout 30

# Set environment variables
aws lambda update-function-configuration \
  --function-name tome-lambda \
  --environment "Variables={TOME_DB=dynamodb://tome-data,TOME_MACHINE_ID=0}"
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TOME_DB` | Database URL: `postgres://...`, `sqlite://...`, or `dynamodb://<table-name>` |
| `TOME_MACHINE_ID` | Sonyflake machine ID (0–32767; default: 0) |

## Backend Selection

The Lambda entry point (`tome-server/src/bin/tome-lambda.rs`) dispatches on the `TOME_DB` URL prefix:

| Prefix | Backend |
|--------|---------|
| `dynamodb://` | `DynamoStore` (requires `--features dynamodb`) |
| Otherwise | `SeaOrmStore` (PostgreSQL / SQLite) |
