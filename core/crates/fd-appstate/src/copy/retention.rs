use super::index::SnapshotEntry;
use std::collections::BTreeSet;
pub fn retained(entries: &[SnapshotEntry]) -> BTreeSet<String> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| b.taken_at.cmp(&a.taken_at));
    let mut kept = BTreeSet::new();
    let mut hours = BTreeSet::new();
    let mut days = BTreeSet::new();
    for (i, e) in sorted.iter().enumerate() {
        let hour = e.taken_at.get(..13).unwrap_or(&e.taken_at).to_owned();
        let day = e.taken_at.get(..10).unwrap_or(&e.taken_at).to_owned();
        let hourly = !hours.contains(&hour) && hours.len() < 24;
        let daily = !days.contains(&day) && days.len() < 30;
        if hourly {
            hours.insert(hour);
        }
        if daily {
            days.insert(day);
        }
        if i < 30 || hourly || daily {
            kept.insert(e.snapshot_id.clone());
        }
    }
    kept
}
