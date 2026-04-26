use oxide_platform_api::telephony::normalize_country_iso;

#[test]
fn normalize_country_iso_accepts_alpha_two_code() {
   assert_eq!(normalize_country_iso("us"), Some("US".to_owned()));
   assert_eq!(normalize_country_iso("GB"), Some("GB".to_owned()));
}

#[test]
fn normalize_country_iso_rejects_invalid_values() {
   assert_eq!(normalize_country_iso(""), None);
   assert_eq!(normalize_country_iso("USA"), None);
   assert_eq!(normalize_country_iso("1A"), None);
}
