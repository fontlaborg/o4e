// this_file: crates/o4e-fontdb/src/lib.rs

//! Shared font discovery helpers and fallback metadata for o4e.

use dashmap::DashMap;
use fontdb::{Database, Family, Query, Source, Stretch, Style, Weight};
use log::warn;
use o4e_core::{
    types::{Font, FontSource, FontStyle},
    O4eError, Result,
};
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use shellexpand::tilde;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Global font database instance backed by `fontdb`.
pub struct FontDatabase {
    db: RwLock<Database>,
    cache: DashMap<String, Arc<FontHandle>>,
}

impl FontDatabase {
    /// Access the process-wide font database.
    pub fn global() -> &'static Self {
        static INSTANCE: OnceCell<FontDatabase> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            let mut db = Database::new();
            db.load_system_fonts();
            for extra in extra_font_dirs() {
                if extra.exists() {
                    db.load_fonts_dir(extra);
                }
            }

            FontDatabase {
                db: RwLock::new(db),
                cache: DashMap::new(),
            }
        })
    }

    /// Resolve a [`Font`] into a concrete handle that backends can load.
    pub fn resolve(&self, font: &Font) -> Result<Arc<FontHandle>> {
        self.resolve_inner(Some(font), &font.source, &font.family)
    }

    /// Resolve a [`FontSource`] without a full [`Font`] context.
    pub fn resolve_source(
        &self,
        source: &FontSource,
        fallback_name: &str,
    ) -> Result<Arc<FontHandle>> {
        self.resolve_inner(None, source, fallback_name)
    }

    fn resolve_inner(
        &self,
        font: Option<&Font>,
        source: &FontSource,
        fallback_name: &str,
    ) -> Result<Arc<FontHandle>> {
        match source {
            FontSource::Family(name) => self.resolve_family(font, name, fallback_name),
            FontSource::Path(path) => self.resolve_path(path, fallback_name),
            FontSource::Bytes { name, data } => Ok(self.resolve_bytes(name, data.clone())),
        }
    }

    fn resolve_family(
        &self,
        font: Option<&Font>,
        name: &str,
        fallback_name: &str,
    ) -> Result<Arc<FontHandle>> {
        let db = self.db.read();
        let families_vec: Vec<Family<'_>> = if name == fallback_name {
            vec![Family::Name(name)]
        } else {
            vec![Family::Name(name), Family::Name(fallback_name)]
        };
        let (weight, style) = if let Some(font) = font {
            (
                Weight(font.weight),
                match font.style {
                    FontStyle::Normal => Style::Normal,
                    FontStyle::Italic => Style::Italic,
                    FontStyle::Oblique => Style::Oblique,
                },
            )
        } else {
            (Weight::NORMAL, Style::Normal)
        };
        let query = Query {
            families: &families_vec,
            weight,
            stretch: Stretch::Normal,
            style,
        };

        if let Some(id) = db.query(&query) {
            let face = db
                .face(id)
                .ok_or_else(|| O4eError::FontNotFound { name: name.into() })?;
            return self.resolve_face(face);
        }

        drop(db);
        warn!("Font '{}' not found in system database", name);
        Err(O4eError::FontNotFound {
            name: name.to_string(),
        })
    }

    fn resolve_path(&self, path: &str, label: &str) -> Result<Arc<FontHandle>> {
        let expanded = tilde(path).to_string();
        let canonical = canonicalize(&expanded);
        let key = format!("file:{}#0", canonical.to_string_lossy());
        if let Some(entry) = self.cache.get(&key) {
            return Ok(entry.clone());
        }

        let bytes =
            std::fs::read(&canonical).map_err(|e| O4eError::font_load(canonical.clone(), e))?;
        let handle = Arc::new(FontHandle {
            key: key.clone(),
            path: Some(canonical),
            face_index: 0,
            bytes: Arc::from(bytes.into_boxed_slice()),
            family: label.to_string(),
        });
        self.cache.insert(key, handle.clone());
        Ok(handle)
    }

    fn resolve_bytes(&self, name: &str, data: Arc<[u8]>) -> Arc<FontHandle> {
        let key = format!("memory:{name}:{:p}", Arc::as_ptr(&data));
        if let Some(entry) = self.cache.get(&key) {
            return entry.clone();
        }

        let handle = Arc::new(FontHandle {
            key: key.clone(),
            path: None,
            face_index: 0,
            bytes: data,
            family: name.to_string(),
        });
        self.cache.insert(key, handle.clone());
        handle
    }

    fn resolve_face(&self, face: &fontdb::FaceInfo) -> Result<Arc<FontHandle>> {
        let key = cache_key(face);
        if let Some(entry) = self.cache.get(&key) {
            return Ok(entry.clone());
        }

        let (path, bytes) = match &face.source {
            Source::File(path) => {
                let canonical = canonicalize(path);
                let data = std::fs::read(&canonical)
                    .map_err(|e| O4eError::font_load(canonical.clone(), e))?;
                (Some(canonical), Arc::from(data.into_boxed_slice()))
            }
            Source::Binary(data) => {
                let owned = data.as_ref().as_ref().to_vec();
                (None, Arc::from(owned.into_boxed_slice()))
            }
            Source::SharedFile(path, data) => {
                let owned = data.as_ref().as_ref().to_vec();
                (Some(path.clone()), Arc::from(owned.into_boxed_slice()))
            }
        };

        let handle = Arc::new(FontHandle {
            key: key.clone(),
            path,
            face_index: face.index,
            bytes,
            family: face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| face.post_script_name.clone()),
        });
        self.cache.insert(key, handle.clone());
        Ok(handle)
    }
}

