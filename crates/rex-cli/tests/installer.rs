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

#[test]
fn every_manifest_carries_the_same_version_the_release_tag_must_equal() {
    let conf = conf();
    let version = conf["version"].as_str().unwrap();
    let root = tauri_dir().join("..");
    let pkg: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
    assert_eq!(pkg["version"], version, "package.json");
    let cargo = std::fs::read_to_string(tauri_dir().join("Cargo.toml")).unwrap();
    let cargo_version = cargo
        .lines()
        .find_map(|l| l.strip_prefix("version = \"")?.strip_suffix('"'))
        .unwrap();
    assert_eq!(cargo_version, version, "src-tauri/Cargo.toml");
}
