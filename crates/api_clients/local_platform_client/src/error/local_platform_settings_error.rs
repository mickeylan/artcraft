use std::fmt;

/// Failures while reading, validating or writing local platform settings.
///
/// File paths are safe to report; the settings value is not echoed because it
/// may be a mistyped URL containing credentials.
#[derive(Debug)]
pub enum LocalPlatformSettingsError {
  Read { path: String, source: std::io::Error },
  Write { path: String, source: std::io::Error },
  Parse { path: String, source: serde_json::Error },
  Serialize(serde_json::Error),
  UnsupportedVersion { found: String, supported: &'static str },
  InvalidBaseUrl { reason: String },
}

impl fmt::Display for LocalPlatformSettingsError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Read { path, source } => {
        write!(f, "Could not read local platform settings at {path}: {source}")
      },
      Self::Write { path, source } => {
        write!(f, "Could not write local platform settings at {path}: {source}")
      },
      Self::Parse { path, source } => {
        write!(f, "Could not parse local platform settings at {path}: {source}")
      },
      Self::Serialize(source) => {
        write!(f, "Could not serialize local platform settings: {source}")
      },
      Self::UnsupportedVersion { found, supported } => {
        write!(f, "Local platform settings version {found} is not supported; this build reads version {supported}")
      },
      Self::InvalidBaseUrl { reason } => {
        write!(f, "Invalid local platform URL in settings: {reason}")
      },
    }
  }
}

impl std::error::Error for LocalPlatformSettingsError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
      Self::Parse { source, .. } => Some(source),
      Self::Serialize(source) => Some(source),
      Self::UnsupportedVersion { .. } | Self::InvalidBaseUrl { .. } => None,
    }
  }
}
