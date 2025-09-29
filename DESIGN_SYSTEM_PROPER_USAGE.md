# Design System - Proper Usage (General-Purpose Library)

## Overview

The `design_system` module provides **general-purpose utilities** for building responsive design systems. It does NOT contain app-specific values like colors, font sizes, or spacing constants - those belong in your application layer.

---

## ✅ What's in the Library (General-Purpose)

### 1. **ScreenScale** - Responsive Font/Spacing Scaling

```rust
pub struct ScreenScale {
    pub scale_factor: f32,
}

impl ScreenScale {
    pub fn new(screen_width: f32, reference_width: f32) -> Self;
    pub fn scale(&self, raw_value: f32) -> f32;
}
```

**Purpose**: Scale any value proportionally based on screen width.

**Usage**:
```rust
// Create scaler (e.g., iPhone reference width)
let scaler = ScreenScale::new(screen_width, 414.0);

// Scale font sizes
let heading_font = scaler.scale(24.0);
let body_font = scaler.scale(16.0);

// Scale spacing
let padding = scaler.scale(12.0);
let margin = scaler.scale(20.0);
```

---

### 2. **GeometricScale** - Consistent Size Progression

```rust
pub struct GeometricScale {
    pub base: f32,
    pub ratio: f32,
    pub count: usize,
}

impl GeometricScale {
    pub fn new(base: f32, ratio: f32, count: usize) -> Self;
    pub fn at(&self, index: usize) -> f32;
    pub fn all(&self) -> Vec<f32>;
}
```

**Purpose**: Generate a series of sizes using geometric progression.

**Usage**:
```rust
// Create spacing scale (e.g., 8px base with 1.5x growth)
let spacing = GeometricScale::new(8.0, 1.5, 6);

let xs = spacing.at(0);  // 8.0
let sm = spacing.at(1);  // 12.0
let md = spacing.at(2);  // 18.0
let lg = spacing.at(3);  // 27.0
let xl = spacing.at(4);  // 40.5
let xxl = spacing.at(5); // 60.75

// Or get all at once
let all_sizes = spacing.all();
```

---

## ❌ What's NOT in the Library (App-Specific)

These belong in **your application code**, not OxideUI:
- ❌ Specific color palettes
- ❌ Named font presets (e.g., "heading", "body")
- ❌ Hardcoded spacing values
- ❌ App-specific sizing constants

---

## ✅ How to Build Your App's Design System

### Example: Nametag Design System (Application Layer)

