//! RustCrypto STREAM-BE32: authenticated ordering and final-frame detection.
use crate::{Result, invalid};
use aead_stream::{DecryptorBE32, EncryptorBE32};
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
use zeroize::Zeroizing;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
const CHUNK: usize = 65536;
const PREFIX: usize = 16 + 24 + 48 + 19;
pub struct SnapshotPayload {
    pub manifest: Value,
    pub config: Value,
    pub secrets: Vec<u8>,
    pub dek: [u8; 32],
}
fn random<const N: usize>() -> [u8; N] {
    let mut b = [0; N];
    getrandom::fill(&mut b).expect("entropy");
    b
}
pub fn wrap(bytes: &[u8], key: &[u8; 32], aad: &[u8]) -> Result<Vec<u8>> {
    let nonce = random::<24>();
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let mut output = nonce.to_vec();
    output.extend(
        cipher
            .encrypt(&XNonce::from(nonce), Payload { msg: bytes, aad })
            .map_err(|_| invalid("state.crypto_invalid"))?,
    );
    Ok(output)
}
pub fn unwrap(bytes: &[u8], key: &[u8; 32], aad: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 40 {
        return Err(invalid("state.crypto_truncated"));
    }
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    cipher
        .decrypt(
            &XNonce::try_from(&bytes[..24]).map_err(|_| invalid("state.crypto_invalid"))?,
            Payload {
                msg: &bytes[24..],
                aad,
            },
        )
        .map_err(|_| invalid("recovery.key_invalid"))
}
pub fn encrypt_bytes(plain: &[u8], master: &[u8; 32], magic: &[u8; 8]) -> Result<Vec<u8>> {
    if plain.len() > MAX_BYTES {
        return Err(invalid("state.too_large"));
    }
    let key = Zeroizing::new(random::<32>());
    let nonce = random::<19>();
    let mut out = magic.to_vec();
    out.extend(1u16.to_le_bytes());
    out.extend([0; 6]);
    let wrapped = wrap(key.as_ref(), master, &out)?;
    out.extend(wrapped);
    out.extend(nonce);
    let aad = out.clone();
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let mut encryptor = EncryptorBE32::from_aead(cipher, &nonce.into());
    let mut chunks = plain.chunks(CHUNK).peekable();
    while let Some(chunk) = chunks.next() {
        if chunks.peek().is_none() {
            let bytes = encryptor
                .encrypt_last(Payload {
                    msg: chunk,
                    aad: &aad,
                })
                .map_err(|_| invalid("state.crypto_invalid"))?;
            out.push(1);
            out.extend((bytes.len() as u32).to_le_bytes());
            out.extend(bytes);
            return Ok(out);
        }
        let bytes = encryptor
            .encrypt_next(Payload {
                msg: chunk,
                aad: &aad,
            })
            .map_err(|_| invalid("state.crypto_invalid"))?;
        out.push(0);
        out.extend((bytes.len() as u32).to_le_bytes());
        out.extend(bytes);
    }
    let bytes = encryptor
        .encrypt_last(Payload {
            msg: &[],
            aad: &aad,
        })
        .map_err(|_| invalid("state.crypto_invalid"))?;
    out.push(1);
    out.extend((bytes.len() as u32).to_le_bytes());
    out.extend(bytes);
    Ok(out)
}
pub fn decrypt_bytes(bytes: &[u8], master: &[u8; 32], magic: &[u8; 8]) -> Result<Vec<u8>> {
    if bytes.len() < PREFIX + 21
        || bytes.len() > MAX_BYTES + 128 * 1024
        || &bytes[..8] != magic
        || bytes[8..10] != 1u16.to_le_bytes()
        || bytes[10..16] != [0; 6]
    {
        return Err(invalid("backup_file.invalid"));
    }
    let unwrapped = Zeroizing::new(unwrap(&bytes[16..88], master, &bytes[..16])?);
    let key: [u8; 32] = unwrapped
        .as_slice()
        .try_into()
        .map_err(|_| invalid("state.key_invalid"))?;
    let nonce: [u8; 19] = bytes[88..PREFIX].try_into().unwrap();
    let mut decryptor =
        DecryptorBE32::from_aead(XChaCha20Poly1305::new(&Key::from(key)), &nonce.into());
    let mut out = Vec::new();
    let mut offset = PREFIX;
    while offset + 5 <= bytes.len() {
        let last = bytes[offset];
        let size = u32::from_le_bytes(bytes[offset + 1..offset + 5].try_into().unwrap()) as usize;
        offset += 5;
        if !(16..=CHUNK + 16).contains(&size) || offset + size > bytes.len() {
            return Err(invalid("state.frame_invalid"));
        }
        let payload = Payload {
            msg: &bytes[offset..offset + size],
            aad: &bytes[..PREFIX],
        };
        offset += size;
        if last == 1 {
            if offset != bytes.len() {
                return Err(invalid("state.trailing_data"));
            }
            out.extend(
                decryptor
                    .decrypt_last(payload)
                    .map_err(|_| invalid("state.integrity_failed"))?,
            );
            return Ok(out);
        }
        if last != 0 {
            return Err(invalid("state.frame_invalid"));
        }
        out.extend(
            decryptor
                .decrypt_next(payload)
                .map_err(|_| invalid("state.integrity_failed"))?,
        );
        if out.len() > MAX_BYTES {
            return Err(invalid("state.too_large"));
        }
    }
    Err(invalid("state.final_frame_missing"))
}
fn append(builder: &mut tar::Builder<Vec<u8>>, name: &str, data: &[u8]) -> Result<()> {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o600);
    h.set_cksum();
    builder.append_data(&mut h, name, data)?;
    Ok(())
}
pub fn encode(payload: &SnapshotPayload, master: &[u8; 32]) -> Result<Vec<u8>> {
    let mut tar = tar::Builder::new(Vec::new());
    append(
        &mut tar,
        "manifest.json",
        &serde_json::to_vec(&payload.manifest)?,
    )?;
    append(
        &mut tar,
        "config.json",
        &serde_json::to_vec(&payload.config)?,
    )?;
    append(&mut tar, "secrets/secrets.enc", &payload.secrets)?;
    append(
        &mut tar,
        "secrets/dek.wrapped",
        &wrap(&payload.dek, master, b"FileDock-DEK-v1")?,
    )?;
    let raw = tar.into_inner()?;
    if raw.len() > MAX_BYTES {
        return Err(invalid("state.too_large"));
    }
    let compressed = zstd::stream::encode_all(Cursor::new(raw), 3)?;
    encrypt_bytes(&compressed, master, b"FDSTATE\0")
}
pub fn decode(bytes: &[u8], master: &[u8; 32]) -> Result<SnapshotPayload> {
    let plain = Zeroizing::new(decrypt_bytes(bytes, master, b"FDSTATE\0")?);
    let mut decompressed = Zeroizing::new(Vec::new());
    zstd::stream::read::Decoder::new(Cursor::new(plain.as_slice()))?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut decompressed)?;
    if decompressed.len() > MAX_BYTES {
        return Err(invalid("state.too_large"));
    }
    let mut files = BTreeMap::new();
    let mut archive = tar::Archive::new(Cursor::new(decompressed.as_slice()));
    for entry in archive.entries()? {
        let e = entry?;
        if !e.header().entry_type().is_file() {
            return Err(invalid("state.archive_invalid"));
        }
        let name = e.path()?.to_string_lossy().into_owned();
        if ![
            "manifest.json",
            "config.json",
            "secrets/secrets.enc",
            "secrets/dek.wrapped",
        ]
        .contains(&name.as_str())
            || files.contains_key(&name)
        {
            return Err(invalid("state.archive_invalid"));
        }
        let mut data = Vec::new();
        e.take((MAX_BYTES + 1) as u64).read_to_end(&mut data)?;
        if data.len() > MAX_BYTES {
            return Err(invalid("state.too_large"));
        }
        files.insert(name, data);
    }
    if files.len() != 4 {
        return Err(invalid("state.archive_missing"));
    }
    let dek = Zeroizing::new(unwrap(
        &files["secrets/dek.wrapped"],
        master,
        b"FileDock-DEK-v1",
    )?);
    Ok(SnapshotPayload {
        manifest: serde_json::from_slice(&files["manifest.json"])?,
        config: serde_json::from_slice(&files["config.json"])?,
        secrets: files.remove("secrets/secrets.enc").unwrap(),
        dek: dek
            .as_slice()
            .try_into()
            .map_err(|_| invalid("state.dek_invalid"))?,
    })
}

impl Drop for SnapshotPayload {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.dek.zeroize();
    }
}
