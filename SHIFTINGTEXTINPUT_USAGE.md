# ShiftingTextInput - Proper Usage Guide

## Overview

The `ShiftingTextInput` is now a **general-purpose** text input component with animated prompt behavior. All field-specific logic (placeholders, validation rules, max lengths) is configured by the application, not hardcoded in the library.

---

## ✅ What's Now Configurable

```rust
pub struct ShiftingTextInput {
    pub placeholder: String,           // What to show when empty
    pub prompt: Option<String>,        // Label that shifts above when focused
    pub max_length: Option<usize>,     // Character limit
    pub filter: CharFilter,            // What characters to allow
    pub style: ShiftingTextInputStyle, // Visual styling
}
```

---

## Character Filtering

### Built-in Filters

```rust
pub enum CharFilter {
    None,                              // Allow all characters
    Alphabetic,                        // Only letters
    AlphabeticHyphen,                  // Letters + hyphen
    Digits,                            // 0-9 only
    Alphanumeric,                      // Letters + numbers
    AlphanumericPlus(String),          // Alphanumeric + custom chars
    Custom(Arc<dyn Fn(char) -> bool>), // Custom validator function
}
```

### Examples

```rust
// Username field (alphanumeric + underscore)
let username_input = ShiftingTextInput {
    placeholder: String::from("username"),
    prompt: Some(String::from("username")),
    max_length: Some(15),
    filter: CharFilter::AlphanumericPlus(String::from("_")),
    ..Default::default()
};

// Firstname (letters only)
let firstname_input = ShiftingTextInput {
    placeholder: String::from("first"),
    prompt: None,
    max_length: Some(20),
    filter: CharFilter::Alphabetic,
    ..Default::default()
};

// Bio (alphanumeric + special chars)
let bio_input = ShiftingTextInput {
    placeholder: String::from("bio"),
    prompt: None,
    max_length: Some(150),
    filter: CharFilter::AlphanumericPlus(String::from("@-&(),.' +")),
    ..Default::default()
};

// Mobile (digits only)
let mobile_input = ShiftingTextInput {
    placeholder: String::from("mobile"),
    prompt: Some(String::from("mobile")),
    max_length: Some(15),
    filter: CharFilter::Digits,
    ..Default::default()
};

// Password (no restrictions)
let password_input = ShiftingTextInput {
    placeholder: String::from("password"),
    prompt: Some(String::from("password")),
    max_length: Some(128),
    filter: CharFilter::None,
    ..Default::default()
};
```

---

## Custom Validators

For complex validation, use `CharFilter::Custom`:

```rust
use alloc::sync::Arc;

// Snapchat username: must start with letter, only alphanumeric + _-.
let snapchat_filter = CharFilter::Custom(Arc::new(|ch: char| {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.'
}));

let snapchat_input = ShiftingTextInput {
    placeholder: String::from("username"),
    prompt: None,
    max_length: Some(15),
    filter: snapchat_filter,
    ..Default::default()
};
```

---

## State Management

```rust
let mut state = ShiftingTextInputState::default();

// User types
let user_input = String::from("john_doe123!");
state.set_text(user_input, &username_input.filter, username_input.max_length);
// Result: "john_doe123" (! filtered out)

// Focus handling
state.on_focus();  // Prompt shifts up
state.on_blur();   // Prompt shifts back if empty

// Validation failure
if !is_valid(&state.text) {
    state.fail();  // Triggers shake + red background
}
state.clear_fail(); // Reset to normal

// Animation update (call every frame)
state.tick();
```

---

## Rendering

```rust
shifting_input.encode(
    &state,
    rect,
    device_scale,
    text_ctx,
    uploader,
    builder,
);
```

---

## Nametag-Specific Configuration (Application Layer)

In your Nametag app code, you would create helper functions:

```rust
// nametag/src/ui/text_fields.rs
use oxideui_ui_core::elements::{ShiftingTextInput, CharFilter};

pub fn username_field() -> ShiftingTextInput {
    ShiftingTextInput {
        placeholder: String::from("username"),
        prompt: Some(String::from("username")),
        max_length: Some(15),
        filter: CharFilter::AlphanumericPlus(String::from("_")),
        ..Default::default()
    }
}

pub fn firstname_field() -> ShiftingTextInput {
    ShiftingTextInput {
        placeholder: String::from("first"),
        prompt: None,
        max_length: Some(20),
        filter: CharFilter::Alphabetic,
        ..Default::default()
    }
}

pub fn bio_field() -> ShiftingTextInput {
    ShiftingTextInput {
        placeholder: String::from("bio"),
        prompt: None,
        max_length: Some(150),
        filter: CharFilter::AlphanumericPlus(String::from("@-&(),.' +")),
        ..Default::default()
    }
}

pub fn social_field(platform: SocialPlatform) -> ShiftingTextInput {
    let (placeholder, max_length) = match platform {
        SocialPlatform::Youtube => ("channel name", 20),
        SocialPlatform::Instagram => ("username", 30),
        SocialPlatform::Twitter => ("username", 15),
        SocialPlatform::Snapchat => ("username", 15),
        // ...
    };

    ShiftingTextInput {
        placeholder: String::from(placeholder),
        prompt: None,
        max_length: Some(max_length),
        filter: CharFilter::None, // Or platform-specific
        ..Default::default()
    }
}
```

