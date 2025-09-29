//! Contacts API for accessing device contacts
//! Used for social graph calculation and friend discovery

use alloc::string::String;
use alloc::vec::Vec;

/// Contact phone number with region
#[derive(Clone, Debug)]
pub struct ContactPhone {
    pub number: String,
    pub region_code: Option<String>, // ISO country code (e.g., "US", "GB")
    pub normalized: Option<String>,  // E.164 format if parseable
}

/// Contact email address
#[derive(Clone, Debug)]
pub struct ContactEmail {
    pub address: String,
    pub is_valid: bool,
}

/// A single contact from the device's address book
#[derive(Clone, Debug)]
pub struct Contact {
    pub identifier: String, // Platform-specific unique ID
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub phones: Vec<ContactPhone>,
    pub emails: Vec<ContactEmail>,
}

impl Contact {
    /// Get full name (given + family)
    pub fn full_name(&self) -> String {
        match (&self.given_name, &self.family_name) {
            (Some(g), Some(f)) => alloc::format!("{} {}", g, f),
            (Some(g), None) => g.clone(),
            (None, Some(f)) => f.clone(),
            (None, None) => String::new(),
        }
    }

    /// Get all contact bits (phones + emails) for matching
    pub fn contact_bits(&self) -> Vec<String> {
        let mut bits = Vec::new();

        // Add normalized phone numbers
        for phone in &self.phones {
            if let Some(normalized) = &phone.normalized {
                bits.push(normalized.clone());
            } else {
                bits.push(phone.number.clone());
            }
        }

        // Add validated emails
        for email in &self.emails {
            if email.is_valid {
                bits.push(email.address.to_lowercase());
            }
        }

        bits
    }
}

/// Result of fetching contacts
#[derive(Clone, Debug)]
pub enum ContactsFetchResult {
    Success { contacts: Vec<Contact>, waypoint: Option<String> },
    Denied,
    Error(String),
}

/// Contact change event
#[derive(Clone, Debug)]
pub enum ContactChange {
    Added(Contact),
    Updated(Contact),
    Deleted { identifier: String },
}

/// Platform-agnostic contacts manager trait
pub trait ContactsManager: Send + Sync {
    /// Fetch all contacts
    ///
    /// If `waypoint` is provided, only fetch changes since that waypoint.
    /// Returns new waypoint for incremental updates.
    fn fetch_contacts(&mut self, waypoint: Option<String>) -> ContactsFetchResult;

    /// Subscribe to contact changes
    ///
    /// Returns a subscription ID that can be used to unsubscribe
    fn subscribe_to_changes<F>(&mut self, callback: F) -> u32
    where
        F: Fn(ContactChange) + Send + 'static;

    /// Unsubscribe from contact changes
    fn unsubscribe(&mut self, subscription_id: u32);

    /// Get carrier region code (for phone number parsing)
    fn carrier_region_code(&self) -> Option<String>;
}