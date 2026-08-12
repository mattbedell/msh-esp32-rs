use std::{path::PathBuf, process::ExitCode};

use anyhow::{Result, anyhow};

mod meshtastic_protos;

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    let workspace_dir = std::env::var("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .expect("'CARGO_WORKSPACE_DIR' must be set");

    let meshtastic_protos_dir = std::env::var("MESHTASTIC_PROTOS_DIR")
        .map(|s| workspace_dir.join(s))
        .expect("'MESHTASTIC_PROTOS_DIR' must be set");

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
                .or_else(|| std::env::var("MESHTASTIC_PROTOS_VERSION").ok())
                .ok_or_else(|| anyhow!("Meshtastic protobuf version must be provided"))?;

            meshtastic_protos::checkout_protos(&workspace_dir, &meshtastic_protos_dir, &proto_tag)?;
            let nanopb_settings =
                meshtastic_protos::parse_static_settings(&meshtastic_protos_dir)?;
            meshtastic_protos::write_micropb_config(
                &workspace_dir.join("micropb.toml"),
                nanopb_settings,
            )?;
            Ok(())
        }
        _ => Err(anyhow!("unrecognized xtask")),
    }
}
