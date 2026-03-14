//! Telephony information service.

pub trait TelephonyService: Send + Sync {
    /// Returns the ISO 3166-1 country code (e.g., "US", "GB") for the user's
    /// home cellular provider, if available.
    fn home_country_iso_code(&self) -> Option<alloc::string::String>;
}