```rust
// nametag/src/design/mod.rs
use oxideui_ui_core::design_system::{ScreenScale, GeometricScale};
use oxideui_renderer_api as gfx;

/// Nametag color palette
pub struct Colors;

impl Colors {
    pub const BASE: gfx::Color = gfx::Color::rgba(0.925, 0.941, 0.945, 1.0);
    pub const BASE_ALPHA: gfx::Color = gfx::Color::rgba(0.925, 0.941, 0.945, 0.5);
    pub const RED: gfx::Color = gfx::Color::rgba(0.965, 0.141, 0.353, 1.0);
    pub const HIGHLIGHT: gfx::Color = gfx::Color::rgba(1.0, 0.224, 0.459, 1.0);
    pub const EVENING: gfx::Color = gfx::Color::rgba(0.137, 0.145, 0.173, 1.0);
    pub const LINE: gfx::Color = gfx::Color::rgba(0.925, 0.941, 0.945, 0.1);
}

/// Nametag atomic sizing
pub struct Atoms {
    pub scaler: ScreenScale,
    pub sizes: GeometricScale,
    pub buffers: GeometricScale,
    pub screen_width: f32,
    pub screen_inset: f32,
}

impl Atoms {
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        let scaler = ScreenScale::new(screen_width, 414.0);
        let base_atom = scaler.scale(19.5);

        // Create 6 atom sizes with 1.40x growth
        let sizes = GeometricScale::new(base_atom, 1.40, 6);

        // Buffers are 32.5% of atoms
        let buffers = GeometricScale::new(base_atom * 0.325, 1.40, 6);

        let screen_inset = screen_width * 0.03;

        Self { scaler, sizes, buffers, screen_width, screen_inset }
    }

    // Named accessors for convenience
    pub fn very_very_small(&self) -> f32 { self.sizes.at(0) }
    pub fn very_small(&self) -> f32 { self.sizes.at(1) }
    pub fn small(&self) -> f32 { self.sizes.at(2) }
    pub fn medium(&self) -> f32 { self.sizes.at(3) }
    pub fn large(&self) -> f32 { self.sizes.at(4) }
    pub fn very_large(&self) -> f32 { self.sizes.at(5) }

    pub fn buffer_xs(&self) -> f32 { self.buffers.at(0) }
    pub fn buffer_sm(&self) -> f32 { self.buffers.at(1) }
    pub fn buffer_md(&self) -> f32 { self.buffers.at(2) }
    pub fn buffer_lg(&self) -> f32 { self.buffers.at(3) }

    pub fn max_atom(&self) -> f32 { self.screen_width - 2.0 * self.screen_inset }

    // App-specific atoms
    pub fn profile_photo(&self) -> f32 { self.large() }
    pub fn control_button(&self) -> f32 { self.small() * 0.75 }
    pub fn social_icon(&self) -> f32 { self.medium() * 0.80 }
}

/// Nametag font presets
pub struct Fonts {
    scaler: ScreenScale,
}

impl Fonts {
    pub fn new(screen_width: f32) -> Self {
        Self { scaler: ScreenScale::new(screen_width, 414.0) }
    }

    // Typography scale
    pub fn count(&self) -> f32 { self.scaler.scale(18.0) }
    pub fn label(&self) -> f32 { self.scaler.scale(7.0) }
    pub fn messenger(&self) -> f32 { self.scaler.scale(17.5) }
    pub fn heading(&self) -> f32 { self.scaler.scale(35.0) }
    pub fn profile_name(&self) -> f32 { self.scaler.scale(37.5) }
    pub fn body(&self) -> f32 { self.scaler.scale(17.5) }
    pub fn caption(&self) -> f32 { self.scaler.scale(12.5) }
}

/// Nametag animation timing
pub struct Timing;

impl Timing {
    pub const SCATTER_MS: u32 = 200;
    pub const SHAKE_CYCLE_MS: u32 = 35;
    pub const BADGE_BOUNCE_MS: u32 = 450;
    pub const RECORD_TIMEOUT_MS: u32 = 9000;
}
```

---

## 📖 Usage in Application

### Initialize Design System

```rust
// nametag/src/app.rs
use crate::design::{Atoms, Fonts, Colors, Timing};

struct NametagApp {
    atoms: Atoms,
    fonts: Fonts,
}

impl NametagApp {
    fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            atoms: Atoms::new(screen_width, screen_height),
            fonts: Fonts::new(screen_width),
        }
    }
}
```

### Use in Components

```rust
// Create button with your design tokens
let button = Button {
    text: String::from("Follow"),
    style: ButtonStyle {
        corner: atoms.buffer_sm(),
        pad_x: atoms.buffer_md(),
        pad_y: atoms.buffer_sm(),
        color: Colors::HIGHLIGHT,
        color_pressed: Colors::RED,
        text_px: fonts.body(),
        text_color: Colors::BASE,
        ..Default::default()
    },
};

// Size elements
let photo_size = atoms.profile_photo();
let spacing = atoms.buffer_md();

// Use colors
label.color = Colors::BASE;
divider.color = Colors::LINE;

// Use timing
let orchestrator = ScatterOrchestrator::new(Timing::SCATTER_MS);
```

---

## 🎯 Why This is Better

### ❌ Bad (App Logic in Library)
```rust
// In OxideUI library:
pub struct ColorPalette;
impl ColorPalette {
    pub const HIGHLIGHT: gfx::Color = gfx::Color::rgba(1.0, 0.224, 0.459, 1.0);
    pub const MESSENGER_FONT: f32 = 17.5;
}
```

**Problems**:
- Library knows about "messenger" and "highlight" (app concepts)
- Other apps can't use the library without these irrelevant constants
- Can't have different color schemes per app
- Library is polluted with app-specific names

### ✅ Good (Pure Tools in Library)
```rust
// In OxideUI library:
pub struct ScreenScale {
    pub fn scale(&self, value: f32) -> f32;
}

pub struct GeometricScale {
    pub fn at(&self, index: usize) -> f32;
}
```

**Benefits**:
- Library provides tools, not decisions
- Any app can use it
- App layer defines colors/fonts/sizes
- Library stays general-purpose

