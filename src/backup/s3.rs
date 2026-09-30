//! The upload, and the only place `aws_config`, `aws_sdk_s3`,
//! `aws_smithy_types` and `tokio` are named.
//!
//! The runtime is built for one upload and dropped. Neither application is
//! otherwise async: SQLite is blocking and the TUI is a poll loop.

use anyhow::{Context, Result, anyhow};
use aws_config::BehaviorVersion;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::error::display::DisplayErrorContext;
use std::path::Path;

/// One `PutObject`. No multipart: a database is at most megabytes against a 5 GB
/// single-request limit, and multipart needs permissions the IAM policy
/// withholds. No server-side-encryption header: the bucket's default applies.
/// The region comes from the profile, where `aws configure set region` puts it.
pub fn upload(profile: &str, bucket: &str, key: &str, file: &Path) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the runtime for the upload")?;

    runtime.block_on(async {
        // The profile's keys and nothing else: the default chain would try
        // `AWS_ACCESS_KEY_ID` first, and a shell exporting another identity
        // would upload as that one instead of the PutObject-only user.
        let credentials = ProfileFileCredentialsProvider::builder()
            .profile_name(profile)
            .build();
        let config = aws_config::defaults(BehaviorVersion::latest())
            .profile_name(profile)
            .credentials_provider(credentials)
            .load()
            .await;

        // Said by name here: without a region S3 answers `PermanentRedirect`
        // or a dispatch failure, neither of which names the missing line.
        if config.region().is_none() {
            return Err(anyhow!(
                "no region for profile {profile}: run \
                 `aws configure set region <region> --profile {profile}`, \
                 or set AWS_REGION"
            ));
        }

        let client = aws_sdk_s3::Client::new(&config);
        let body = ByteStream::from_path(file)
            .await
            .with_context(|| format!("reading {}", file.display()))?;

        client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .content_type("application/zstd")
            // Create, never replace: the IAM policy refuses a PutObject
            // without this, which is what stops a stolen key from overwriting
            // an existing backup.
            .if_none_match("*")
            .send()
            .await
            // An `SdkError` displays as "service error"; the AccessDenied or
            // NoSuchBucket worth reading is in its source chain.
            .map_err(|e| anyhow!("{}", DisplayErrorContext(&e)))
            .with_context(|| format!("uploading to s3://{bucket}/{key}"))?;

        Ok(())
    })
}
