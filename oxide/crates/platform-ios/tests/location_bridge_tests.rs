#[test]
fn location_last_reads_native_cache_on_main_queue() {
    let source = include_str!("../src/ios/location.m");
    let function = source
        .split("uint8_t oxide_host_location_last")
        .nth(1)
        .expect("location last bridge")
        .split("int32_t oxide_host_location_set_accuracy")
        .next()
        .expect("location accuracy bridge marker");

    assert!(function.contains("if (out_ptr == NULL)"));
    assert!(function.contains("dispatch_main_sync(^"));
    assert!(function.contains("*out_ptr = g_last_sample;"));
    assert!(function.contains("return has_sample;"));
}
