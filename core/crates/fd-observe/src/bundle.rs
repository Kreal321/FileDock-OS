//! Only explicitly selected observations enter diagnostics; no secret-store or database files.
use serde_json::Value;
pub fn create(files: Vec<(String, Value)>) -> Result<Vec<u8>, std::io::Error> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, value) in files {
        if name.contains('/') || name.contains("..") {
            return Err(std::io::Error::other("diagnostics.path_invalid"));
        }
        let bytes = serde_json::to_vec_pretty(&fd_secrets::redact::redact_value(value))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes.as_slice())?;
    }
    zstd::stream::encode_all(tar.into_inner()?.as_slice(), 3)
}
