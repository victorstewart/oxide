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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_scale_proportional() {
        let scale = ScreenScale::new(320.0, 414.0);
        let font_size = scale.scale(18.0);

        // 320/414 * 18 ≈ 13.91
        assert!((font_size - 13.91).abs() < 0.1);
    }

    #[test]
    fn geometric_scale_progression() {
        let scale = GeometricScale::new(10.0, 1.5, 5);

        assert!((scale.at(0) - 10.0).abs() < 0.01); // base
        assert!((scale.at(1) - 15.0).abs() < 0.01); // base * 1.5
        assert!((scale.at(2) - 22.5).abs() < 0.01); // base * 1.5^2
        assert!((scale.at(3) - 33.75).abs() < 0.01); // base * 1.5^3
    }

    #[test]
    fn geometric_scale_all() {
        let scale = GeometricScale::new(10.0, 2.0, 4);
        let sizes = scale.all();

        assert_eq!(sizes.len(), 4);
        assert!((sizes[0] - 10.0).abs() < 0.01);
        assert!((sizes[1] - 20.0).abs() < 0.01);
        assert!((sizes[2] - 40.0).abs() < 0.01);
        assert!((sizes[3] - 80.0).abs() < 0.01);
    }

    #[test]
    fn geometric_scale_clamping() {
        let scale = GeometricScale::new(10.0, 2.0, 3);

        // Index beyond count returns last value
        assert_eq!(scale.at(5), scale.at(2));
    }

    #[test]
    #[should_panic(expected = "screen_width must be positive")]
    fn screen_scale_invalid_width() {
        ScreenScale::new(0.0, 414.0);
    }

    #[test]
    #[should_panic(expected = "reference_width must be positive")]
    fn screen_scale_invalid_reference() {
        ScreenScale::new(320.0, 0.0);
    }

    #[test]
    #[should_panic(expected = "base must be positive")]
    fn geometric_scale_invalid_base() {
        GeometricScale::new(0.0, 1.5, 5);
    }

    #[test]
    #[should_panic(expected = "ratio must be positive")]
    fn geometric_scale_invalid_ratio() {
        GeometricScale::new(10.0, -1.5, 5);
    }

    #[test]
    #[should_panic(expected = "count must be at least 1")]
    fn geometric_scale_invalid_count() {
        GeometricScale::new(10.0, 1.5, 0);
    }
}
