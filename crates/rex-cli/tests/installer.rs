//! The Windows setup wizard is built only on the release runner, so nothing in
//! the ordinary CI would notice a renamed image, a wrongly sized bitmap or a
//! version that drifted. These checks read the files from the source tree and
//! run everywhere.

use serde_json::Value;
use std::path::{Path, PathBuf};

fn tauri_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri")
}

fn conf() -> Value {
    let raw = std::fs::read_to_string(tauri_dir().join("tauri.conf.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

struct Bmp {
    magic: [u8; 2],
    header_size: u32,
    width: i32,
    height: i32,
    bits: u16,
    compression: u32,
}

fn bmp(path: &str) -> Bmp {
    let b = std::fs::read(tauri_dir().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    Bmp {
        magic: [b[0], b[1]],
        header_size: u32_at(14),
        width: u32_at(18) as i32,
        height: (u32_at(22) as i32).abs(),
        bits: u16::from_le_bytes([b[28], b[29]]),
        compression: u32_at(30),
    }
}

#[test]
fn the_wizard_artwork_is_24_bit_uncompressed_bitmaps_at_the_sizes_nsis_draws_them() {
    // NSIS rejects other bitmap flavours (palette, RLE, newer headers) at build time.
    let conf = conf();
    let nsis = &conf["bundle"]["windows"]["nsis"];
    for (key, w, h) in [
        ("sidebarImage", 164, 314),
        ("headerImage", 150, 57),
        ("uninstallerHeaderImage", 150, 57),
    ] {
        let path = nsis[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} is not set"));
        let b = bmp(path);
        assert_eq!(&b.magic, b"BM", "{path}");
        assert_eq!(b.header_size, 40, "{path}: BITMAPINFOHEADER only");
        assert_eq!((b.width, b.height), (w, h), "{path}");
        assert_eq!(b.bits, 24, "{path}");
        assert_eq!(b.compression, 0, "{path}");
    }
}

#[test]
fn the_wizard_points_at_real_files_and_shows_both_licences() {
    let conf = conf();
    let nsis = &conf["bundle"]["windows"]["nsis"];
    let icon = nsis["installerIcon"].as_str().unwrap();
    assert!(tauri_dir().join(icon).is_file(), "{icon}");
    let license = conf["bundle"]["licenseFile"].as_str().unwrap();
    let text = std::fs::read_to_string(tauri_dir().join(license)).unwrap();
    assert!(text.contains("MIT LICENSE") && text.contains("APACHE LICENSE, VERSION 2.0"));
    assert!(text.contains("Permission is hereby granted, free of charge"));
    assert_eq!(conf["bundle"]["license"], "MIT OR Apache-2.0");
    assert_eq!(nsis["installMode"], "both");
    assert_eq!(nsis["languages"], serde_json::json!(["English"]));
}

/// The version a manifest declares: the first `version = "x"` line of a
/// Cargo.toml or Gradle file, or the `version` field of a JSON file.
fn declared_version(path: &std::path::Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if path.extension().is_some_and(|e| e == "json") {
        let v: Value = serde_json::from_str(&text).unwrap();
        return v["version"].as_str().unwrap().to_string();
    }
    text.lines()
        .find_map(|l| l.strip_prefix("version = \"")?.strip_suffix('"'))
        .unwrap_or_else(|| panic!("{}: no version line", path.display()))
        .to_string()
}

/// Crates that version themselves (the prompt contract is `rex-prompt 1.0.0`),
/// so they are deliberately not tied to the release.
const OWN_VERSION: &[&str] = &["rex-prompt"];

#[test]
fn every_manifest_carries_the_same_version_the_release_tag_must_equal() {
    let conf = conf();
    let version = conf["version"].as_str().unwrap();
    let root = tauri_dir().join("..");

    let mut manifests: Vec<PathBuf> = vec![
        root.join("package.json"),
        tauri_dir().join("Cargo.toml"),
        root.join("examples/rex-ultra-scripted-host/Cargo.toml"),
        root.join("packages/rex-mcp-npm/package.json"),
        root.join("extensions/vscode/package.json"),
        root.join("extensions/jetbrains/build.gradle.kts"),
    ];
    // Every crate, so a new one cannot be forgotten at the next release.
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if !OWN_VERSION.contains(&name.as_str()) {
            manifests.push(dir.join("Cargo.toml"));
        }
    }
    let wrong: Vec<String> = manifests
        .iter()
        .filter(|m| declared_version(m) != version)
        .map(|m| format!("{} says {}", m.display(), declared_version(m)))
        .collect();
    assert!(
        wrong.is_empty(),
        "these do not match tauri.conf.json ({version}): {wrong:#?}"
    );

    // The lockfile's root entry, which `npm ci` checks against package.json.
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("package-lock.json")).unwrap())
            .unwrap();
    assert_eq!(lock["version"], version, "package-lock.json");
    assert_eq!(
        lock["packages"][""]["version"], version,
        "package-lock.json root package"
    );

    // The manual-release form defaults to the tag this version needs.
    let release = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(
        release.contains(&format!("default: \"v{version}\"")),
        "release.yml's default tag is not v{version}"
    );
}
