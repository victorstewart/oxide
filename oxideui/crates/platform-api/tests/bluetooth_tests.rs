use oxideui_platform_api::{
    AdvertisementData, BleCacheEntry, BleUuid, BluetoothEvent, PeripheralInfo, ScanOptions,
};

#[test]
fn scan_options_defaults_allow_duplicates_false() {
    let opts = ScanOptions::default();
    assert!(opts.services.is_empty());
    assert!(!opts.allow_duplicates);
}

#[test]
fn advertisement_data_connectable_roundtrip() {
    let adv = AdvertisementData {
        services: vec![BleUuid([1; 16])],
        manufacturer_data: Some(vec![0xAA, 0xBB]),
        connectable: true,
    };
    assert_eq!(adv.services.len(), 1);
    assert_eq!(adv.manufacturer_data.as_ref().map(|d| d.len()), Some(2));
    assert!(adv.connectable);
}

#[test]
fn bluetooth_event_cache_updated_wraps_entry() {
    let entry = BleCacheEntry {
        peripheral: PeripheralInfo {
            id: 7,
            name: Some("demo".into()),
            rssi_dbm: -40,
            advertisement: AdvertisementData {
                services: vec![],
                manufacturer_data: None,
                connectable: true,
            },
        },
        last_seen_ms: 123,
    };
    match BluetoothEvent::CacheUpdated(entry.clone()) {
        BluetoothEvent::CacheUpdated(e) => {
            assert_eq!(e.last_seen_ms, 123);
            assert_eq!(e.peripheral.name.as_deref(), Some("demo"));
        }
        _ => panic!("expected cache update"),
    }
}
