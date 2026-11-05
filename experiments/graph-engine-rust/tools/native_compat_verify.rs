use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MANIFEST_SHA256: &str =
    "e64534513fb200261b1853452e8bf995dc9500660ef1064ff01c5d1b097fb6f8";
pub const INPUT_PATH: &str = "compat/unicode/cpython-3.12.10/unicode-tables.bin";

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn regular(path: &Path) -> Result<Vec<u8>, String> {
    for parent in path.ancestors() {
        let metadata = fs::symlink_metadata(parent).map_err(|_| "COMPAT_INPUT_MISSING")?;
        if metadata.file_type().is_symlink() {
            return Err("COMPAT_LINK_FORBIDDEN".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("COMPAT_LINK_FORBIDDEN".into());
            }
        }
    }
    let before = fs::metadata(path).map_err(|_| "COMPAT_INPUT_MISSING")?;
    if !before.is_file() {
        return Err("COMPAT_REGULAR_FILE_REQUIRED".into());
    }
    let bytes = fs::read(path).map_err(|_| "COMPAT_INPUT_READ_FAILURE")?;
    let after = fs::metadata(path).map_err(|_| "COMPAT_INPUT_CHANGED")?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() as u64 != after.len()
    {
        return Err("COMPAT_INPUT_CHANGED".into());
    }
    Ok(bytes)
}

pub struct VerifiedInput {
    pub bytes: Vec<u8>,
    pub input_sha256: String,
    pub input_path: PathBuf,
}

pub fn verify(package: &Path, artifacts: &Path) -> Result<VerifiedInput, String> {
    if !artifacts.is_absolute() {
        return Err("COMPAT_ARTIFACT_ROOT_ABSOLUTE_REQUIRED".into());
    }
    let manifest_bytes = regular(&package.join("compat/unicode-input-manifest.json"))?;
    if digest(&manifest_bytes) != MANIFEST_SHA256 {
        return Err("COMPAT_MANIFEST_DIGEST_MISMATCH".into());
    }
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "COMPAT_MANIFEST_INVALID")?;
    if manifest["schema_version"] != 1
        || manifest["input"]["path"] != INPUT_PATH
        || manifest["input"]["format"] != "graph-unicode"
        || manifest["input"]["version"] != 1
    {
        return Err("COMPAT_MANIFEST_INCOMPATIBLE".into());
    }
    let generator_path = manifest["generator"]["path"]
        .as_str()
        .ok_or("COMPAT_GENERATOR_INVALID")?;
    if generator_path != "tools/generate_unicode.py"
        || manifest["generator"]["transitive_inputs"] != serde_json::json!([])
    {
        return Err("COMPAT_GENERATOR_INVALID".into());
    }
    let generator = regular(&package.join(generator_path))?;
    if Some(digest(&generator).as_str()) != manifest["generator"]["sha256"].as_str() {
        return Err("COMPAT_GENERATOR_DIGEST_MISMATCH".into());
    }
    let repo = package
        .parent()
        .and_then(Path::parent)
        .ok_or("COMPAT_REPOSITORY_MISSING")?;
    let modules = manifest["oracle"]["modules"]
        .as_object()
        .ok_or("COMPAT_ORACLE_INVALID")?;
    if modules.len() != 17 {
        return Err("COMPAT_ORACLE_INVALID".into());
    }
    for (name, expected) in modules {
        if name.contains(['/', '\\']) || !name.ends_with(".py") {
            return Err("COMPAT_ORACLE_INVALID".into());
        }
        let source = regular(
            &repo
                .join("skills/software-engineering-graph/graph_engine")
                .join(name),
        )?;
        if Some(digest(&source).as_str()) != expected.as_str() {
            return Err("COMPAT_ORACLE_SOURCE_MISMATCH".into());
        }
    }
    let path = artifacts.join(INPUT_PATH);
    let bytes = regular(&path)?;
    let expected_length = manifest["input"]["size_bytes"]
        .as_u64()
        .ok_or("COMPAT_LENGTH_INVALID")?;
    if bytes.len() as u64 != expected_length {
        return Err("COMPAT_INPUT_LENGTH_MISMATCH".into());
    }
    let input_sha256 = digest(&bytes);
    if Some(input_sha256.as_str()) != manifest["input"]["sha256"].as_str() {
        return Err("COMPAT_INPUT_DIGEST_MISMATCH".into());
    }
    if bytes.get(..8) != Some(b"GUNICOD1") {
        return Err("COMPAT_INPUT_FORMAT_INVALID".into());
    }
    let header_length = u32::from_le_bytes(
        bytes
            .get(8..12)
            .ok_or("COMPAT_INPUT_TRUNCATED")?
            .try_into()
            .unwrap(),
    ) as usize;
    let header: Value = serde_json::from_slice(
        bytes
            .get(12..12 + header_length)
            .ok_or("COMPAT_INPUT_TRUNCATED")?,
    )
    .map_err(|_| "COMPAT_HEADER_INVALID")?;
    if header
        != serde_json::json!({"codepoints":0x110000,"format":"graph-unicode","version":1,"oracle":manifest["oracle"]})
    {
        return Err("COMPAT_INPUT_PROVENANCE_MISMATCH".into());
    }
    let count_offset = 12 + header_length + 0x110000 * 2;
    let count = u32::from_le_bytes(
        bytes
            .get(count_offset..count_offset + 4)
            .ok_or("COMPAT_INPUT_TRUNCATED")?
            .try_into()
            .unwrap(),
    );
    let mut offset = count_offset + 4;
    let mut previous = None;
    for _ in 0..count {
        let record = bytes
            .get(offset..offset + 6)
            .ok_or("COMPAT_INPUT_TRUNCATED")?;
        let point = u32::from_le_bytes(record[..4].try_into().unwrap());
        if point > 0x10ffff
            || previous.is_some_and(|p| p >= point)
            || record[4] == 0
            || record[5] == 0
        {
            return Err("COMPAT_MAPPING_INVALID".into());
        }
        previous = Some(point);
        offset += 6;
        for _ in 0..usize::from(record[4]) + usize::from(record[5]) {
            let point = u32::from_le_bytes(
                bytes
                    .get(offset..offset + 4)
                    .ok_or("COMPAT_INPUT_TRUNCATED")?
                    .try_into()
                    .unwrap(),
            );
            if point > 0x10ffff {
                return Err("COMPAT_MAPPING_INVALID".into());
            }
            offset += 4;
        }
    }
    if offset != bytes.len() {
        return Err("COMPAT_INPUT_TRAILING_BYTES".into());
    }
    Ok(VerifiedInput {
        bytes,
        input_sha256,
        input_path: path,
    })
}
