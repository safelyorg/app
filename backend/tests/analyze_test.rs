use backend::handlers::analyze::{RATE_LIMITS, check_rate_limit};
use std::{collections::HashMap, sync::Mutex, time::Duration, time::Instant};
use uuid::Uuid;

#[test]
fn checking_rate_limit_one_time() {
    let user_id = Uuid::new_v4();

    let result = check_rate_limit(user_id);

    assert!(result.is_ok(), "expected the very first call to succeed");
}

#[test]
fn checking_rate_limit_five_times() {
    let user_id = Uuid::new_v4();

    for call_number in 0..=5 {
        let result = check_rate_limit(user_id);
        assert!(
            result.is_ok(),
            "expected call number {} to succeed, got: {:?}",
            call_number,
            result
        );
    }
}

#[test]
fn check_rate_limit_blocks_the_eleventh_call_within_the_window() {
    let user_id = Uuid::new_v4();

    for call_number in 1..=10 {
        let result = check_rate_limit(user_id);
        assert!(
            result.is_ok(),
            "expected call number {} to succeed, got: {:?}",
            call_number,
            result
        );
    }

    let eleventh_result = check_rate_limit(user_id);
    assert!(
        eleventh_result.is_err(),
        "expected the 11th call to be blocked, got: {:?}",
        eleventh_result
    );
}

#[test]
fn check_rate_limit_resets_after_the_window_expires() {
    let user_id = Uuid::new_v4();

    let map = RATE_LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let mut map = map.lock().unwrap();
        let time_passed = Instant::now() - Duration::from_secs(400);
        map.insert(user_id, (11, time_passed));
    }

    let result = check_rate_limit(user_id);

    assert!(
        result.is_ok(),
        "expected the expired window to reset, allowing this call through, got: {:?}",
        result
    );
}
