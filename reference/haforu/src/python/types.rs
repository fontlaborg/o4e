// this_file: src/python/types.rs

//! Type conversions between Rust and Python.
//!
//! This module handles conversion of haforu types to/from Python objects.
//! Error conversions are now handled in the `errors` module.

// Re-export error conversion utilities for convenience
pub use super::errors::ErrorConverter;

// Note: The From<HaforuError> for PyErr implementation has been moved to
// src/python/errors.rs for centralized error handling with enhanced context support.
