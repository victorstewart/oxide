use oxideui_platform_api::{PermissionDomain, PermissionStatus};

fn encode_domain(domain: PermissionDomain) -> u32 {
    match domain {
        PermissionDomain::Notifications => 0,
        PermissionDomain::Location => 1,
        PermissionDomain::Camera => 2,
        PermissionDomain::Contacts => 3,
        PermissionDomain::Bluetooth => 4,
        PermissionDomain::Motion => 5,
        PermissionDomain::Microphone => 6,
        PermissionDomain::MediaLibrary => 7,
    }
}

fn decode_domain(value: u32) -> PermissionDomain {
    match value {
        0 => PermissionDomain::Notifications,
        1 => PermissionDomain::Location,
        2 => PermissionDomain::Camera,
        3 => PermissionDomain::Contacts,
        4 => PermissionDomain::Bluetooth,
        5 => PermissionDomain::Motion,
        6 => PermissionDomain::Microphone,
        7 => PermissionDomain::MediaLibrary,
        _ => PermissionDomain::Notifications,
    }
}

#[test]
fn permission_status_contract_matches_host() {
    assert_eq!(PermissionStatus::NotDetermined as u32, 0);
    assert_eq!(PermissionStatus::Denied as u32, 1);
    assert_eq!(PermissionStatus::Limited as u32, 2);
    assert_eq!(PermissionStatus::Authorized as u32, 3);
}

#[test]
fn permission_domain_round_trip() {
    for id in 0u32..=7u32 {
        let domain = decode_domain(id);
        assert_eq!(encode_domain(domain), id);
    }
}

#[test]
fn new_domains_are_encodable() {
    assert_eq!(encode_domain(PermissionDomain::Microphone), 6);
    assert_eq!(encode_domain(PermissionDomain::MediaLibrary), 7);
}