fn cache_key(face: &fontdb::FaceInfo) -> String {
    match &face.source {
        Source::File(path) => format!("file:{}#{}", path.display(), face.index),
        Source::Binary(_) => format!("memory:{}#{}", face.post_script_name, face.index),
        Source::SharedFile(path, _) => format!("file:{}#{}", path.display(), face.index),
    }
}

fn canonicalize(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref()
        .canonicalize()
        .unwrap_or_else(|_| path.as_ref().to_path_buf())
}

fn extra_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(value) = std::env::var("O4E_FONT_DIRS") {
        dirs.extend(std::env::split_paths(&value));
    }
    for sys in o4e_core::utils::system_font_dirs() {
        dirs.push(PathBuf::from(tilde(&sys).to_string()));
    }
    dirs
}

/// Font data resolved from the system or user-provided sources.
#[derive(Debug, Clone)]
pub struct FontHandle {
    pub key: String,
    pub path: Option<PathBuf>,
    pub face_index: u32,
    pub bytes: Arc<[u8]>,
    pub family: String,
}

impl FontHandle {
    /// A [`FontSource`] that points directly at this handle's bytes.
    pub fn to_source(&self) -> FontSource {
        match &self.path {
            Some(path) => FontSource::Path(path.to_string_lossy().into_owned()),
            None => FontSource::Bytes {
                name: self.family.clone(),
                data: self.bytes.clone(),
            },
        }
    }
}

/// Return script-specific fallback font families.
pub fn script_fallbacks(script: &str) -> &'static [&'static str] {
    match script.to_ascii_lowercase().as_str() {
        "arabic" => &ARABIC_FALLBACKS,
        "devanagari" => &DEVANAGARI_FALLBACKS,
        "han" | "hiragana" | "katakana" => &CJK_FALLBACKS,
        "hangul" => &HANGUL_FALLBACKS,
        "hebrew" => &HEBREW_FALLBACKS,
        "thai" => &THAI_FALLBACKS,
        "cyrillic" => &CYRILLIC_FALLBACKS,
        "greek" => &GREEK_FALLBACKS,
        _ => &DEFAULT_FALLBACKS,
    }
}

const ARABIC_FALLBACKS: [&str; 4] = [
    "NotoNaskhArabic-Regular",
    "NotoNaskhArabic",
    "GeezaPro",
    "ArialUnicodeMS",
];
const DEVANAGARI_FALLBACKS: [&str; 3] = [
    "NotoSansDevanagari-Regular",
    "NotoSansDevanagari",
    "KohinoorDevanagari",
];
const CJK_FALLBACKS: [&str; 3] = ["NotoSansCJKsc-Regular", "PingFangSC", "NotoSansJP-Regular"];
const HANGUL_FALLBACKS: [&str; 2] = ["NotoSansKR-Regular", "AppleSDGothicNeo-Regular"];
const HEBREW_FALLBACKS: [&str; 2] = ["NotoSansHebrew-Regular", "ArialHebrew"];
const THAI_FALLBACKS: [&str; 2] = ["NotoSansThai-Regular", "Thonburi"];
const CYRILLIC_FALLBACKS: [&str; 3] = ["NotoSans-Regular", "PTSans-Regular", "ArialUnicodeMS"];
const GREEK_FALLBACKS: [&str; 2] = ["NotoSans-Regular", "ArialUnicodeMS"];
const DEFAULT_FALLBACKS: [&str; 3] = ["NotoSans-Regular", "DejaVuSans", "ArialUnicodeMS"];