---

## 🏗️ Example: Different App Using Same Library

```rust
// some_other_app/src/design/mod.rs
use oxideui_ui_core::design_system::{ScreenScale, GeometricScale};

pub struct MyAppColors;
impl MyAppColors {
    pub const PRIMARY: gfx::Color = gfx::Color::rgba(0.2, 0.5, 0.9, 1.0); // Blue
    pub const ACCENT: gfx::Color = gfx::Color::rgba(0.9, 0.3, 0.2, 1.0);  // Orange
}

pub struct MyAppSizing {
    sizes: GeometricScale,
}

impl MyAppSizing {
    pub fn new(screen_width: f32) -> Self {
        let scaler = ScreenScale::new(screen_width, 375.0); // Different reference!
        let base = scaler.scale(16.0); // Different base!
        Self {
            sizes: GeometricScale::new(base, 1.25, 5), // Different ratio!
        }
    }

    pub fn tiny(&self) -> f32 { self.sizes.at(0) }
    pub fn small(&self) -> f32 { self.sizes.at(1) }
    pub fn medium(&self) -> f32 { self.sizes.at(2) }
    // ...
}
```

**Same OxideUI library, completely different design system.**

---

## 📐 Complete Example: Building Your Design System

```rust
// your_app/src/design_system.rs

use oxideui_ui_core::design_system::{ScreenScale, GeometricScale};
use oxideui_renderer_api as gfx;

// 1. Define your colors
pub struct AppColors;
impl AppColors {
    pub const TEXT: gfx::Color = gfx::Color::rgba(0.1, 0.1, 0.1, 1.0);
    pub const BACKGROUND: gfx::Color = gfx::Color::rgba(1.0, 1.0, 1.0, 1.0);
    pub const ACCENT: gfx::Color = gfx::Color::rgba(0.2, 0.6, 1.0, 1.0);
    // ... your colors
}

// 2. Build sizing system
pub struct AppSizing {
    pub screen_width: f32,
    pub screen_height: f32,
    pub spacing: GeometricScale,
    pub icon_sizes: GeometricScale,
    pub scaler: ScreenScale,
}

impl AppSizing {
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        let scaler = ScreenScale::new(screen_width, 375.0);

        // Spacing: 4, 8, 16, 32, 64
        let spacing = GeometricScale::new(4.0, 2.0, 5);

        // Icon sizes: 16, 24, 32, 48
        let base_icon = scaler.scale(16.0);
        let icon_sizes = GeometricScale::new(base_icon, 1.5, 4);

        Self { screen_width, screen_height, spacing, icon_sizes, scaler }
    }

    pub fn spacing_xs(&self) -> f32 { self.spacing.at(0) }
    pub fn spacing_sm(&self) -> f32 { self.spacing.at(1) }
    pub fn spacing_md(&self) -> f32 { self.spacing.at(2) }

    pub fn icon_sm(&self) -> f32 { self.icon_sizes.at(0) }
    pub fn icon_md(&self) -> f32 { self.icon_sizes.at(1) }
}

// 3. Typography
pub struct AppFonts {
    scaler: ScreenScale,
}

impl AppFonts {
    pub fn new(screen_width: f32) -> Self {
        Self { scaler: ScreenScale::new(screen_width, 375.0) }
    }

    pub fn display(&self) -> f32 { self.scaler.scale(32.0) }
    pub fn heading(&self) -> f32 { self.scaler.scale(24.0) }
    pub fn body(&self) -> f32 { self.scaler.scale(16.0) }
    pub fn caption(&self) -> f32 { self.scaler.scale(12.0) }
}

// 4. Use in your app
pub struct AppDesign {
    pub sizing: AppSizing,
    pub fonts: AppFonts,
}

impl AppDesign {
    pub fn new(screen_width: f32, screen_height: f32) -> Self {
        Self {
            sizing: AppSizing::new(screen_width, screen_height),
            fonts: AppFonts::new(screen_width),
        }
    }
}
```

---

## 🎨 Using Your Design System

