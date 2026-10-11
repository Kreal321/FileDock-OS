//! gRPC interceptors (FR-009 to FR-011, research R5).

use std::os::fd::AsRawFd;

use tonic::metadata::MetadataMap;
use tonic::service::Interceptor;
use tonic::{Request, Status};

pub const INTERFACE_VERSION: &str = "1.0";

const VERSION: &str = "fd-interface-version";
const ACTOR: &str = "fd-actor";
const REQUEST_ID: &str = "fd-request-id";

#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub uid: Option<u32>,
}

#[derive(Clone)]
pub struct InterfaceInterceptor {
    pub version: &'static str,
    pub mgmt_uid: Option<u32>,
}

impl Interceptor for InterfaceInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(peer) = request.extensions().get::<PeerInfo>()
            && !peer_allowed(peer.uid, self.mgmt_uid)
        {
            return Err(Status::permission_denied(
                "peer uid is not filedock-mgmt or root",
            ));
        }

        let meta = request.metadata();
        if version_of(meta).is_none() {
            return Err(Status::failed_precondition(
                "fd-interface-version is required",
            ));
        }
        Ok(request)
    }
}

pub fn peer_allowed(peer_uid: Option<u32>, mgmt_uid: Option<u32>) -> bool {
    match peer_uid {
        None => true,
        Some(0) => true,
        Some(uid) => mgmt_uid == Some(uid),
    }
}

pub fn peer_uid_of(fd: impl AsRawFd) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let mut cred = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut _ as *mut libc::c_void,
                &mut len,
            )
        };
        (rc == 0).then_some(cred.uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = fd;
        None
    }
}

fn version_of(meta: &MetadataMap) -> Option<String> {
    meta.get(VERSION)?.to_str().ok().map(str::to_owned)
}

fn parse_version(text: &str) -> (u32, u32) {
    let mut parts = text.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

pub fn check_metadata(
    meta: &MetadataMap,
    local_version: &str,
    mutating: bool,
) -> Result<(), Status> {
    let remote = version_of(meta)
        .ok_or_else(|| Status::failed_precondition("fd-interface-version is required"))?;
    let (local_major, _) = parse_version(local_version);
    let (remote_major, _) = parse_version(&remote);
    if remote_major != local_major && mutating {
        return Err(Status::failed_precondition("interface major mismatch"));
    }
    if mutating {
        if meta
            .get(ACTOR)
            .and_then(|v| v.to_str().ok())
            .is_none_or(str::is_empty)
        {
            return Err(Status::invalid_argument(
                "fd-actor is required on mutations",
            ));
        }
        if meta
            .get(REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .is_none_or(str::is_empty)
        {
            return Err(Status::invalid_argument(
                "fd-request-id is required on mutations",
            ));
        }
    }
    Ok(())
}

pub fn actor_of(meta: &MetadataMap) -> String {
    meta.get(ACTOR)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("system")
        .to_owned()
}

pub fn request_id_of(meta: &MetadataMap) -> String {
    meta.get(REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_mgmt_are_allowed() {
        assert!(peer_allowed(Some(0), Some(1000)));
        assert!(peer_allowed(Some(1000), Some(1000)));
        assert!(!peer_allowed(Some(7), Some(1000)));
    }

    #[test]
    fn major_mismatch_blocks_mutations() {
        let mut meta = MetadataMap::new();
        meta.insert(VERSION, "2.0".parse().unwrap());
        meta.insert(ACTOR, "admin:a".parse().unwrap());
        meta.insert(REQUEST_ID, "r1".parse().unwrap());
        assert!(check_metadata(&meta, "1.0", true).is_err());
        assert!(check_metadata(&meta, "1.0", false).is_ok());
    }
}
