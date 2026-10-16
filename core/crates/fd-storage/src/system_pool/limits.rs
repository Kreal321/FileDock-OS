use crate::{Error, Result};
#[derive(Debug)]
pub struct Capacity {
    pub bytes: u64,
    pub reserve_limited: bool,
}
pub fn capacity(
    total: u64,
    non_pool_used: u64,
    pool_used: u64,
    percent: u32,
    reserve: u64,
) -> Result<Capacity> {
    if !(10..=90).contains(&percent) {
        return Err(Error("system_pool.max_usage_out_of_range".into()));
    }
    if reserve < 20_000_000_000 {
        return Err(Error("system_pool.reserve_too_small".into()));
    }
    let cap = total / 100 * u64::from(percent);
    let remaining = total.saturating_sub(non_pool_used).saturating_sub(reserve);
    let bytes = cap.min(remaining) / 4096 * 4096;
    if bytes < pool_used {
        return Err(Error("system_pool.below_usage".into()));
    }
    Ok(Capacity {
        bytes,
        reserve_limited: remaining < cap,
    })
}
pub fn guard(total: u64) -> u64 {
    5_000_000_000u64.min(total / 50)
}