---

## Animation Details

### Prompt Shift Animation
- **Duration**: Smooth interpolation at 15% per frame
- **Trigger**: Focus or text present
- **Target**: 0.0 (centered placeholder) → 1.0 (prompt above)

### Shake Animation
- **Duration**: 35ms per cycle
- **Cycles**: 6 (12 movements total)
- **Amplitude**: ±2pt horizontal
- **Trigger**: `state.fail()` call

---

## Styling

```rust
let mut input = username_field();
input.style.font_px = 20.0;
input.style.prompt_font_px = 12.0;
input.style.text_color = gfx::Color::rgba(1.0, 1.0, 1.0, 1.0); // White
input.style.background_focus = gfx::Color::rgba(0.2, 0.2, 0.3, 1.0); // Dark blue
input.style.corner = 12.0;
```

---

## Key Differences from Old Implementation

### ❌ Before (Bad - App Logic in Library)
```rust
pub enum ShiftingTextFieldType {
    Firstname, Lastname, Bio, Username, // ... 14 variants
}

impl ShiftingTextFieldType {
    pub fn max_length(&self) -> Option<usize> { /* hardcoded */ }
    pub fn placeholder(&self) -> &'static str { /* hardcoded */ }
    pub fn allows_char(&self, ch: char) -> bool { /* hardcoded */ }
}
```

### ✅ After (Good - Pure Configuration)
```rust
pub struct ShiftingTextInput {
    pub placeholder: String,
    pub prompt: Option<String>,
    pub max_length: Option<usize>,
    pub filter: CharFilter,
    pub style: ShiftingTextInputStyle,
}
```

**Why This is Better:**
1. **Library stays general-purpose** - No social media platform names in core UI code
2. **Flexible** - Any app can configure for their needs
3. **Extensible** - Custom validators via `CharFilter::Custom`
4. **Maintainable** - Platform-specific logic lives in app layer
5. **Testable** - Easy to test different configurations

---

## Migration Notes

If you have existing code using the old enum-based API:

```rust
// Old
let input = ShiftingTextInput {
    field_type: ShiftingTextFieldType::Username,
    ..Default::default()
};

// New
let input = ShiftingTextInput {
    placeholder: String::from("username"),
    prompt: Some(String::from("username")),
    max_length: Some(15),
    filter: CharFilter::AlphanumericPlus(String::from("_")),
    ..Default::default()
};

// Or use app-layer helper
let input = nametag::ui::username_field();
```

---

## Complete Example

```rust
use oxideui_ui_core::elements::{ShiftingTextInput, ShiftingTextInputState, CharFilter};

// Create input
let username_input = ShiftingTextInput {
    placeholder: String::from("username"),
    prompt: Some(String::from("username")),
    max_length: Some(15),
    filter: CharFilter::AlphanumericPlus(String::from("_")),
    ..Default::default()
};

// Create state
let mut state = ShiftingTextInputState::default();

// User interaction loop
loop {
    // Handle user input
    if let Some(input) = get_keyboard_input() {
        state.set_text(input, &username_input.filter, username_input.max_length);
    }

    // Handle focus
    if focus_changed {
        if focused {
            state.on_focus();
        } else {
            state.on_blur();
        }
    }

    // Validate
    if submit_pressed {
        if state.text.len() < 3 {
            state.fail(); // Too short
        } else {
            // Success
            submit_username(&state.text);
        }
    }

    // Update animations
    state.tick();

    // Render
    username_input.encode(&state, rect, device_scale, text_ctx, uploader, builder);
}
```

---

## Summary

The `ShiftingTextInput` is now a **properly designed library component** that:
- ✅ Has no app-specific logic
- ✅ Is fully configurable
- ✅ Supports custom validation
- ✅ Maintains the same animation behavior
- ✅ Follows OxideUI architecture patterns

All Nametag-specific configurations belong in your application layer, not the core UI library.