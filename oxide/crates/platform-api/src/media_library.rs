//! Media library service.

use crate::PlatformError;
use core::future::Future;
use core::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetId(pub alloc::string::String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetType {
    Image,
    Video,
}

#[derive(Debug, Clone)]
pub struct MediaAsset {
    pub id: AssetId,
    pub asset_type: AssetType,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageQuality {
    /// A small, low-resolution thumbnail, suitable for grid views.
    Thumbnail,
    /// A full-screen, high-quality version of the image.
    Display,
}

#[derive(Debug, Clone)]
pub enum AssetData {
    Image {
        data: alloc::vec::Vec<u8>,
        format: ImageFormat, // e.g., Jpeg, Png, Heic
    },
    Video {
        /// A path to a local file that can be played or uploaded.
        /// The host is responsible for managing the lifecycle of this file.
        file_path: alloc::string::String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Heic,
}

pub trait MediaLibrary: Send + Sync {
    /// Queries the library for assets, sorted by creation date descending.
    fn query_assets(
        &self,
        asset_type: AssetType,
        limit: u32,
        offset: u32,
    ) -> Pin<
        alloc::boxed::Box<
            dyn Future<Output = Result<alloc::vec::Vec<MediaAsset>, PlatformError>> + Send + '_,
        >,
    >;

    /// Requests the image data for a given asset ID at a specific quality.
    /// The host is responsible for handling any necessary downloads from a cloud service (e.g., iCloud).
    fn request_image_data(
        &self,
        id: &AssetId,
        quality: ImageQuality,
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<AssetData, PlatformError>> + Send + '_>>;

    /// Requests the video data for a given asset ID.
    /// The host is responsible for transcoding or providing a file path to the video data.
    fn request_video_data(
        &self,
        id: &AssetId,
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<AssetData, PlatformError>> + Send + '_>>;
}
