fn source_between<'a>(source: &'a str, start_marker: &str, end_marker: &str) -> &'a str {
    let start = source.find(start_marker).expect(start_marker);
    let end = source[start..].find(end_marker).expect(end_marker) + start;
    &source[start..end]
}

#[test]
fn camera_capture_without_audio_subscribers_uses_preview_only_mode() {
    let source = include_str!("../../platform-apple/src/lib.rs");
    let function = source_between(
        source,
        "fn camera_capture_start_mode(has_audio_subscribers: bool) -> CameraCaptureStartMode",
        "\n}\n\nfn remove_camera_subscriber",
    );

    assert!(function.contains("if has_audio_subscribers"));
    assert!(function.contains("CameraCaptureStartMode::Default"));
    assert!(function.contains("CameraCaptureStartMode::PreviewOnly"));
}
