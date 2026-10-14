//! Bounded, verified .fdext archives. No untrusted tar extraction is performed.
use crate::{
    AdapterError,
    manifest::{LoadedManifest, parse_manifest},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Component, Path},
};
pub const MAX_PACKAGE: usize = 64 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, AdapterError>;
pub fn invalid(code: &str) -> AdapterError {
    AdapterError::Failed(code.into())
}
pub fn relative(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('\\')
        || name.contains(['\n', '\r', '\0'])
        || Path::new(name)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(invalid("extension.path_invalid"));
    }
    Ok(())
}
#[derive(Clone)]
pub struct Package {
    pub manifest: LoadedManifest,
    pub hash: String,
    pub signed: bool,
    pub signer_key_id: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub executable: BTreeSet<String>,
}
impl Package {
    pub fn read(bytes: &[u8], trusted: &Path, release: &str) -> Result<Self> {
        Self::read_inner(bytes, trusted, release, None)
    }
    /// An authenticated recovery snapshot already pins the exact approved artifact hash.
    pub fn read_recorded(
        bytes: &[u8],
        trusted: &Path,
        release: &str,
        hash: &str,
        signer: &str,
    ) -> Result<Self> {
        if hex::encode(Sha256::digest(bytes)) != hash {
            return Err(invalid("extension.recorded_package_required"));
        }
        Self::read_inner(bytes, trusted, release, Some(signer))
    }
    fn read_inner(
        bytes: &[u8],
        trusted: &Path,
        release: &str,
        recorded_signer: Option<&str>,
    ) -> Result<Self> {
        if bytes.len() > MAX_PACKAGE {
            return Err(invalid("extension.package_too_large"));
        }
        let decoder = zstd::stream::read::Decoder::new(Cursor::new(bytes))
            .map_err(|_| invalid("extension.archive_invalid"))?;
        let mut archive = tar::Archive::new(decoder.take((MAX_PACKAGE * 4 + 1) as u64));
        let mut files = BTreeMap::new();
        let mut executable = BTreeSet::new();
        let mut total = 0;
        for item in archive
            .entries()
            .map_err(|_| invalid("extension.archive_invalid"))?
        {
            let item = item.map_err(|_| invalid("extension.archive_invalid"))?;
            let path = item
                .path()
                .map_err(|_| invalid("extension.path_invalid"))?
                .to_string_lossy()
                .into_owned();
            relative(&path)?;
            if !item.header().entry_type().is_file()
                || files.contains_key(&path)
                || files.len() >= 4096
            {
                return Err(invalid("extension.archive_invalid"));
            }
            if item.header().mode().unwrap_or(0) & 0o111 != 0 {
                executable.insert(path.clone());
            }
            let mut data = Vec::new();
            item.take((MAX_PACKAGE + 1) as u64)
                .read_to_end(&mut data)
                .map_err(|_| invalid("extension.archive_invalid"))?;
            total += data.len();
            if data.len() > MAX_PACKAGE || total > MAX_PACKAGE * 4 {
                return Err(invalid("extension.package_too_large"));
            }
            files.insert(path, data);
        }
        let sums = files
            .get("SHA256SUMS")
            .ok_or_else(|| invalid("extension.checksums_missing"))?;
        let mut checked = BTreeSet::new();
        for line in std::str::from_utf8(sums)
            .map_err(|_| invalid("extension.checksums_invalid"))?
            .lines()
        {
            let (hash, name) = line
                .split_once("  ")
                .ok_or_else(|| invalid("extension.checksums_invalid"))?;
            relative(name)?;
            if name == "SHA256SUMS"
                || name == "SHA256SUMS.minisig"
                || !checked.insert(name.to_owned())
            {
                return Err(invalid("extension.checksums_invalid"));
            }
            let data = files
                .get(name)
                .ok_or_else(|| invalid("extension.integrity_failed"))?;
            if hex::encode(Sha256::digest(data)) != hash {
                return Err(invalid("extension.integrity_failed"));
            }
        }
        if files
            .keys()
            .any(|n| n != "SHA256SUMS" && n != "SHA256SUMS.minisig" && !checked.contains(n))
        {
            return Err(invalid("extension.unchecked_payload"));
        }
        if files
            .get("manifest.yaml")
            .is_some_and(|f| f.len() > 1024 * 1024)
        {
            return Err(invalid("extension.manifest_too_large"));
        }
        let manifest = parse_manifest(
            std::str::from_utf8(
                files
                    .get("manifest.yaml")
                    .ok_or_else(|| invalid("extension.manifest_missing"))?,
            )
            .map_err(|_| invalid("extension.manifest_invalid"))?,
        )
        .map_err(|_| invalid("extension.manifest_invalid"))?;
        for field in ["intervalSeconds", "timeoutSeconds", "startupGraceSeconds"] {
            if manifest.raw["health"][field]
                .as_u64()
                .is_some_and(|seconds| seconds > 3600)
            {
                return Err(invalid("extension.health_timeout_invalid"));
            }
        }
        if manifest.class != "extension" {
            return Err(invalid("extension.class_invalid"));
        }
        let range = manifest.raw["compatibility"]["appliance"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(", ");
        let range = semver::VersionReq::parse(&range)
            .map_err(|_| invalid("extension.compatibility_invalid"))?;
        let release = semver::Version::parse(release)
            .map_err(|_| invalid("extension.compatibility_invalid"))?;
        if !range.matches(&release)
            || manifest.raw["compatibility"]["extensionProtocol"]
                .as_str()
                .is_some_and(|p| p != "1")
        {
            return Err(invalid("extension.incompatible"));
        }
        for field in std::iter::once(manifest.raw.get("configSchema")).chain(
            manifest
                .families()
                .iter()
                .map(|v| v.get("connectionSchema")),
        ) {
            if let Some(path) = field.and_then(|v| v.as_str()) {
                relative(path)?;
                let schema: serde_json::Value = serde_json::from_slice(
                    files
                        .get(path)
                        .ok_or_else(|| invalid("extension.schema_missing"))?,
                )
                .map_err(|_| invalid("extension.schema_invalid"))?;
                jsonschema::validator_for(&schema)
                    .map_err(|_| invalid("extension.schema_invalid"))?;
            }
        }
        let mut signed = false;
        let mut signer_key_id = String::new();
        if let Some(raw) = files.get("SHA256SUMS.minisig") {
            let signature = minisign_verify::Signature::decode(
                std::str::from_utf8(raw).map_err(|_| invalid("extension.signature_invalid"))?,
            )
            .map_err(|_| invalid("extension.signature_invalid"))?;
            if let Ok(entries) = std::fs::read_dir(trusted) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|s| s.to_str()) != Some("pub") {
                        continue;
                    }
                    if let Ok(key) = minisign_verify::PublicKey::from_file(entry.path())
                        && key.verify(sums, &signature, false).is_ok()
                    {
                        signed = true;
                        signer_key_id = entry
                            .file_name()
                            .to_string_lossy()
                            .trim_end_matches(".pub")
                            .to_owned();
                        break;
                    }
                }
            }
            if !signed {
                if let Some(signer) = recorded_signer {
                    signed = true;
                    signer_key_id = signer.into();
                } else {
                    return Err(invalid("extension.signature_untrusted_or_invalid"));
                }
            }
        }
        Ok(Self {
            manifest,
            hash: hex::encode(Sha256::digest(bytes)),
            signed,
            signer_key_id,
            files,
            executable,
        })
    }
    pub fn extract_verified(&self, root: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root).map_err(|_| invalid("extension.install_io"))?;
        for (name, bytes) in &self.files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap())
                .map_err(|_| invalid("extension.install_io"))?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| invalid("extension.install_io"))?;
            use std::io::Write;
            file.write_all(bytes)
                .map_err(|_| invalid("extension.install_io"))?;
            file.sync_all()
                .map_err(|_| invalid("extension.install_io"))?;
            std::fs::set_permissions(
                path,
                std::fs::Permissions::from_mode(if self.executable.contains(name) {
                    0o555
                } else {
                    0o444
                }),
            )
            .map_err(|_| invalid("extension.install_io"))?;
        }
        let mut parents = BTreeSet::new();
        for name in self.files.keys() {
            if let Some(parent) = root.join(name).parent() {
                parents.insert(parent.to_path_buf());
            }
        }
        parents.insert(root.to_path_buf());
        for parent in parents {
            std::fs::File::open(parent)
                .and_then(|f| f.sync_all())
                .map_err(|_| invalid("extension.install_io"))?;
        }
        Ok(())
    }
    pub fn permissions(&self) -> Vec<String> {
        self.manifest.raw["permissions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    }
}
