// this_file: backends/o4e-core/src/cache.rs

//! Font caching infrastructure for efficient font management.

use crate::{O4eError, Result, ShapingResult};
use dashmap::DashMap;
use lru::LruCache;
use memmap2::Mmap;
use parking_lot::Mutex;
use std::fs::File;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Key for font lookups
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FontKey {
    pub path: PathBuf,
    pub face_index: u32,
}

/// Key for shape cache lookups
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ShapeKey {
    pub text: String,
    pub font_key: FontKey,
    pub size: u32, // Quantized size
    pub features: Vec<(String, bool)>,
}

/// Key for glyph cache lookups
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlyphKey {
    pub font_key: FontKey,
    pub glyph_id: u32,
    pub size: u32, // Quantized size
}

/// Parsed font face (backend-specific)
pub struct FontFace {
    pub data: Arc<Mmap>,
    pub face_index: u32,
    // Backend-specific parsed data would go here
}

/// Rendered glyph
pub struct RenderedGlyph {
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub left: f32,
    pub top: f32,
}

/// Font cache for efficient font and glyph management
pub struct FontCache {
    /// Memory-mapped font files
    mmap_cache: DashMap<PathBuf, Arc<Mmap>>,

    /// Parsed font faces
    face_cache: DashMap<FontKey, Arc<FontFace>>,

    /// Shaped text cache (thread-local LRU)
    shape_cache: Mutex<LruCache<ShapeKey, Arc<ShapingResult>>>,

    /// Rendered glyph cache
    glyph_cache: DashMap<GlyphKey, Arc<RenderedGlyph>>,

}

impl FontCache {
    /// Create a new font cache
    pub fn new(cache_size: usize) -> Self {
        Self {
            mmap_cache: DashMap::new(),
            face_cache: DashMap::new(),
            shape_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(512).unwrap()),
            )),
            glyph_cache: DashMap::new(),
        }
    }

    /// Get or load a font from disk
    pub fn get_or_load_font(&self, path: &Path, face_index: u32) -> Result<Arc<FontFace>> {
        let key = FontKey {
            path: path.to_owned(),
            face_index,
        };

        // Check face cache first
        if let Some(face) = self.face_cache.get(&key) {
            return Ok(face.clone());
        }

        // Get or create memory map
        let mmap = self.get_or_load_mmap(path)?;

        // Create font face
        let face = Arc::new(FontFace {
            data: mmap,
            face_index,
        });

        // Cache and return
        self.face_cache.insert(key, face.clone());
        Ok(face)
    }

    /// Get or create a memory map for a font file
    fn get_or_load_mmap(&self, path: &Path) -> Result<Arc<Mmap>> {
        // Check mmap cache first
        if let Some(mmap) = self.mmap_cache.get(path) {
            return Ok(mmap.clone());
        }

        // Load and memory map the file
        let file = File::open(path).map_err(|e| O4eError::font_load(path.to_owned(), e))?;

        let mmap =
            unsafe { Mmap::map(&file).map_err(|e| O4eError::font_load(path.to_owned(), e))? };

        let mmap = Arc::new(mmap);
        self.mmap_cache.insert(path.to_owned(), mmap.clone());
        Ok(mmap)
    }

    /// Get cached shaped text
    pub fn get_shaped(&self, key: &ShapeKey) -> Option<Arc<ShapingResult>> {
        let mut cache = self.shape_cache.lock();
        cache.get(key).cloned()
    }

    /// Cache shaped text
    pub fn cache_shaped(&self, key: ShapeKey, shaped: ShapingResult) -> Arc<ShapingResult> {
        let shaped = Arc::new(shaped);
        let mut cache = self.shape_cache.lock();
        cache.put(key, shaped.clone());
        shaped
    }

    /// Get cached glyph
    pub fn get_glyph(&self, key: &GlyphKey) -> Option<Arc<RenderedGlyph>> {
        self.glyph_cache.get(key).map(|g| g.clone())
    }

    /// Cache rendered glyph
    pub fn cache_glyph(&self, key: GlyphKey, glyph: RenderedGlyph) -> Arc<RenderedGlyph> {
        let glyph = Arc::new(glyph);
        self.glyph_cache.insert(key, glyph.clone());
        glyph
    }

    /// Clear all caches
    pub fn clear(&self) {
        self.mmap_cache.clear();
        self.face_cache.clear();
        self.shape_cache.lock().clear();
        self.glyph_cache.clear();
    }

    /// Returns true when all cache layers are empty.
    pub fn is_empty(&self) -> bool {
        self.mmap_cache.is_empty()
            && self.face_cache.is_empty()
            && self.shape_cache.lock().len() == 0
            && self.glyph_cache.is_empty()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            mmap_count: self.mmap_cache.len(),
            face_count: self.face_cache.len(),
            shape_count: self.shape_cache.lock().len(),
            glyph_count: self.glyph_cache.len(),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub mmap_count: usize,
    pub face_count: usize,
    pub shape_count: usize,
    pub glyph_count: usize,
}

impl CacheStats {
    pub fn is_empty(&self) -> bool {
        self.mmap_count == 0
            && self.face_count == 0
            && self.shape_count == 0
            && self.glyph_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BoundingBox, Direction, Font, Glyph};
    use std::path::PathBuf;

    fn test_font_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fonts/NotoSans-Regular.ttf")
    }

    #[test]
    fn clear_removes_all_cached_entries() {
        let cache = FontCache::new(8);
        let path = test_font_path();
        cache
            .get_or_load_font(&path, 0)
            .expect("fixture font should load");

        let font_key = FontKey {
            path: path.clone(),
            face_index: 0,
        };
        let glyph_key = GlyphKey {
            font_key: font_key.clone(),
            glyph_id: 42,
            size: 1200,
        };
        cache.cache_glyph(
            glyph_key,
            RenderedGlyph {
                bitmap: vec![255],
                width: 1,
                height: 1,
                left: 0.0,
                top: 0.0,
            },
        );

        let shape_key = ShapeKey {
            text: "abc".to_string(),
            font_key,
            size: 1200,
            features: vec![("liga".to_string(), true)],
        };
        cache.cache_shaped(
            shape_key,
            ShapingResult {
                text: "abc".to_string(),
                glyphs: vec![Glyph {
                    id: 1,
                    cluster: 0,
                    x: 0.0,
                    y: 0.0,
                    advance: 10.0,
                }],
                advance: 10.0,
                bbox: BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                font: Some(Font::new("Test", 12.0)),
                direction: Direction::LeftToRight,
            },
        );

        assert!(
            !cache.is_empty(),
            "cache should report entries before clearing: {:?}",
            cache.stats()
        );
        cache.clear();
        assert!(cache.is_empty(), "cache should be empty after clear");
        assert!(cache.stats().is_empty(), "stats should reset after clear");
    }
}
