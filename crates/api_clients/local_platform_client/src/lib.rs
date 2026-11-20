//! Native client for an optional, user-configured local generation platform.
//!
//! This crate is deliberately independent of ArtCraft authentication, the
//! ArtCraft backend and Tauri. It talks only to the base URL the user
//! configures, so the local channel works without an ArtCraft session and can
//! never fall back to a cloud provider.
//!
//! # Provisional contract
//!
//! The platform interface is not confirmed yet (see
//! `docs/new-aigc-minimax-h3-integration-requirements.md`, section 3). Only the
//! capability document (`GET /api/model-catalog`) is implemented here, and its
//! body is returned as untyped JSON with a structural summary. Do not extract
//! named fields until the platform team delivers confirmed samples; inventing
//! field names would make a contract test that proves nothing.
//!
//! Upload, task creation, polling and download are intentionally absent: each
//! needs platform-confirmed shapes (sections 3.2–3.5) before it can be tested.
//!
//! # Trust boundary
//!
//! * Requests carry no ArtCraft cookies or tokens.
//! * Credentials embedded in the base URL are rejected, so they cannot reach
//!   settings files, error messages or logs.
//! * The HTTP client ignores system proxy settings: a loopback or LAN address
//!   must not be handed to an HTTP proxy.

mod base_url;
pub mod capabilities;
pub mod client;
pub mod error;
pub mod settings;
