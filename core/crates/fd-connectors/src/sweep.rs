//! Sweep a location only while holding its checked Writer session.
use crate::ownership::{OwnershipError, Result, Role, WriterSession};
pub const MIN_AGE_HOURS: u32 = 24;
pub async fn owned(session: &WriterSession) -> Result<u32> {
    if session.location.role != Role::Writer {
        return Err(OwnershipError::Refused("location.recovery_reader".into()));
    }
    session.check().await?;
    session.io.abort_incomplete("", MIN_AGE_HOURS, true).await
}
pub fn probe_prefix(appliance_id: &str) -> Option<String> {
    uuid::Uuid::parse_str(appliance_id)
        .ok()
        .map(|_| format!("_filedock/probe/{appliance_id}/"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_scope_cannot_be_an_entire_bucket() {
        assert!(probe_prefix("").is_none());
        assert!(probe_prefix("../other").is_none());
        assert_eq!(
            probe_prefix("00000000-0000-7000-8000-000000000001").unwrap(),
            "_filedock/probe/00000000-0000-7000-8000-000000000001/"
        );
    }
}
