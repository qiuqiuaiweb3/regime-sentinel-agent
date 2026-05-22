#[test]
fn gemini_throttle_defaults_to_disabled_30_minutes_and_two_calls_per_hour() {
    let config = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        None, None, None, None,
    )
    .expect("default config");

    assert!(!config.enabled);
    assert_eq!(config.summary_interval_minutes, 30);
    assert_eq!(config.max_calls_per_hour, 2);
    assert_eq!(config.manual_cooldown_seconds, 300);
}

#[test]
fn gemini_throttle_parses_enabled_15_minute_config() {
    let config = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        Some("true"),
        Some("15"),
        Some("4"),
        Some("900"),
    )
    .expect("explicit config");

    assert!(config.enabled);
    assert_eq!(config.summary_interval_minutes, 15);
    assert_eq!(config.max_calls_per_hour, 4);
    assert_eq!(config.manual_cooldown_seconds, 900);
}

#[test]
fn gemini_throttle_rejects_intervals_below_15_minutes() {
    let error = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        Some("true"),
        Some("5"),
        Some("4"),
        Some("300"),
    )
    .expect_err("interval below floor");

    assert!(error.contains("at least 15"));
}

#[test]
fn gemini_throttle_requires_enabled_interval_and_hourly_budget() {
    let config = regime_service::gemini_throttle::GeminiThrottleConfig {
        enabled: true,
        summary_interval_minutes: 15,
        max_calls_per_hour: 2,
        manual_cooldown_seconds: 300,
    };

    assert!(config.should_start_summary(900_000, None, 0));
    assert!(!config.should_start_summary(1_000_000, Some(200_000), 0));
    assert!(config.should_start_summary(1_100_000, Some(200_000), 1));
    assert!(!config.should_start_summary(1_100_000, Some(200_000), 2));
}

#[test]
fn gemini_throttle_applies_manual_explain_cooldown_and_hourly_cap() {
    let config = regime_service::gemini_throttle::GeminiThrottleConfig {
        enabled: true,
        summary_interval_minutes: 30,
        max_calls_per_hour: 2,
        manual_cooldown_seconds: 300,
    };

    assert!(config.should_start_manual_explain(1_000_000, None, 0));
    assert!(!config.should_start_manual_explain(1_100_000, Some(1_000_000), 0));
    assert!(config.should_start_manual_explain(1_300_000, Some(1_000_000), 1));
    assert!(!config.should_start_manual_explain(1_300_000, Some(1_000_000), 2));
    assert_eq!(
        config.manual_retry_after_seconds(1_100_000, Some(1_000_000)),
        Some(200)
    );
}

#[test]
fn gemini_call_budget_is_shared_by_auto_and_manual_calls() {
    let budget = regime_service::gemini_throttle::GeminiCallBudget::new();
    let config = regime_service::gemini_throttle::GeminiThrottleConfig {
        enabled: true,
        summary_interval_minutes: 30,
        max_calls_per_hour: 2,
        manual_cooldown_seconds: 300,
    };

    assert!(budget.reserve_summary_call(&config, 1_000_000, None));
    assert!(budget.reserve_manual_explain_call(&config, 1_300_000, None));
    assert!(!budget.reserve_manual_explain_call(&config, 1_600_000, Some(1_300_000)));
}

#[test]
fn gemini_throttle_rejects_zero_manual_cooldown() {
    let error = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        Some("true"),
        Some("30"),
        Some("4"),
        Some("0"),
    )
    .expect_err("zero manual cooldown");

    assert!(error.contains("GEMINI_MANUAL_COOLDOWN_SECONDS"));
}
