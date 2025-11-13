// this_file: src/fonts.rs

//! Font loading, variation handling, and caching.
//!
//! This module provides zero-copy font loading via memory mapping,
//! variable font coordinate application, and LRU caching of font instances.

use crate::error::{Error, Result};
use camino::Utf8Path;
use lru::LruCache;
use memmap2::Mmap;
use read_fonts::{types::Tag, FileRef, FontRef};
use skrifa::MetadataProvider;
use std::collections::HashMap;
use std::fs::File;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Memory-mapped font with metadata and instance cache.
pub struct FontLoader {
    cache: Arc<Mutex<LruCache<FontCacheKey, Arc<FontInstance>>>>,
}

/// Font cache statistics for observability.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Maximum number of cached font instances.
    pub capacity: usize,
    /// Currently cached font instances.
    pub entries: usize,
}

/// Font instance with applied variations.
pub struct FontInstance {
    /// Memory-mapped font data
    #[allow(dead_code)]
    mmap: Arc<Mmap>,
    /// Font reference (zero-copy view into mmap)
    font_ref: FontRef<'static>,
    /// Applied variation coordinates
    coordinates: HashMap<String, f32>,
}

/// Cache key for font instances.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct FontCacheKey {
    path: String,
    coordinates: Vec<(String, u32)>, // (axis, f32 as bits)
}

impl FontLoader {
    /// Create a new font loader with specified cache size.
    pub fn new(cache_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(512).unwrap());
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(cache_size))),
        }
    }

    /// Load a font and apply variable font coordinates.
    ///
    /// Returns a cached instance if available, otherwise loads from disk.
    pub fn load_font(
        &self,
        path: &Utf8Path,
        coordinates: &HashMap<String, f32>,
    ) -> Result<Arc<FontInstance>> {
        // Check cache first
        let cache_key = FontCacheKey {
            path: path.to_string(),
            coordinates: coordinates
                .iter()
                .map(|(k, v)| (k.clone(), v.to_bits()))
                .collect(),
        };

        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(instance) = cache.get(&cache_key) {
                return Ok(Arc::clone(instance));
            }
        }

        // Not in cache - load from disk
        let instance = Self::load_font_impl(path, coordinates)?;
        let instance = Arc::new(instance);

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(cache_key, Arc::clone(&instance));
        }

        Ok(instance)
    }

    /// Clear all cached font instances.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Resize the cache to the requested capacity (drops old entries).
    pub fn set_capacity(&self, cache_size: usize) {
        let cap = NonZeroUsize::new(cache_size.max(1)).unwrap();
        let mut cache = self.cache.lock().unwrap();
        if cache.cap() == cap {
            return;
        }
        *cache = LruCache::new(cap);
    }

    /// Return current cache statistics.
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        CacheStats {
            capacity: cache.cap().get(),
            entries: cache.len(),
        }
    }

    /// Internal implementation: load font from disk and apply variations.
    fn load_font_impl(path: &Utf8Path, coordinates: &HashMap<String, f32>) -> Result<FontInstance> {
        // Memory-map the font file
        let file = File::open(path.as_std_path()).map_err(|e| Error::Mmap {
            path: path.as_std_path().to_path_buf(),
            source: e,
        })?;

        // Pre-check file size against limit
        let meta = file.metadata().map_err(|e| Error::Mmap {
            path: path.as_std_path().to_path_buf(),
            source: e,
        })?;
        crate::security::validate_font_size(meta.len())?;

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| Error::Mmap {
                path: path.as_std_path().to_path_buf(),
                source: e,
            })?
        };

        let mmap = Arc::new(mmap);

        // Parse font
        let font_data: &'static [u8] =
            unsafe { std::slice::from_raw_parts(mmap.as_ptr(), mmap.len()) };

        let file_ref = FileRef::new(font_data).map_err(|e| Error::InvalidFont {
            path: path.as_std_path().to_path_buf(),
            reason: format!("Failed to parse font file: {}", e),
        })?;

        let font_ref = match file_ref {
            FileRef::Font(f) => f,
            FileRef::Collection(c) => c.get(0).map_err(|e| Error::InvalidFont {
                path: path.as_std_path().to_path_buf(),
                reason: format!("Failed to get font from collection: {}", e),
            })?,
        };

        // Validate and clamp variation coordinates
        let clamped_coords = if !coordinates.is_empty() {
            Self::validate_and_clamp_coordinates(&font_ref, path.as_std_path(), coordinates)?
        } else {
            coordinates.clone()
        };

        Ok(FontInstance {
            mmap,
            font_ref,
            coordinates: clamped_coords,
        })
    }

    /// Validate variation axes and clamp coordinates to bounds.
    fn validate_and_clamp_coordinates(
        font: &FontRef,
        path: &Path,
        coordinates: &HashMap<String, f32>,
    ) -> Result<HashMap<String, f32>> {
        // Extract available axes from font
        let axes: HashMap<String, (f32, f32, f32)> = font
            .axes()
            .iter()
            .map(|axis| {
                let tag = axis.tag().to_string();
                (
                    tag,
                    (axis.min_value(), axis.default_value(), axis.max_value()),
                )
            })
            .collect();

        if axes.is_empty() {
            // Static font - ignore all coordinates
            if !coordinates.is_empty() {
                log::warn!(
                    "Font {} is static but coordinates provided - ignoring",
                    path.display()
                );
            }
            return Ok(HashMap::new());
        }

        // Validate and clamp each coordinate
        let mut clamped = HashMap::new();
        for (axis, value) in coordinates {
            if let Some((min, _default, max)) = axes.get(axis) {
                let clamped_value = value.clamp(*min, *max);
                if (clamped_value - value).abs() > 0.001 {
                    log::warn!(
                        "Coordinate for axis '{}' clamped from {} to {} (bounds: [{}, {}])",
                        axis,
                        value,
                        clamped_value,
                        min,
                        max
                    );
                }
                clamped.insert(axis.clone(), clamped_value);
            } else {
                let available: Vec<String> = axes.keys().cloned().collect();
                return Err(Error::UnknownAxis {
                    axis: axis.clone(),
                    path: path.to_path_buf(),
                    available,
                });
            }
        }

        Ok(clamped)
    }

    /// Get current cache statistics.
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.lock().unwrap();
        (cache.len(), cache.cap().get())
    }
}

