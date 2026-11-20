use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde_derive::{Deserialize, Serialize};
use url::Url;

use crate::base_url::{normalize_base_url, parse_base_url};
use crate::error::local_platform_settings_error::LocalPlatformSettingsError;

/// Bumped when a field changes meaning. A newer file is rejected rather than
/// silently half-understood, because the address decides where uploads go.
pub const CURRENT_SETTINGS_VERSION: &str = "1";

/// The user's optional local platform configuration.
///
/// `None` means the channel is not configured and must stay hidden. An address
/// alone is enough to enable it: there is no separate boolean that could drift
/// out of sync with the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPlatformSettings {
  pub maybe_base_url: Option<Url>,
}

/// On-disk shape. Optional fields are omitted when empty so an older build
/// reading a newer file sees a version it can reject explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalPlatformSettingsSerializable {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  version: Option<String>,

  #[serde(default, skip_serializing_if = "Option::is_none")]
  base_url: Option<String>,
}

impl LocalPlatformSettings {
  /// Build settings from an already-parsed URL, normalizing it.
  pub fn new(maybe_base_url: Option<Url>) -> Result<Self, LocalPlatformSettingsError> {
    let maybe_base_url = match maybe_base_url {
      Some(url) => Some(normalize_base_url(url).map_err(|reason| LocalPlatformSettingsError::InvalidBaseUrl { reason })?),
      None => None,
    };

    Ok(Self { maybe_base_url })
  }

  pub fn disabled() -> Self {
    Self { maybe_base_url: None }
  }

  pub fn is_configured(&self) -> bool {
    self.maybe_base_url.is_some()
  }

  /// Read the settings file, or `None` when it does not exist.
  ///
  /// A missing file is not an error: the local channel is optional and
  /// unconfigured by default.
  pub fn load_from_path(path: &Path) -> Result<Option<Self>, LocalPlatformSettingsError> {
    if !path.exists() {
      return Ok(None);
    }

    let path_label = path.display().to_string();
    let contents = std::fs::read_to_string(path).map_err(|source| LocalPlatformSettingsError::Read { path: path_label.clone(), source })?;

    let serializable: LocalPlatformSettingsSerializable = serde_json::from_str(&contents).map_err(|source| LocalPlatformSettingsError::Parse { path: path_label, source })?;

    if let Some(version) = serializable.version.as_deref()
      && version != CURRENT_SETTINGS_VERSION
    {
      return Err(LocalPlatformSettingsError::UnsupportedVersion { found: version.to_string(), supported: CURRENT_SETTINGS_VERSION });
    }

    let maybe_base_url = match serializable.base_url.as_deref() {
      Some(value) => Some(parse_base_url(value).map_err(|reason| LocalPlatformSettingsError::InvalidBaseUrl { reason })?),
      None => None,
    };

    Ok(Some(Self { maybe_base_url }))
  }

  pub fn persist_to_path(&self, path: &Path) -> Result<(), LocalPlatformSettingsError> {
    let serializable = LocalPlatformSettingsSerializable { version: Some(CURRENT_SETTINGS_VERSION.to_string()), base_url: self.maybe_base_url.as_ref().map(|url| url.to_string()) };

    let json = serde_json::to_string_pretty(&serializable).map_err(LocalPlatformSettingsError::Serialize)?;

    let path_label = path.display().to_string();
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(path).map_err(|source| LocalPlatformSettingsError::Write { path: path_label.clone(), source })?;

    file.write_all(json.as_bytes()).map_err(|source| LocalPlatformSettingsError::Write { path: path_label.clone(), source })?;
    file.flush().map_err(|source| LocalPlatformSettingsError::Write { path: path_label, source })?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  const SETTINGS_FILE_NAME: &str = "local_platform.json";
  const BASE_URL: &str = "http://127.0.0.1:8000";

  fn settings_path(directory: &TempDir) -> std::path::PathBuf {
    directory.path().join(SETTINGS_FILE_NAME)
  }

  fn write_raw(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("writes the fixture settings file");
  }

  #[test]
  fn missing_file_means_the_channel_is_unconfigured() {
    let directory = TempDir::new().expect("temp dir");
    let loaded = LocalPlatformSettings::load_from_path(&settings_path(&directory)).expect("loads");
    assert!(loaded.is_none());
  }

  #[test]
  fn round_trip_preserves_the_normalized_url() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    let settings = LocalPlatformSettings::new(Some(Url::parse(BASE_URL).expect("valid"))).expect("valid settings");

    settings.persist_to_path(&path).expect("persists");
    let loaded = LocalPlatformSettings::load_from_path(&path).expect("loads").expect("exists");

    assert_eq!(loaded, settings);
    assert_eq!(loaded.maybe_base_url.expect("configured").as_str(), "http://127.0.0.1:8000/");
  }

  #[test]
  fn disabled_settings_persist_without_an_address() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);

    LocalPlatformSettings::disabled().persist_to_path(&path).expect("persists");
    let contents = std::fs::read_to_string(&path).expect("reads");

    assert!(contents.contains(CURRENT_SETTINGS_VERSION));
    assert!(!contents.contains("base_url"), "an unconfigured channel writes no address: {contents}");
  }

  #[test]
  fn a_newer_settings_version_is_rejected() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, r#"{"version":"2","base_url":"http://127.0.0.1:8000"}"#);

    match LocalPlatformSettings::load_from_path(&path) {
      Err(LocalPlatformSettingsError::UnsupportedVersion { found, supported }) => {
        assert_eq!(found, "2");
        assert_eq!(supported, CURRENT_SETTINGS_VERSION);
      },
      other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
  }

  #[test]
  fn a_malformed_file_is_an_error_not_a_silent_default() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, "{not json");

    assert!(matches!(LocalPlatformSettings::load_from_path(&path), Err(LocalPlatformSettingsError::Parse { .. })));
  }

  #[test]
  fn an_invalid_address_in_the_file_is_rejected() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, r#"{"version":"1","base_url":"ftp://127.0.0.1:8000"}"#);

    match LocalPlatformSettings::load_from_path(&path) {
      Err(LocalPlatformSettingsError::InvalidBaseUrl { reason }) => {
        assert!(reason.contains("unsupported scheme"));
      },
      other => panic!("expected InvalidBaseUrl, got {other:?}"),
    }
  }

  #[test]
  fn unknown_fields_are_ignored() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, r#"{"version":"1","base_url":"http://127.0.0.1:8000","future_field":true}"#);

    let loaded = LocalPlatformSettings::load_from_path(&path).expect("loads").expect("exists");
    assert!(loaded.is_configured());
  }

  #[test]
  fn a_file_without_a_version_is_accepted() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, r#"{"base_url":"http://127.0.0.1:8000"}"#);

    let loaded = LocalPlatformSettings::load_from_path(&path).expect("loads").expect("exists");
    assert!(loaded.is_configured());
  }

  #[test]
  fn credentials_in_a_stored_address_are_rejected_without_echoing_them() {
    let directory = TempDir::new().expect("temp dir");
    let path = settings_path(&directory);
    write_raw(&path, r#"{"version":"1","base_url":"http://user:secret@127.0.0.1:8000"}"#);

    let error = LocalPlatformSettings::load_from_path(&path).expect_err("must reject credentials");
    let message = error.to_string();

    assert!(!message.contains("secret"), "error must not echo the password: {message}");
    assert!(!message.contains("user"), "error must not echo the username: {message}");
  }
}
