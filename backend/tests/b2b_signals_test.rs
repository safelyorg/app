mod common;

use crate::common::{make_listing, make_supplier};
use backend::services::signals::{
    build_b2b_company_age_signal, build_b2b_listing_completeness_signal,
    build_b2b_transparency_signal, build_b2b_verification_signal,
};
use chrono::Datelike;

#[test]
fn verified_badge_produces_good_signal() {
    let supplier = make_supplier(true, None, None, None, None);
    let signal = build_b2b_verification_signal(&supplier);
    assert_eq!(signal.signal_type, "good");
    assert_eq!(signal.value, "Verified");
}

#[test]
fn unverified_badge_produces_caution_signal() {
    let supplier = make_supplier(false, None, None, None, None);
    let signal = build_b2b_verification_signal(&supplier);
    assert_eq!(signal.signal_type, "caution");
    assert_eq!(signal.value, "Unverified");
}

#[test]
fn no_year_established_returns_none() {
    let supplier = make_supplier(true, None, None, None, None);
    assert!(build_b2b_company_age_signal(&supplier).is_none());
}

#[test]
fn genuinely_new_company_gets_caution() {
    let current_year = chrono::Utc::now().year();
    let supplier = make_supplier(true, Some(&current_year.to_string()), None, None, None);
    let signal = build_b2b_company_age_signal(&supplier).unwrap();
    assert_eq!(signal.signal_type, "caution");
}

#[test]
fn established_company_gets_good() {
    let supplier = make_supplier(true, Some("2013"), None, None, None);
    let signal = build_b2b_company_age_signal(&supplier).unwrap();
    assert_eq!(signal.signal_type, "good");
}

#[test]
fn malformed_year_returns_none_not_a_crash() {
    let supplier = make_supplier(true, Some("not a year"), None, None, None);
    assert!(build_b2b_company_age_signal(&supplier).is_none());
}

#[test]
fn low_transparency_gives_info_not_caution() {
    let supplier = make_supplier(true, None, Some("0-10"), None, None);
    let signal = build_b2b_transparency_signal(&supplier);
    assert_eq!(signal.signal_type, "info");
    assert_eq!(signal.value, "1/3 fields provided");
}

#[test]
fn high_transparency_gives_good() {
    let supplier = make_supplier(true, None, Some("0-10"), Some("200K"), Some("10%"));
    let signal = build_b2b_transparency_signal(&supplier);
    assert_eq!(signal.signal_type, "good");
    assert_eq!(signal.value, "3/3 fields provided");
}

#[test]
fn zero_filled_listing_gives_caution() {
    let listing = make_listing(0);
    let signal = build_b2b_listing_completeness_signal(&listing);
    assert_eq!(signal.signal_type, "caution");
}

#[test]
fn at_least_one_filled_listing_gives_info_not_caution() {
    let listing = make_listing(1);
    let signal = build_b2b_listing_completeness_signal(&listing);
    assert_eq!(signal.signal_type, "info");
}
