use std::path::PathBuf;

use micropb_gen::Generator;

fn main() {
    let workspace_dir = std::env::var("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .expect("'CARGO_WORKSPACE_DIR' must be set");
    let meshtastic_repo_dir = std::env::var("MESHTASTIC_PROTOS_DIR")
        .map(|s| workspace_dir.join(s))
        .expect("'MESHTASTIC_PROTOS_DIR' must be set");

    let meshtastic_protos_dir = meshtastic_repo_dir.join("meshtastic");

    let out_file = std::env::var("OUT_DIR")
        .map(|s| {
            let mut p = PathBuf::from(s);
            p.push("proto.rs");
            p
        })
        .unwrap();

    let micropb_cfg = workspace_dir.join("micropb.toml");

    let mut generator = Generator::new();
    generator.use_container_heapless();
    generator
        .parse_config_file(&micropb_cfg, ".meshtastic")
        .unwrap();
    generator.add_protoc_arg(format!("--proto_path={}", meshtastic_repo_dir.display()));

    let protos: Vec<_> = std::fs::read_dir(meshtastic_protos_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|d| d.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "proto"))
        .collect();

    generator.compile_protos(&protos, out_file).unwrap();
}
