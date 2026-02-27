use std::path::{Path, PathBuf};

use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use tracing::info;

use crate::{StoreError, error::Result, storage::Storage};

/// Storage backed by AWS S3 (or compatible object store).
pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
    /// Key prefix within the bucket (without trailing slash).
    prefix: PathBuf,
}

impl S3Storage {
    pub fn new(client: aws_sdk_s3::Client, bucket: impl Into<String>, prefix: impl Into<PathBuf>) -> Self {
        Self { client, bucket: bucket.into(), prefix: prefix.into() }
    }

    fn full_key(&self, remote_path: &str) -> String {
        let p = self.prefix.join(remote_path);
        // Normalise to forward slashes.
        p.to_string_lossy().replace('\\', "/")
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let s3_prefix = self.full_key(prefix);
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&s3_prefix)
            .send()
            .await
            .map_err(|e| StoreError::AwsS3(e.to_string()))?;

        let prefix_root = self.prefix.to_string_lossy().into_owned();
        let result = resp
            .contents()
            .iter()
            .filter_map(|obj| {
                obj.key()
                    .and_then(|k| k.strip_prefix(&format!("{}/", prefix_root)).or_else(|| k.strip_prefix(&prefix_root)))
                    .map(|k| k.to_owned())
            })
            .collect();
        Ok(result)
    }

    async fn upload(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let key = self.full_key(remote_path);
        info!("s3 upload: {:?} -> s3://{}/{}", local_file, self.bucket, key);
        let body = ByteStream::from_path(local_file).await.map_err(|e| StoreError::Other(e.to_string()))?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .send()
            .await
            .map_err(|e| StoreError::AwsS3(e.to_string()))?;
        Ok(())
    }

    async fn download(&self, remote_path: &str, local_file: &Path) -> Result<()> {
        let key = self.full_key(remote_path);
        info!("s3 download: s3://{}/{} -> {:?}", self.bucket, key, local_file);
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StoreError::AwsS3(e.to_string()))?;

        let mut reader = resp.body.into_async_read();
        let mut file = tokio::fs::File::create(local_file).await?;
        tokio::io::copy(&mut reader, &mut file).await?;
        Ok(())
    }

    async fn delete(&self, remote_path: &str) -> Result<()> {
        let key = self.full_key(remote_path);
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| StoreError::AwsS3(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, remote_path: &str) -> Result<bool> {
        let key = self.full_key(remote_path);
        match self.client.head_object().bucket(&self.bucket).key(&key).send().await {
            Ok(_) => Ok(true),
            Err(e) => {
                let se = e.into_service_error();
                if se.is_not_found() { Ok(false) } else { Err(StoreError::AwsS3(se.to_string())) }
            }
        }
    }
}
