//! Automatic local takeover is allowed only for an unchanged, inactive ownership generation.
pub fn automatic_allowed(
    local: bool,
    recorded_generation: u64,
    current_generation: u64,
    last_write_at: &str,
) -> bool {
    local
        && recorded_generation == current_generation
        && chrono::DateTime::parse_from_rfc3339(last_write_at)
            .is_ok_and(|last| chrono::Utc::now().signed_duration_since(last).num_seconds() > 900)
}
