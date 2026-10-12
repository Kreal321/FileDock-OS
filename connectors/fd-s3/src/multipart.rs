//! Multipart uploads above 64 MiB use 16 MiB parts and abort on any error.

pub const PART_SIZE: usize = 16 * 1024 * 1024;
pub const MULTIPART_THRESHOLD: usize = 64 * 1024 * 1024;

pub fn should_multipart(size: u64) -> bool {
    size as usize > MULTIPART_THRESHOLD
}
