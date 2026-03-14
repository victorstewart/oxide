# platform-api `telephony.rs`

## Intention and purpose
- Define Oxide's generic telephony service contract.
- Centralize ISO country-code normalization so host shims and platform implementations do not re-implement carrier-country parsing.

## Entry points list
- `TelephonyService`
  Generic trait for reading telephony-derived country information.
- `normalize_country_iso(raw)`
  Shared helper that trims, validates, and uppercases ISO 3166-1 alpha-2 country codes.

## Logic narrative
- `normalize_country_iso` is intentionally narrow: it accepts only two ASCII alphabetic characters and returns an uppercase owned string.
- Platform adapters use it after reading raw carrier/home-region values from the host OS or environment overrides.

## Testing and benchmarks
- Covered by unit tests in `crates/platform-api/src/telephony.rs`.

## Changelog
- 2026-03-12: moved generic country-code normalization into Oxide so iOS telephony services and app override shims share one implementation.