impl FontInstance {
    /// Get the font reference.
    pub fn font_ref(&self) -> &FontRef<'static> {
        &self.font_ref
    }

    /// Get the applied variation coordinates.
    pub fn coordinates(&self) -> &HashMap<String, f32> {
        &self.coordinates
    }

    /// Get the raw font data bytes.
    pub fn font_data(&self) -> &[u8] {
        self.mmap.as_ref()
    }

    /// Create a skrifa Location for rendering.
    pub fn location(&self) -> Vec<(Tag, f32)> {
        self.coordinates
            .iter()
            .filter_map(|(tag_str, value)| {
                Tag::new_checked(tag_str.as_bytes())
                    .ok()
                    .map(|tag| (tag, *value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Tests require actual font files to be present
    // In production, use test fixtures from test-fonts/

    #[test]
    fn test_font_loader_creation() {
        let loader = FontLoader::new(256);
        let (used, cap) = loader.cache_stats();
        assert_eq!(used, 0);
        assert_eq!(cap, 256);
    }

    #[test]
    fn test_cache_key_equality() {
        let key1 = FontCacheKey {
            path: "font.ttf".to_string(),
            coordinates: vec![("wght".to_string(), 600.0f32.to_bits())],
        };
        let key2 = FontCacheKey {
            path: "font.ttf".to_string(),
            coordinates: vec![("wght".to_string(), 600.0f32.to_bits())],
        };
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_inequality_different_coords() {
        let key1 = FontCacheKey {
            path: "font.ttf".to_string(),
            coordinates: vec![("wght".to_string(), 600.0f32.to_bits())],
        };
        let key2 = FontCacheKey {
            path: "font.ttf".to_string(),
            coordinates: vec![("wght".to_string(), 700.0f32.to_bits())],
        };
        assert_ne!(key1, key2);
    }
}
