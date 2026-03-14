//! AWS SigV4 request signing for sync HTTP peers.
//!
//! When a sync peer's config contains `"auth": "aws-iam"`, outgoing HTTP
//! requests are signed with AWS Signature Version 4 using credentials
//! resolved from the default credential chain (`AWS_*` env vars, instance
//! profile, etc.).  The service name defaults to `"lambda"` (for Lambda
//! Function URLs) and the region is read from `peer.config["region"]` or
//! the default AWS config.

use std::time::SystemTime;

use anyhow::{Context, Result};
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{SignableBody, SignableRequest, SigningSettings, sign};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use reqwest::header::HeaderValue;

/// Resolved AWS signing context for a sync peer.
pub struct AwsSigner {
    identity: Identity,
    region: String,
    service: String,
}

impl AwsSigner {
    /// Build a signer from the default AWS credential chain.
    ///
    /// `region_override` takes precedence over the SDK default region.
    /// `service_override` defaults to `"lambda"`.
    pub async fn from_env(region_override: Option<&str>, service_override: Option<&str>) -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

        let provider = config.credentials_provider().context("no AWS credentials provider configured")?;

        let credentials = provider.as_ref().provide_credentials().await.context("failed to resolve AWS credentials")?;

        let region = region_override
            .map(String::from)
            .or_else(|| config.region().map(|r| r.to_string()))
            .context("AWS region not configured (set AWS_REGION or peer config \"region\")")?;

        let service = service_override.unwrap_or("lambda").to_string();

        let identity: Identity = Credentials::new(
            credentials.access_key_id(),
            credentials.secret_access_key(),
            credentials.session_token().map(String::from),
            None,
            "tome-sync",
        )
        .into();

        Ok(Self { identity, region, service })
    }

    /// Sign a GET request URL, returning the signed `reqwest::Request`.
    pub fn sign_get(&self, client: &reqwest::Client, url: &str) -> Result<reqwest::Request> {
        let mut req = client.get(url).build()?;
        self.sign_request(&mut req, &[])?;
        Ok(req)
    }

    /// Sign a POST request with a JSON body, returning the signed `reqwest::Request`.
    pub fn sign_post(&self, client: &reqwest::Client, url: &str, body: &[u8]) -> Result<reqwest::Request> {
        let mut req = client.post(url).header("content-type", "application/json").body(body.to_vec()).build()?;
        self.sign_request(&mut req, body)?;
        Ok(req)
    }

    fn sign_request(&self, req: &mut reqwest::Request, body: &[u8]) -> Result<()> {
        let signable_body = if body.is_empty() { SignableBody::UnsignedPayload } else { SignableBody::Bytes(body) };

        let headers: Vec<(&str, &str)> =
            req.headers().iter().filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str(), v))).collect();

        let signable =
            SignableRequest::new(req.method().as_str(), req.url().as_str(), headers.into_iter(), signable_body)
                .context("failed to create signable request")?;

        let params = v4::SigningParams::builder()
            .identity(&self.identity)
            .region(&self.region)
            .name(&self.service)
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .context("failed to build signing params")?
            .into();

        let output = sign(signable, &params).context("SigV4 signing failed")?;
        let (instructions, _signature) = output.into_parts();

        for (name, value) in instructions.headers() {
            req.headers_mut()
                .insert(reqwest::header::HeaderName::from_bytes(name.as_bytes())?, HeaderValue::from_str(value)?);
        }

        Ok(())
    }
}

/// Check if a sync peer's config requires AWS IAM auth.
pub fn needs_aws_auth(config: &serde_json::Value) -> bool {
    config.get("auth").and_then(|v| v.as_str()) == Some("aws-iam")
}

/// Extract the region override from peer config, if present.
pub fn peer_region(config: &serde_json::Value) -> Option<&str> {
    config.get("region").and_then(|v| v.as_str())
}

/// Extract the service name override from peer config, if present.
pub fn peer_service(config: &serde_json::Value) -> Option<&str> {
    config.get("service").and_then(|v| v.as_str())
}
