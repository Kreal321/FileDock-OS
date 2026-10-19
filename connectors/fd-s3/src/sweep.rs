//! List and abort only old multipart uploads within the Core-authorized prefix.
use crate::{config::RoutedConnection, sigv4};
use serde::Deserialize;
use tonic::Status;
#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Listing {
    #[serde(default)]
    is_truncated: bool,
    #[serde(default)]
    next_key_marker: String,
    #[serde(default)]
    next_upload_id_marker: String,
    #[serde(default, rename = "Upload")]
    uploads: Vec<Upload>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Upload {
    key: String,
    upload_id: String,
    initiated: chrono::DateTime<chrono::Utc>,
}
fn eligible(prefix: &str, upload: &Upload, cutoff: chrono::DateTime<chrono::Utc>) -> bool {
    upload.key.starts_with(prefix) && upload.initiated < cutoff
}
pub async fn abort(
    conn: &RoutedConnection,
    prefix: &str,
    hours: u32,
    root_owner_checked: bool,
) -> Result<u32, Status> {
    if (prefix.is_empty() && !root_owner_checked)
        || prefix.starts_with('/')
        || prefix.split('/').any(|p| p == ".." || p == ".")
        || hours < 24
    {
        return Err(Status::invalid_argument("sweep.scope_invalid"));
    }
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours.into());
    let mut count = 0;
    let mut marker = String::new();
    let mut upload_marker = String::new();
    loop {
        let mut query = vec![("uploads", ""), ("prefix", prefix)];
        if !marker.is_empty() {
            query.push(("key-marker", &marker));
            query.push(("upload-id-marker", &upload_marker));
        }
        let bytes = sigv4::request(conn, "GET", None, &query).await?;
        let page: Listing = quick_xml::de::from_reader(bytes.as_slice())
            .map_err(|_| Status::data_loss("sweep.response_invalid"))?;
        for upload in page.uploads.iter().filter(|u| eligible(prefix, u, cutoff)) {
            sigv4::request(
                conn,
                "DELETE",
                Some(&upload.key),
                &[("uploadId", &upload.upload_id)],
            )
            .await?;
            count += 1;
        }
        if !page.is_truncated {
            break;
        }
        if page.next_key_marker == marker && page.next_upload_id_marker == upload_marker {
            return Err(Status::data_loss("sweep.pagination_invalid"));
        }
        marker = page.next_key_marker;
        upload_marker = page.next_upload_id_marker;
    }
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_aborts_young_or_out_of_scope_uploads() {
        let now = chrono::Utc::now();
        let mut upload = Upload {
            key: "owned/a".into(),
            upload_id: "id".into(),
            initiated: now - chrono::Duration::hours(25),
        };
        assert!(eligible(
            "owned/",
            &upload,
            now - chrono::Duration::hours(24)
        ));
        assert!(!eligible("other/", &upload, now));

        upload.initiated = now;
        assert!(!eligible(
            "owned/",
            &upload,
            now - chrono::Duration::hours(24)
        ));
    }
}
