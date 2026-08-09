use anyhow::{Result, anyhow};
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const MESHTASTIC_PROTOS_DIR: &str = "meshtastic_protos";

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    // let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let task = std::env::args().nth(1).unwrap_or_default();
    let workspace_dir =
        std::env::var("CARGO_WORKSPACE_DIR").expect("'CARGO_WORKSPACE_DIR' must be set");

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

fn prepare_protos(workspace_dir: &str, tag: &str) -> Result<()> {
    let mut meshtastic_protos_dir = PathBuf::from(&workspace_dir);
    meshtastic_protos_dir.push(MESHTASTIC_PROTOS_DIR);
    checkout_protos(workspace_dir, &meshtastic_protos_dir, tag)?;
    parse_static_settings(workspace_dir, &meshtastic_protos_dir)?;
    Ok(())
}

fn checkout_protos(workspace_dir: &str, meshtastic_protos_dir: &Path, tag: &str) -> Result<()> {
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

fn parse_static_settings(workspace_dir: &str, mashtastic_dir: &Path) -> Result<()> {
    Ok(())
}
