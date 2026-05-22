#[test]
fn gemini_throttle_defaults_to_disabled_30_minutes_and_two_calls_per_hour() {
    let config =
        regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(None, None, None)
            .expect("default config");

    assert!(!config.enabled);
    assert_eq!(config.summary_interval_minutes, 30);
    assert_eq!(config.max_calls_per_hour, 2);
}

#[test]
fn gemini_throttle_parses_enabled_15_minute_config() {
    let config = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        Some("true"),
        Some("15"),
        Some("4"),
    )
    .expect("explicit config");

    assert!(config.enabled);
    assert_eq!(config.summary_interval_minutes, 15);
    assert_eq!(config.max_calls_per_hour, 4);
}

#[test]
fn gemini_throttle_rejects_intervals_below_15_minutes() {
    let error = regime_service::gemini_throttle::GeminiThrottleConfig::from_env_values(
        Some("true"),
        Some("5"),
        Some("4"),
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
    };

    assert!(config.should_start_summary(900_000, None, 0));
    assert!(!config.should_start_summary(1_000_000, Some(200_000), 0));
    assert!(config.should_start_summary(1_100_000, Some(200_000), 1));
    assert!(!config.should_start_summary(1_100_000, Some(200_000), 2));
}
