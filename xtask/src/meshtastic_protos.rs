use anyhow::Result;
use glob_match as glob;
use prost::Message;
use prost_types::{DescriptorProto, FileDescriptorSet};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufWriter, Write},
    path::Path,
    process::Command,
};

type PathedSettings = BTreeMap<String, BTreeMap<String, String>>;

struct SettingsMatcher {
    pattern: String,
    settings: Vec<Setting>,
}

impl SettingsMatcher {
    fn from_nanopb(nanopb: &str) -> Option<Self> {
        let mut raw = nanopb.split_whitespace();

        let pattern = raw.next()?;

        let settings: Vec<_> = raw.filter_map(Setting::from_nanopb).collect();

        if settings.is_empty() {
            None
        } else {
            Some(Self {
                pattern: pattern.into(),
                settings,
            })
        }
    }
}

struct Setting {
    name: &'static str,
    value: String,
}

impl Setting {
    fn from_nanopb(nanopb: &str) -> Option<Self> {
        let mut raw = nanopb.split(':');

        let key = raw.next()?;
        let value = raw.next()?;

        match key {
            "max_size" => Some(Self {
                name: "max_bytes",
                value: value.to_string(),
            }),
            "max_length" => {
                let l: usize = value.parse().unwrap_or(0);
                Some(Self {
                    name: "max_size",
                    value: (l + 1).to_string(),
                })
            }
            "max_count" => Some(Self {
                name: "max_len",
                value: value.to_string(),
            }),
            "int_size" => Some(Self {
                name: "int_size",
                value: (match value {
                    "8" => "\"S8\"",
                    "16" => "\"S16\"",
                    "32" => "\"S32\"",
                    _ => panic!("{value} int size not supported"),
                })
                .to_string(),
            }),
            "type" => match value {
                "FT_IGNORE" => Some(Self {
                    name: "skip",
                    value: "true".to_string(),
                }),
                _ => {
                    eprintln!("WARN: no micropb mapping for '{key}:{value}'");
                    None
                }
            },
            _ => {
                eprintln!("WARN: no micropb mapping for '{key}:{value}'");
                None
            }
        }
    }
}

pub fn checkout_protos(
    workspace_dir: &Path,
    meshtastic_protos_dir: &Path,
    tag: &str,
) -> Result<()> {
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

pub fn parse_static_settings(
    meshtastic_dir: &Path,
) -> Result<PathedSettings> {
    let proto_dir = meshtastic_dir.join("meshtastic");
    let desc = tempfile::NamedTempFile::new()?;

    let mut proto_settings: PathedSettings = BTreeMap::new();

    for entry in fs::read_dir(&proto_dir)? {
        let entry = entry?;
        let path = entry.path();

        let Some("options") = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
        let proto_path = proto_dir.join(format!("{stem}.proto"));

        let _ = Command::new("protoc")
            .args([
                format!("--proto_path={}", proto_dir.display()),
                format!("--proto_path={}", meshtastic_dir.display()),
                format!("--descriptor_set_out={}", desc.path().display()),
                proto_path.to_str().unwrap().to_string(),
            ])
            .status()?;

        let out = fs::read(desc.path())?;
        let fd_set = FileDescriptorSet::decode(&out[..])?;

        for file in fd_set.file {
            let matchers = build_settings_matchers(&path)?;
            for msg in file.message_type {
                build_path_map(msg.name(), &msg, &mut proto_settings, &matchers);
            }
        }
    }
    Ok(proto_settings)
}

fn build_settings_matchers(filepath: &Path) -> Result<Vec<SettingsMatcher>> {
    let opts = std::fs::read_to_string(filepath)?;

    let mut matchers = Vec::new();

    for line in opts.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.split('#').next().unwrap().trim();
        if let Some(matcher) = SettingsMatcher::from_nanopb(line) {
            matchers.push(matcher);
        }
    }

    Ok(matchers)
}

fn build_path_map(
    prefix: &str,
    msg: &DescriptorProto,
    map: &mut PathedSettings,
    matchers: &[SettingsMatcher],
) {
    for field in &msg.field {
        let path = format!("{prefix}.{}", field.name());
        set_path_settings(&path, map, matchers);
    }

    for oneof in &msg.oneof_decl {
        let path = format!("{prefix}.{}", oneof.name());
        set_path_settings(&path, map, matchers);
    }

    for nested in &msg.nested_type {
        let path = format!("{prefix}.{}", nested.name());
        build_path_map(&path, nested, map, matchers);
    }
}

fn set_path_settings(path: &str, map: &mut PathedSettings, matchers: &[SettingsMatcher]) {
    for matcher in matchers {
        if glob::glob_match(&matcher.pattern, path) {
            let entry = map.entry(path.into()).or_default();
            for setting in &matcher.settings {
                entry.insert(setting.name.to_string(), setting.value.clone());
            }
        }
    }
}

pub fn write_micropb_config(filepath: &Path, settings: PathedSettings) -> Result<()> {
    let file = fs::File::create(filepath)?;
    let mut buf = BufWriter::new(file);

    buf.write_all(b"#### !!!!! GENERATED NO EDIT !!!!!\n#### run 'cargo xtask proto:install' to generate")?;

    for (path, settings) in settings.iter() {
        write!(buf, "\n\n[\"{path}\"]")?;
        for (key, value) in settings.iter() {
            write!(buf, "\n{key} = {value}")?;
        }
    }

    Ok(())
}