```rust
// your_app/src/screens/home.rs
use crate::design_system::{AppDesign, AppColors};
use oxideui_ui_core::elements::{Button, ButtonStyle, Label};

fn build_home_screen(design: &AppDesign) {
    // Button with your design tokens
    let button = Button {
        text: String::from("Get Started"),
        style: ButtonStyle {
            corner: design.sizing.spacing_sm(),
            pad_x: design.sizing.spacing_md(),
            pad_y: design.sizing.spacing_sm(),
            color: AppColors::ACCENT,
            text_px: design.fonts.body(),
            text_color: AppColors::TEXT,
            ..Default::default()
        },
    };

    // Label
    let mut label = Label::default();
    label.text = String::from("Welcome");
    label.color = AppColors::TEXT;
    label.font_px = design.fonts.heading();

    // Spacing
    let margin = design.sizing.spacing_lg();
    let gap = design.sizing.spacing_md();
}
```

---

## 🔧 Advanced: Custom Scaling Functions

You can build more complex sizing systems in your app:

```rust
// your_app/src/design_system.rs

impl AppSizing {
    /// Golden ratio spacing
    pub fn golden_spacing(&self, index: usize) -> f32 {
        const PHI: f32 = 1.618;
        let scale = GeometricScale::new(8.0, PHI, 8);
        scale.at(index)
    }

    /// Screen-percentage sizing
    pub fn screen_percent(&self, percent: f32) -> f32 {
        self.screen_width * percent
    }

    /// Safe area with custom inset
    pub fn safe_width(&self, inset_percent: f32) -> f32 {
        let inset = self.screen_width * inset_percent;
        self.screen_width - 2.0 * inset
    }
}
```

---

## 📚 Real-World Examples

### Nametag's Design System (In App Layer)

```rust
// nametag/src/design/atoms.rs
use oxideui_ui_core::design_system::{ScreenScale, GeometricScale};

pub struct NametagAtoms {
    sizes: GeometricScale,
    buffers: GeometricScale,
}

impl NametagAtoms {
    pub fn new(screen_width: f32) -> Self {
        let scaler = ScreenScale::new(screen_width, 414.0);
        let base = scaler.scale(19.5);

        Self {
            sizes: GeometricScale::new(base, 1.40, 6),
            buffers: GeometricScale::new(base * 0.325, 1.40, 6),
        }
    }

    pub fn very_very_small(&self) -> f32 { self.sizes.at(0) }
    pub fn very_small(&self) -> f32 { self.sizes.at(1) }
    pub fn small(&self) -> f32 { self.sizes.at(2) }
    pub fn medium(&self) -> f32 { self.sizes.at(3) }
    pub fn large(&self) -> f32 { self.sizes.at(4) }
    pub fn very_large(&self) -> f32 { self.sizes.at(5) }

    pub fn small_buffer(&self) -> f32 { self.buffers.at(2) }
    pub fn medium_buffer(&self) -> f32 { self.buffers.at(3) }
}

// nametag/src/design/colors.rs
pub struct NametagColors;
impl NametagColors {
    pub const BASE: gfx::Color = gfx::Color::rgba(0.925, 0.941, 0.945, 1.0);
    pub const HIGHLIGHT: gfx::Color = gfx::Color::rgba(1.0, 0.224, 0.459, 1.0);
    pub const RED: gfx::Color = gfx::Color::rgba(0.965, 0.141, 0.353, 1.0);
}

// nametag/src/design/fonts.rs
pub struct NametagFonts {
    scaler: ScreenScale,
}

impl NametagFonts {
    pub fn new(screen_width: f32) -> Self {
        Self { scaler: ScreenScale::new(screen_width, 414.0) }
    }

    pub fn profile_name(&self) -> f32 { self.scaler.scale(37.5) }
    pub fn messenger(&self) -> f32 { self.scaler.scale(17.5) }
    pub fn count(&self) -> f32 { self.scaler.scale(18.0) }
}
```

---

## ✅ Summary

### OxideUI Provides (General Tools):
- ✅ `ScreenScale` - Proportional scaling utility
- ✅ `GeometricScale` - Size progression builder

### Your App Provides (Specific Values):
- ✅ Colors (your palette)
- ✅ Fonts (your typography)
- ✅ Sizes (your spacing/atoms)
- ✅ Timing (your animation durations)

**This separation keeps OxideUI general-purpose while giving you full design control.**

---

## 🎯 Migration Note

If you have existing design systems (like Nametag's LayoutMaster), create them in your app layer using OxideUI's utilities. Don't expect the library to provide them.

**Library = Tools. Application = Decisions.**