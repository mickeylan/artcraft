use std::time::Duration;

use url::Url;

use crate::base_url::{normalize_base_url, parse_base_url};
use crate::capabilities::model_catalog::MODEL_CATALOG_PATH;
use crate::error::local_platform_error::LocalPlatformError;

/// Capability queries should answer quickly. A slow or stopped local service
/// must not leave a settings pane spinning.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// A configured connection to the local platform.
///
/// The client owns its HTTP configuration so every request from this crate
/// shares the same timeout and proxy policy.
#[derive(Debug, Clone)]
pub struct LocalPlatformClient {
  base_url: Url,
  model_catalog_url: Url,
  http: reqwest::Client,
  request_timeout: Duration,
}

impl LocalPlatformClient {
  /// Build a client from a normalized URL using [`DEFAULT_REQUEST_TIMEOUT`].
  pub fn new(base_url: Url) -> Result<Self, LocalPlatformError> {
    Self::with_request_timeout(base_url, DEFAULT_REQUEST_TIMEOUT)
  }

  /// Build a client from a user-entered URL string.
  pub fn from_base_url_str(value: &str) -> Result<Self, LocalPlatformError> {
    let base_url = parse_base_url(value).map_err(LocalPlatformError::invalid_base_url)?;
    Self::new(base_url)
  }

  pub fn with_request_timeout(base_url: Url, request_timeout: Duration) -> Result<Self, LocalPlatformError> {
    let base_url = normalize_base_url(base_url).map_err(LocalPlatformError::invalid_base_url)?;

    // `MODEL_CATALOG_PATH` has no leading slash, so this appends to the base
    // path instead of discarding it. `normalize_base_url` guarantees the base
    // path ends in `/`.
    let model_catalog_url = base_url.join(MODEL_CATALOG_PATH).map_err(|err| LocalPlatformError::invalid_base_url(format!("could not build the model catalog URL: {err}")))?;

    // `no_proxy()` is deliberate: a loopback or LAN address is not reachable
    // through a system HTTP proxy, and sending it there would leak the address
    // and fail with a confusing error.
    let http = reqwest::Client::builder().timeout(request_timeout).no_proxy().build().map_err(LocalPlatformError::ClientBuild)?;

    Ok(Self { base_url, model_catalog_url, http, request_timeout })
  }

  pub fn base_url(&self) -> &Url {
    &self.base_url
  }

  pub fn model_catalog_url(&self) -> &Url {
    &self.model_catalog_url
  }

  pub fn request_timeout(&self) -> Duration {
    self.request_timeout
  }

  pub(crate) fn http(&self) -> &reqwest::Client {
    &self.http
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const BASE_URL_WITH_PORT: &str = "http://127.0.0.1:8000";

  #[test]
  fn new_uses_the_default_timeout() {
    let client = LocalPlatformClient::from_base_url_str(BASE_URL_WITH_PORT).expect("builds");
    assert_eq!(client.request_timeout(), DEFAULT_REQUEST_TIMEOUT);
  }

  #[test]
  fn catalog_url_appends_to_a_bare_host() {
    let client = LocalPlatformClient::from_base_url_str(BASE_URL_WITH_PORT).expect("builds");
    assert_eq!(client.model_catalog_url().as_str(), "http://127.0.0.1:8000/api/model-catalog");
  }

  #[test]
  fn catalog_url_appends_to_an_existing_path_prefix() {
    let client = LocalPlatformClient::from_base_url_str("http://127.0.0.1:8000/platform").expect("builds");
    assert_eq!(client.model_catalog_url().as_str(), "http://127.0.0.1:8000/platform/api/model-catalog");
  }

  #[test]
  fn credentials_in_the_url_are_rejected() {
    let error = LocalPlatformClient::from_base_url_str("http://user:secret@127.0.0.1:8000").expect_err("rejects");
    assert!(error.to_string().contains("credentials"));
  }

  #[test]
  fn a_custom_timeout_is_kept() {
    let timeout = Duration::from_secs(3);
    let client = LocalPlatformClient::with_request_timeout(Url::parse(BASE_URL_WITH_PORT).expect("valid"), timeout).expect("builds");
    assert_eq!(client.request_timeout(), timeout);
  }
}
