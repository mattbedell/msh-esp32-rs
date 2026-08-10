use anyhow::{Result, anyhow};

use prost::Message;
use prost_types::FileDescriptorSet;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const MESHTASTIC_PROTOS_DIR: &str = "meshtastic_protobufs";

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    let workspace_dir =
        std::env::var("CARGO_WORKSPACE_DIR").map(PathBuf::from).expect("'CARGO_WORKSPACE_DIR' must be set");

    match task.as_str() {
        "proto:install" => {
            let proto_tag = std::env::args()
                .nth(2)
                .map(|v| {
                    let ver = v.trim();
                    if ver.starts_with("v") {
                        ver.to_owned()
                    } else {
                        format!("v{ver}")
                    }
                })
                .or_else(|| std::env::var("MESHTASTIC_PROTO_VERSION").ok())
                .ok_or_else(|| anyhow!("Meshtastic protobuf version must be provided"))?;

            prepare_protos(&workspace_dir, &proto_tag)?;
            Ok(())
        }
        _ => Err(anyhow!("unrecognized xtask")),
    }
}

fn prepare_protos(workspace_dir: &Path, tag: &str) -> Result<()> {
    let mut meshtastic_protos_dir = PathBuf::from(&workspace_dir);
    meshtastic_protos_dir.push(MESHTASTIC_PROTOS_DIR);
    checkout_protos(workspace_dir, &meshtastic_protos_dir, tag)?;
    parse_static_settings(workspace_dir, &meshtastic_protos_dir)?;
    Ok(())
}

fn checkout_protos(workspace_dir: &Path, meshtastic_protos_dir: &Path, tag: &str) -> Result<()> {
    let _ = Command::new("git")
        .current_dir(workspace_dir)
        .args(["submodule", "update", "--init"])
        .status()?;

    let _ = Command::new("git")
        .current_dir(meshtastic_protos_dir)
        .args(["fetch", "--tags"])
        .status()?;

    let _ = Command::new("git")
        .current_dir(meshtastic_protos_dir)
        .args(["checkout", tag])
        .status()?;

    println!(">>> Checked out Meshtastic Protobufs: '{tag}'");
    Ok(())
}

fn parse_static_settings(workspace_dir: &Path, meshtastic_dir: &Path) -> Result<()> {
    let mut proto_dir = meshtastic_dir.to_owned();
    proto_dir.push("meshtastic");
    let desc = tempfile::NamedTempFile::new()?;

    for entry in fs::read_dir(&proto_dir)? {
        let entry = entry?;
        let path = entry.path();
        // todo: tmp, work on only mesh.proto as I build out the parser
        if entry.file_name() != "mesh.options" {
            continue;
        }
        let Some("options") = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let proto_path = proto_dir.join(format!("{stem}.proto"));

        let _ = Command::new("protoc").args([
            format!("--proto_path={}", proto_dir.display()),
            format!("--proto_path={}", meshtastic_dir.display()),
            format!("--descriptor_set_out={}", desc.path().display()),
            proto_path.to_str().unwrap().to_string(),
        ]).status()?;

        let out = fs::read(desc.path())?;
        let fd_set = FileDescriptorSet::decode(&out[..])?;

        for file in fd_set.file {
            for msg in file.message_type {
                println!("{}", msg.name());
                for field in msg.field {
                    let m = field.name();
                    println!("    {m}");
                }
            }
        }
    }
    Ok(())
}
