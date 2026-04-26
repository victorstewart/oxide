//! General-purpose design system utilities
//!
//! Provides screen-relative scaling and geometric sizing helpers.

/// Screen-relative font scaling utility
///
/// Scales font sizes proportionally based on screen width.
#[derive(Clone, Copy, Debug)]
pub struct ScreenScale {
    pub scale_factor: f32,
}

impl ScreenScale {
    /// Create scaler for given screen width and reference width
    pub fn new(screen_width: f32, reference_width: f32) -> Self {
        assert!(screen_width > 0.0, "screen_width must be positive");
        assert!(reference_width > 0.0, "reference_width must be positive");
        let scale_factor = screen_width / reference_width.max(1.0);
        Self { scale_factor }
    }

    /// Scale a value (typically font size or spacing)
    pub fn scale(&self, raw_value: f32) -> f32 {
        raw_value * self.scale_factor
    }
}

/// Geometric size progression builder
///
/// Creates a series of sizes using geometric progression (e.g., 1.40x growth).
/// Useful for creating consistent spacing/sizing systems.
#[derive(Clone, Debug)]
pub struct GeometricScale {
    pub base: f32,
    pub ratio: f32,
    pub count: usize,
}

impl GeometricScale {
    /// Create geometric scale with base size and growth ratio
    pub fn new(base: f32, ratio: f32, count: usize) -> Self {
        assert!(base > 0.0, "base must be positive");
        assert!(ratio > 0.0, "ratio must be positive");
        assert!(count > 0, "count must be at least 1");
        Self { base, ratio, count }
    }

    /// Get size at index (0 = base, 1 = base*ratio, 2 = base*ratio^2, etc.)
    pub fn at(&self, index: usize) -> f32 {
        if index >= self.count {
            return self.base * self.ratio.powi((self.count - 1) as i32);
        }
        self.base * self.ratio.powi(index as i32)
    }

    /// Get all sizes as a vector
    pub fn all(&self) -> alloc::vec::Vec<f32> {
        (0..self.count).map(|i| self.at(i)).collect()
    }
}
