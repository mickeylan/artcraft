use std::fmt;
use std::time::Duration;

/// How much of a response body is kept in an error.
///
/// Enough to diagnose a misconfigured deployment, short enough for logs and UI.
pub const BODY_EXCERPT_CHARS: usize = 300;

/// Everything that can go wrong while talking to the local platform.
///
/// Endpoints are recorded as URL paths only. The configured host is not
/// repeated in errors, and credentials are rejected before a request is built.
#[derive(Debug)]
pub enum LocalPlatformError {
  /// The configured base URL is not usable.
  InvalidBaseUrl { reason: String },

  /// The underlying HTTP client could not be constructed.
  ClientBuild(reqwest::Error),

  /// The request could not be sent or the response could not be read.
  Request { endpoint: String, source: reqwest::Error },

  /// The platform did not answer before the configured timeout.
  Timeout { endpoint: String, timeout: Duration },

  /// The platform answered with a non-success HTTP status.
  UnexpectedStatus { endpoint: String, status_code: u16, body_excerpt: String },

  /// The platform answered with a body that is not JSON.
  InvalidJson { endpoint: String, body_excerpt: String, source: serde_json::Error },
}

impl LocalPlatformError {
  pub fn invalid_base_url(reason: impl Into<String>) -> Self {
    Self::InvalidBaseUrl { reason: reason.into() }
  }
}

impl fmt::Display for LocalPlatformError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::InvalidBaseUrl { reason } => {
        write!(f, "Invalid local platform URL: {reason}")
      },
      Self::ClientBuild(source) => {
        write!(f, "Could not create the local platform HTTP client: {source}")
      },
      Self::Request { endpoint, source } => {
        write!(f, "Request to the local platform failed at {endpoint}: {source}")
      },
      Self::Timeout { endpoint, timeout } => {
        write!(f, "The local platform did not answer {endpoint} within {timeout:?}")
      },
      Self::UnexpectedStatus { endpoint, status_code, body_excerpt } => {
        write!(f, "The local platform returned HTTP {status_code} for {endpoint}: {body_excerpt}")
      },
      Self::InvalidJson { endpoint, body_excerpt, source } => {
        write!(f, "The local platform returned invalid JSON for {endpoint} ({source}): {body_excerpt}")
      },
    }
  }
}

impl std::error::Error for LocalPlatformError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::ClientBuild(source) | Self::Request { source, .. } => Some(source),
      Self::InvalidJson { source, .. } => Some(source),
      Self::InvalidBaseUrl { .. } | Self::Timeout { .. } | Self::UnexpectedStatus { .. } => None,
    }
  }
}

/// Turn an arbitrary body into a single-line excerpt safe for logs.
pub(crate) fn body_excerpt(body: &str) -> String {
  let mut excerpt: String = body
    .chars()
    .take(BODY_EXCERPT_CHARS)
    .map(|character| match character {
      '\n' | '\r' | '\t' => ' ',
      other => other,
    })
    .collect();

  if body.chars().count() > BODY_EXCERPT_CHARS {
    excerpt.push('…');
  }

  excerpt
}

/// Map a transport failure, distinguishing a timeout from other I/O errors.
pub(crate) fn map_send_error(endpoint: &str, timeout: Duration, source: reqwest::Error) -> LocalPlatformError {
  if source.is_timeout() { LocalPlatformError::Timeout { endpoint: endpoint.to_string(), timeout } } else { LocalPlatformError::Request { endpoint: endpoint.to_string(), source } }
}

#[cfg(test)]
mod tests {
  use super::*;

  mod excerpt_tests {
    use super::*;

    #[test]
    fn short_bodies_are_kept_whole() {
      assert_eq!(body_excerpt("upstream exploded"), "upstream exploded");
    }

    #[test]
    fn long_bodies_are_truncated_with_an_ellipsis() {
      let body = "a".repeat(BODY_EXCERPT_CHARS + 50);
      let excerpt = body_excerpt(&body);
      assert!(excerpt.ends_with('…'));
      assert_eq!(excerpt.chars().count(), BODY_EXCERPT_CHARS + 1);
    }

    #[test]
    fn newlines_are_collapsed_for_single_line_logs() {
      assert_eq!(body_excerpt("line one\nline two\r\nline three"), "line one line two  line three");
    }

    #[test]
    fn multibyte_characters_are_not_split() {
      let body = "漫".repeat(BODY_EXCERPT_CHARS + 10);
      let excerpt = body_excerpt(&body);
      assert_eq!(excerpt.chars().filter(|character| *character == '漫').count(), BODY_EXCERPT_CHARS);
    }
  }

  mod display_tests {
    use super::*;

    #[test]
    fn invalid_url_errors_carry_only_the_reason() {
      let error = LocalPlatformError::invalid_base_url("credentials in the URL are not allowed");
      let message = error.to_string();
      assert!(message.contains("credentials in the URL are not allowed"));
    }

    #[test]
    fn unexpected_status_includes_status_and_body() {
      let error = LocalPlatformError::UnexpectedStatus { endpoint: "/api/model-catalog".to_string(), status_code: 503, body_excerpt: "starting up".to_string() };
      let message = error.to_string();
      assert!(message.contains("503"));
      assert!(message.contains("starting up"));
    }
  }
}
