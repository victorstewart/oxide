//! Oxide Harness Registry
//!
//! Provides a compile-time inventory of all UI components and named animations.
//! This crate is metadata-only and has no runtime dependencies. Harnesses can
//! use these IDs to drive rendering, snapshots, and perf sweeps.

#![forbid(unsafe_code)]
#![allow(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::match_same_arms, clippy::enum_glob_use)]

extern crate alloc;

// ===== Components =====

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComponentId {
    Label,
    ProgressBar,
    Spinner,
    Button,
    Toggle,
    Slider,
    ImageView,
    NineSliceImage,
    CollectionView,
}

pub struct ComponentSpec {
    pub id: ComponentId,
    pub name: &'static str,
    /// Variants are stylistic or configuration variants (e.g., fit modes).
    pub variants: &'static [&'static str],
    /// States are interaction states applicable to this component.
    pub states: &'static [&'static str],
}

// Variant and state lists per component
const LABEL_VARIANTS: &[&str] = &["default", "center", "right", "wrap"];
const LABEL_STATES: &[&str] = &[];

const PROGRESS_VARIANTS: &[&str] = &["determinate", "indeterminate"];
const PROGRESS_STATES: &[&str] = &[];

const SPINNER_VARIANTS: &[&str] = &["default", "thick", "faint"];
const SPINNER_STATES: &[&str] = &[];

const BUTTON_VARIANTS: &[&str] = &["primary"];
const BUTTON_STATES: &[&str] = &["default", "hover", "pressed", "disabled"];

const TOGGLE_VARIANTS: &[&str] = &["default"];
const TOGGLE_STATES: &[&str] = &["off", "on", "dragging"];

const SLIDER_VARIANTS: &[&str] = &["default", "stepped"];
const SLIDER_STATES: &[&str] = &["idle", "dragging"];

const IMAGEVIEW_VARIANTS: &[&str] = &["contain", "cover", "stretch", "alpha50"];
const IMAGEVIEW_STATES: &[&str] = &[];

const NINE_SLICE_VARIANTS: &[&str] = &["default"];
const NINE_SLICE_STATES: &[&str] = &[];

const COLLECTION_VARIANTS: &[&str] = &["grid", "row"];
const COLLECTION_STATES: &[&str] = &["idle", "hovered", "focused"];

pub const COMPONENTS: &[ComponentSpec] = &[
    ComponentSpec {
        id: ComponentId::Label,
        name: "Label",
        variants: LABEL_VARIANTS,
        states: LABEL_STATES,
    },
    ComponentSpec {
        id: ComponentId::ProgressBar,
        name: "ProgressBar",
        variants: PROGRESS_VARIANTS,
        states: PROGRESS_STATES,
    },
    ComponentSpec {
        id: ComponentId::Spinner,
        name: "Spinner",
        variants: SPINNER_VARIANTS,
        states: SPINNER_STATES,
    },
    ComponentSpec {
        id: ComponentId::Button,
        name: "Button",
        variants: BUTTON_VARIANTS,
        states: BUTTON_STATES,
    },
    ComponentSpec {
        id: ComponentId::Toggle,
        name: "Toggle",
        variants: TOGGLE_VARIANTS,
        states: TOGGLE_STATES,
    },
    ComponentSpec {
        id: ComponentId::Slider,
        name: "Slider",
        variants: SLIDER_VARIANTS,
        states: SLIDER_STATES,
    },
    ComponentSpec {
        id: ComponentId::ImageView,
        name: "ImageView",
        variants: IMAGEVIEW_VARIANTS,
        states: IMAGEVIEW_STATES,
    },
    ComponentSpec {
        id: ComponentId::NineSliceImage,
        name: "NineSliceImage",
        variants: NINE_SLICE_VARIANTS,
        states: NINE_SLICE_STATES,
    },
    ComponentSpec {
        id: ComponentId::CollectionView,
        name: "CollectionView",
        variants: COLLECTION_VARIANTS,
        states: COLLECTION_STATES,
    },
];

pub fn components() -> &'static [ComponentSpec] {
    COMPONENTS
}

pub fn component_by_id(id: ComponentId) -> Option<&'static ComponentSpec> {
    COMPONENTS.iter().find(|c| c.id == id)
}

pub fn component_by_name(name: &str) -> Option<&'static ComponentSpec> {
    COMPONENTS.iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

// ===== Animations =====

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationId {
    SpinnerSpin,
    ProgressIndeterminate,
    ButtonPressScale,
    ToggleThumbSpring,
    SliderThumbMove,
    ImageZoomPan,
    AnimTimelineBars,
}

pub struct AnimationSpec {
    pub id: AnimationId,
    pub name: &'static str,
    /// Canonical sampling times in milliseconds for snapshot-based tests.
    pub sample_ms: &'static [u32],
}

const DEFAULT_SAMPLES: &[u32] = &[0, 33, 100, 250, 500, 1000];

pub const ANIMATIONS: &[AnimationSpec] = &[
    AnimationSpec { id: AnimationId::SpinnerSpin, name: "SpinnerSpin", sample_ms: DEFAULT_SAMPLES },
    AnimationSpec {
        id: AnimationId::ProgressIndeterminate,
        name: "ProgressIndeterminate",
        sample_ms: DEFAULT_SAMPLES,
    },
    AnimationSpec {
        id: AnimationId::ButtonPressScale,
        name: "ButtonPressScale",
        sample_ms: DEFAULT_SAMPLES,
    },
    AnimationSpec {
        id: AnimationId::ToggleThumbSpring,
        name: "ToggleThumbSpring",
        sample_ms: DEFAULT_SAMPLES,
    },
    AnimationSpec {
        id: AnimationId::SliderThumbMove,
        name: "SliderThumbMove",
        sample_ms: DEFAULT_SAMPLES,
    },
    AnimationSpec {
        id: AnimationId::ImageZoomPan,
        name: "ImageZoomPan",
        sample_ms: DEFAULT_SAMPLES,
    },
    AnimationSpec {
        id: AnimationId::AnimTimelineBars,
        name: "AnimTimelineBars",
        sample_ms: DEFAULT_SAMPLES,
    },
];

pub fn animations() -> &'static [AnimationSpec] {
    ANIMATIONS
}

pub fn animation_by_id(id: AnimationId) -> Option<&'static AnimationSpec> {
    ANIMATIONS.iter().find(|a| a.id == id)
}

pub fn animation_by_name(name: &str) -> Option<&'static AnimationSpec> {
    ANIMATIONS.iter().find(|a| a.name.eq_ignore_ascii_case(name))
}
