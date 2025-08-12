use glob::glob;
use std::path::PathBuf;

// To download the XMTP proto files, run:
// `buf export buf.build/xmtp/proto --output proto/ --path .`
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set PROTOC to use the vendored protoc binary
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    // Re-run if *any* file in proto/ changes
    println!("cargo:rerun-if-changed=proto");

    // Collect all vendored .proto files
    let protos: Vec<PathBuf> = glob("proto/**/*.proto")?.filter_map(Result::ok).collect();

    std::fs::create_dir_all("src/generated")?;

    // Generate client + server stubs into src/generated (git-ignored)
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .out_dir("src/generated")
        .compile_protos(&protos, &["proto"])?;

    Ok(())
}
