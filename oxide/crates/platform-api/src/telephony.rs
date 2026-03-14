//! Telephony information service.

#[must_use]
pub fn normalize_country_iso(raw: &str) -> Option<alloc::string::String> {
    let trimmed = raw.trim();
    if trimmed.len() != 2 || !trimmed.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed.to_ascii_uppercase())
}

pub trait TelephonyService: Send + Sync {
    /// Returns the ISO 3166-1 country code (e.g., "US", "GB") for the user's
    /// home cellular provider, if available.
    fn home_country_iso_code(&self) -> Option<alloc::string::String>;
}

#[cfg(test)]
mod tests {
    use super::normalize_country_iso;

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
}
