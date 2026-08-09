use anyhow::{Error, Result, anyhow};
use std::{
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
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

            checkout_protos(&workspace_dir, &proto_tag)?;
        }
        _ => {
            anyhow!("unrecognized xtask");
        }
    }
    Ok(())
}

fn checkout_protos(workspace: &str, tag: &str) -> Result<()> {
    let cmd = Command::new("git").current_dir(workspace);
    Ok(())
}
