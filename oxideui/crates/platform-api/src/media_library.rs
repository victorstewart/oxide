//! Media Library API for accessing device photo/video library

use alloc::string::String;
use alloc::vec::Vec;

/// Media asset type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Video,
    Audio,
}

/// Media asset from library
#[derive(Clone, Debug)]
pub struct MediaAsset {
    pub identifier: String,
    pub media_type: MediaType,
    pub creation_date: Option<u64>, // Unix timestamp in seconds
    pub duration_sec: Option<f64>,  // For video/audio
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
}

/// Thumbnail size for asset previews
#[derive(Clone, Copy, Debug)]
pub enum ThumbnailSize {
    Small,  // ~100px
    Medium, // ~300px
    Large,  // ~600px
}

/// Image data from library
#[derive(Clone, Debug)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,      // RGBA8888 format
    pub row_bytes: usize,
}

/// Media fetch options
#[derive(Clone, Debug)]
pub struct FetchOptions {
    pub media_types: Vec<MediaType>,
    pub limit: Option<usize>,
    pub ascending: bool, // Sort by creation date
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            media_types: alloc::vec![MediaType::Image],
            limit: None,
            ascending: false, // Newest first
        }
    }
}

/// Result of fetching media
#[derive(Clone, Debug)]
pub enum MediaFetchResult {
    Success(Vec<MediaAsset>),
    Denied,
    Error(String),
}

/// Result of loading image data
#[derive(Clone, Debug)]
pub enum ImageLoadResult {
    Success(ImageData),
    Error(String),
}

/// Platform-agnostic media library manager trait
pub trait MediaLibraryManager: Send + Sync {
    /// Fetch assets from library
    fn fetch_assets(&mut self, options: FetchOptions) -> MediaFetchResult;

    /// Load thumbnail for asset
    fn load_thumbnail(
        &mut self,
        identifier: &str,
        size: ThumbnailSize,
    ) -> ImageLoadResult;

    /// Load full-resolution image
    fn load_full_image(&mut self, identifier: &str) -> ImageLoadResult;

    /// Subscribe to library changes
    fn subscribe_to_changes<F>(&mut self, callback: F) -> u32
    where
        F: Fn() + Send + 'static;

    /// Unsubscribe from library changes
    fn unsubscribe(&mut self, subscription_id: u32);
}