use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../proto");
    let protos = [
        proto_root.join("filedock/core/v1/core.proto"),
        proto_root.join("filedock/extension/v1/extension.proto"),
    ];
    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&protos, &[proto_root])?;
    Ok(())
}
