use crate::{elements, DrawListBuilder};
use oxideui_permissions::PermissionState;
use oxideui_platform_api::{PermissionDomain, PermissionStatus};
use oxideui_renderer_api as gfx;
use std::collections::BTreeMap;

struct PermissionDescriptor {
    title: &'static str,
    message_request: &'static str,
    message_denied: &'static str,
    cta_request: &'static str,
    cta_denied: &'static str,
}

fn descriptors() -> BTreeMap<PermissionDomain, PermissionDescriptor> {
    use PermissionDomain::*;
    let mut map = BTreeMap::new();
    map.insert(
        Camera,
        PermissionDescriptor {
            title: "Camera Access Required",
            message_request: "Allow access so OxideUI can show live preview and capture media.",
            message_denied:
                "Camera access is disabled. Enable it in Settings to continue previewing.",
            cta_request: "Allow Camera",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
        Microphone,
        PermissionDescriptor {
            title: "Microphone Access Required",
            message_request: "Enable audio capture to record videos with sound.",
            message_denied:
                "Microphone access is disabled. Re-enable it from Settings to record audio.",
            cta_request: "Allow Microphone",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
      Location,
      PermissionDescriptor
      {
         title: "Location Access",
         message_request: "We use location for tagging captures and nearby device discovery.",
         message_denied: "Location access denied. Allow precise location in Settings for full functionality.",
         cta_request: "Allow Location",
         cta_denied: "Open Settings"
      }
   );
    map.insert(
        Bluetooth,
        PermissionDescriptor {
            title: "Bluetooth Access",
            message_request: "Bluetooth is needed for nearby device pairing and sensors.",
            message_denied:
                "Bluetooth access disabled. Enable it in Settings to reconnect accessories.",
            cta_request: "Allow Bluetooth",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
        Motion,
        PermissionDescriptor {
            title: "Motion & Fitness",
            message_request: "Allow motion data for altitude and activity overlays.",
            message_denied: "Motion data denied. Enable it in Settings to resume sensor fusion.",
            cta_request: "Allow Motion",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
        Notifications,
        PermissionDescriptor {
            title: "Notifications",
            message_request:
                "Enable notifications so we can alert you about capture status and sharing.",
            message_denied:
                "Notifications disabled. Turn them back on in Settings for timely updates.",
            cta_request: "Allow Notifications",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
        Contacts,
        PermissionDescriptor {
            title: "Contacts",
            message_request: "Contacts access lets you share media with teammates quickly.",
            message_denied: "Contacts access denied. Enable in Settings to use quick share.",
            cta_request: "Allow Contacts",
            cta_denied: "Open Settings",
        },
    );
    map.insert(
        MediaLibrary,
        PermissionDescriptor {
            title: "Media Library",
            message_request: "Allow photo library access to import reference shots and exports.",
            message_denied: "Media library denied. Enable in Settings to browse captured media.",
            cta_request: "Allow Media Library",
            cta_denied: "Open Settings",
        },
    );
    map
}

fn descriptor(domain: PermissionDomain) -> &'static PermissionDescriptor {
    static DESCRIPTORS: once_cell::sync::Lazy<BTreeMap<PermissionDomain, PermissionDescriptor>> =
        once_cell::sync::Lazy::new(descriptors);
    DESCRIPTORS.get(&domain).unwrap_or(&PermissionDescriptor {
        title: "Permission",
        message_request: "Allow access to continue.",
        message_denied: "Permission denied. Update Settings to continue.",
        cta_request: "Allow",
        cta_denied: "Open Settings",
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PermissionPrompt {
    pub domain: PermissionDomain,
    pub status: PermissionStatus,
    pub last_changed_ms: u64,
}

impl From<PermissionState> for PermissionPrompt {
    fn from(state: PermissionState) -> Self {
        Self { domain: state.domain, status: state.status, last_changed_ms: state.last_changed_ms }
    }
}

pub struct PermissionOverlayUi {
    prompt: Option<PermissionPrompt>,
    button_state: elements::ButtonState,
    last_card: Option<gfx::RectF>,
    last_button: Option<gfx::RectF>,
}

impl Default for PermissionOverlayUi {
    fn default() -> Self {
        Self {
            prompt: None,
            button_state: elements::ButtonState::default(),
            last_card: None,
            last_button: None,
        }
    }
}

impl PermissionOverlayUi {
    pub fn update(&mut self, states: &[PermissionState]) {
        const ORDER: [PermissionDomain; 8] = [
            PermissionDomain::Camera,
            PermissionDomain::Microphone,
            PermissionDomain::Location,
            PermissionDomain::Bluetooth,
            PermissionDomain::Motion,
            PermissionDomain::Notifications,
            PermissionDomain::Contacts,
            PermissionDomain::MediaLibrary,
        ];
        self.prompt = ORDER
            .iter()
            .filter_map(|target| {
                states
                    .iter()
                    .find(|state| {
                        state.domain == *target && state.status != PermissionStatus::Authorized
                    })
                    .copied()
            })
            .map(PermissionPrompt::from)
            .next();
        if self.prompt.is_none() {
            self.button_state.on_pointer_cancel();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.prompt.is_some()
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut elements::TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        let Some(prompt) = self.prompt else {
            self.last_card = None;
            self.last_button = None;
            return;
        };
        let desc = descriptor(prompt.domain);
        let width = (viewport.w - 48.0).min(360.0).max(260.0);
        let height = 220.0;
        let x = viewport.x + (viewport.w - width) * 0.5;
        let y = viewport.y + (viewport.h - height) * 0.5;
        let card = gfx::RectF::new(x, y, width, height);
        self.last_card = Some(card);
        builder.backdrop(card, 18.0, gfx::Color::rgba(0.0, 0.0, 0.0, 1.0), 0.55);
        builder.rrect(card, [18.0; 4], gfx::Color::rgba(0.11, 0.12, 0.14, 0.92));

        let title = elements::Label {
            text: desc.title.into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 0.95),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 16.0,
        };
        title.encode(
            gfx::RectF::new(card.x + 20.0, card.y + 20.0, card.w - 40.0, 22.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        let message = match prompt.status {
            PermissionStatus::Denied => desc.message_denied,
            PermissionStatus::Limited => desc.message_denied,
            PermissionStatus::NotDetermined => desc.message_request,
            PermissionStatus::Authorized => "",
        };
        let body = elements::Label {
            text: message.into(),
            color: gfx::Color::rgba(0.82, 0.85, 0.90, 0.95),
            align: elements::Align::Left,
            wrap: true,
            font_id: 0,
            font_px: 13.0,
        };
        body.encode(
            gfx::RectF::new(card.x + 20.0, card.y + 56.0, card.w - 40.0, 84.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        let button_label =
            if matches!(prompt.status, PermissionStatus::Denied | PermissionStatus::Limited) {
                desc.cta_denied
            } else {
                desc.cta_request
            };
        let button_text = alloc::string::String::from(button_label);
        let button_rect =
            gfx::RectF::new(card.x + 20.0, card.y + card.h - 64.0, card.w - 40.0, 44.0);
        self.last_button = Some(button_rect);
        let button_style = elements::ButtonStyle {
            corner: 12.0,
            pad_x: 16.0,
            pad_y: 12.0,
            color: gfx::Color::rgba(0.28, 0.54, 0.96, 1.0),
            color_pressed: gfx::Color::rgba(0.24, 0.48, 0.88, 1.0),
            color_disabled: gfx::Color::rgba(0.36, 0.42, 0.52, 1.0),
            text_px: 15.0,
            text_color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            press_animation_ms: 100,
        };
        let button = elements::Button { text: button_text.clone(), style: button_style };
        let label = elements::Label {
            text: button_text,
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: elements::Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 15.0,
        };
        button.encode(button_rect, device_scale, text, uploader, &self.button_state, builder);
        label.encode(
            gfx::RectF::new(button_rect.x, button_rect.y + 12.0, button_rect.w, 18.0),
            device_scale,
            text,
            uploader,
            builder,
        );
    }

    pub fn pointer_event(&mut self, x: f32, y: f32, buttons: u32) -> Option<PermissionDomain> {
        let Some(prompt) = self.prompt else { return None };
        let Some(button) = self.last_button else { return None };
        let inside =
            x >= button.x && x <= button.x + button.w && y >= button.y && y <= button.y + button.h;
        if buttons & 1 != 0 {
            if inside {
                self.button_state.on_pointer_down();
            } else {
                self.button_state.on_pointer_cancel();
            }
            return None;
        }
        if self.button_state.is_pressed() {
            let tapped = self.button_state.on_pointer_up();
            if tapped && inside {
                return Some(prompt.domain);
            }
        }
        None
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        if let Some(card) = self.last_card {
            return x >= card.x && x <= card.x + card.w && y >= card.y && y <= card.y + card.h;
        }
        false
    }
}
