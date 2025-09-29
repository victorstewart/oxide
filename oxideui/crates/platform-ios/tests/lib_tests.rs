use oxideui_platform_ios::camera::{
    preset_catalog_from_caps, recommend_for_preset_from, recommend_from,
    resolution_catalog_from_formats, CamFormat, CamPixFmt, CameraPolicy, ResolutionCaps,
};

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn resolution_catalog_merges_formats() {
    let formats = vec![
        CamFormat { width: 1920, height: 1080, fps_min: 24.0, fps_max: 30.0, color_spaces_mask: 1 },
        CamFormat { width: 1920, height: 1080, fps_min: 30.0, fps_max: 60.0, color_spaces_mask: 2 },
        CamFormat { width: 1280, height: 720, fps_min: 24.0, fps_max: 30.0, color_spaces_mask: 1 },
    ];
    let caps = resolution_catalog_from_formats(&formats);
    assert_eq!(caps.len(), 2);

    let hd1080 = caps.iter().find(|c| c.height == 1080).unwrap();
    assert!(approx_eq(hd1080.fps_min, 24.0));
    assert!(approx_eq(hd1080.fps_max, 60.0));
    assert_eq!(hd1080.color_spaces_mask, 3);

    let hd720 = caps.iter().find(|c| c.height == 720).unwrap();
    assert!(approx_eq(hd720.fps_min, 24.0));
    assert!(approx_eq(hd720.fps_max, 30.0));
    assert_eq!(hd720.color_spaces_mask, 1);
}

#[test]
fn preset_catalog_groups_nearest() {
    let caps = vec![
        ResolutionCaps {
            width: 1280,
            height: 700,
            fps_min: 20.0,
            fps_max: 35.0,
            color_spaces_mask: 1,
        },
        ResolutionCaps {
            width: 1280,
            height: 730,
            fps_min: 24.0,
            fps_max: 60.0,
            color_spaces_mask: 2,
        },
        ResolutionCaps {
            width: 3840,
            height: 2160,
            fps_min: 30.0,
            fps_max: 60.0,
            color_spaces_mask: 1,
        },
    ];
    let presets = preset_catalog_from_caps(&caps);
    assert_eq!(presets.len(), 2);

    let preset720 = presets.iter().find(|p| p.preset_height == 720).unwrap();
    assert!(approx_eq(preset720.fps_min, 20.0));
    assert!(approx_eq(preset720.fps_max, 60.0));
    assert_eq!(preset720.color_spaces_mask, 3);

    let preset2160 = presets.iter().find(|p| p.preset_height == 2160).unwrap();
    assert!(approx_eq(preset2160.fps_min, 30.0));
    assert!(approx_eq(preset2160.fps_max, 60.0));
    assert_eq!(preset2160.color_spaces_mask, 1);
}

#[test]
fn recommend_camera_prefers_best_resolution() {
    let pix = vec![
        CamPixFmt { code: 0, bit_depth: 8, range: 0 },
        CamPixFmt { code: 1, bit_depth: 10, range: 0 },
    ];
    let caps = vec![
        ResolutionCaps {
            width: 1920,
            height: 1080,
            fps_min: 24.0,
            fps_max: 60.0,
            color_spaces_mask: 3,
        },
        ResolutionCaps {
            width: 1280,
            height: 720,
            fps_min: 24.0,
            fps_max: 60.0,
            color_spaces_mask: 1,
        },
    ];

    let rec = recommend_from(CameraPolicy::Camera, &pix, &caps).expect("profile");
    assert_eq!(rec.bit_depth, 10);
    assert_eq!(rec.color_space, 1);
    assert_eq!(rec.height, 1080);
    assert_eq!(rec.fps, 30);
}

#[test]
fn recommend_background_clamps_fps_and_bit_depth() {
    let pix = vec![CamPixFmt { code: 0, bit_depth: 10, range: 0 }];
    let caps = vec![ResolutionCaps {
        width: 1280,
        height: 720,
        fps_min: 35.0,
        fps_max: 35.0,
        color_spaces_mask: 1,
    }];

    let rec = recommend_from(CameraPolicy::Background, &pix, &caps).expect("profile");
    assert_eq!(rec.bit_depth, 8);
    assert_eq!(rec.color_space, 0);
    assert_eq!(rec.height, 720);
    assert_eq!(rec.fps, 35);
}

#[test]
fn recommend_returns_none_without_caps() {
    let pix = vec![CamPixFmt { code: 0, bit_depth: 8, range: 0 }];
    let caps: Vec<ResolutionCaps> = Vec::new();
    assert!(recommend_from(CameraPolicy::Camera, &pix, &caps).is_none());
}

#[test]
fn recommend_for_preset_prefers_p3_and_clamps() {
    let caps = vec![ResolutionCaps {
        width: 1920,
        height: 1080,
        fps_min: 24.0,
        fps_max: 30.0,
        color_spaces_mask: 3,
    }];
    let presets = preset_catalog_from_caps(&caps);
    let pix = vec![
        CamPixFmt { code: 0, bit_depth: 8, range: 0 },
        CamPixFmt { code: 1, bit_depth: 10, range: 0 },
    ];

    let rec = recommend_for_preset_from(1080, 120, true, true, &pix, &presets).expect("profile");
    assert_eq!(rec.bit_depth, 10);
    assert_eq!(rec.color_space, 1);
    assert_eq!(rec.height, 1080);
    assert_eq!(rec.fps, 30);
}

#[test]
fn recommend_for_preset_none_when_unavailable() {
    let caps = vec![ResolutionCaps {
        width: 1280,
        height: 720,
        fps_min: 24.0,
        fps_max: 60.0,
        color_spaces_mask: 1,
    }];
    let presets = preset_catalog_from_caps(&caps);
    let pix = vec![CamPixFmt { code: 0, bit_depth: 8, range: 0 }];

    assert!(recommend_for_preset_from(1440, 60, false, false, &pix, &presets).is_none());
}
