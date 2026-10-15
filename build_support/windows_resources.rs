use std::io;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn dword(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn align(bytes: &mut Vec<u8>) {
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
}

fn resource(output: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    align(output);
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.extend_from_slice(&32u32.to_le_bytes());
    for value in [u16::MAX, kind, u16::MAX, name] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(&flags.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes());
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(&0u32.to_le_bytes());
    output.extend_from_slice(data);
    align(output);
}

fn utf16(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn version_block(
    key: &str,
    value: &[u8],
    value_length: u16,
    value_type: u16,
    children: &[Vec<u8>],
) -> Vec<u8> {
    let mut bytes = vec![0, 0];
    bytes.extend_from_slice(&value_length.to_le_bytes());
    bytes.extend_from_slice(&value_type.to_le_bytes());
    bytes.extend_from_slice(&utf16(key));
    align(&mut bytes);
    bytes.extend_from_slice(value);
    for child in children {
        align(&mut bytes);
        bytes.extend_from_slice(child);
    }
    let length = u16::try_from(bytes.len()).expect("version resource exceeds 64 KiB");
    bytes[..2].copy_from_slice(&length.to_le_bytes());
    bytes
}

fn version_info(version: &str) -> io::Result<Vec<u8>> {
    let mut components = [0u16; 4];
    for (index, part) in version
        .split(['-', '+'])
        .next()
        .unwrap()
        .split('.')
        .enumerate()
    {
        if index >= components.len() {
            return Err(invalid("too many version components"));
        }
        components[index] = part
            .parse()
            .map_err(|_| invalid("invalid resource version"))?;
    }
    let high = (u32::from(components[0]) << 16) | u32::from(components[1]);
    let low = (u32::from(components[2]) << 16) | u32::from(components[3]);
    let fixed = [
        0xfeef04bdu32,
        0x00010000,
        high,
        low,
        high,
        low,
        0x3f,
        0,
        0x40004,
        1,
        0,
        0,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect::<Vec<_>>();
    let strings = [
        (
            "FileDescription",
            "原神祈愿 / 崩坏：星穹铁道跃迁记录导出工具",
        ),
        ("FileVersion", version),
        ("LegalCopyright", "MIT License"),
        ("OriginalFilename", "aurora-export.exe"),
        ("ProductName", "AuroraExport"),
        ("ProductVersion", version),
    ]
    .into_iter()
    .map(|(key, text)| {
        let value = utf16(text);
        version_block(key, &value, (value.len() / 2) as u16, 1, &[])
    })
    .collect::<Vec<_>>();
    let table = version_block("040904b0", &[], 0, 1, &strings);
    let string_info = version_block("StringFileInfo", &[], 0, 1, &[table]);
    let translation = version_block("Translation", &[0x09, 0x04, 0xb0, 0x04], 4, 0, &[]);
    let var_info = version_block("VarFileInfo", &[], 0, 1, &[translation]);
    Ok(version_block(
        "VS_VERSION_INFO",
        &fixed,
        fixed.len() as u16,
        0,
        &[string_info, var_info],
    ))
}

pub fn compile(icon: &[u8], manifest: &[u8], version: &str) -> io::Result<Vec<u8>> {
    if icon.len() < 6 || word(icon, 0) != 0 || word(icon, 2) != 1 {
        return Err(invalid("invalid ICO header"));
    }
    let count = word(icon, 4);
    let directory_end = 6 + usize::from(count) * 16;
    if count == 0 || directory_end > icon.len() {
        return Err(invalid("invalid ICO directory"));
    }
    if manifest.is_empty() || std::str::from_utf8(manifest).is_err() {
        return Err(invalid("manifest must contain UTF-8 XML"));
    }
    let mut output = Vec::new();
    resource(&mut output, 0, 0, 0, &[]);
    let mut group = icon[..6].to_vec();
    for index in 0..count {
        let entry = 6 + usize::from(index) * 16;
        let size = dword(icon, entry + 8) as usize;
        let offset = dword(icon, entry + 12) as usize;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| invalid("ICO image overflow"))?;
        if size == 0 || offset < directory_end || end > icon.len() {
            return Err(invalid("ICO image lies outside the file"));
        }
        let id = index + 1;
        resource(&mut output, 3, id, 0x1030, &icon[offset..end]);
        group.extend_from_slice(&icon[entry..entry + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut output, 14, 1, 0x1030, &group);
    resource(&mut output, 16, 1, 0x30, &version_info(version)?);
    resource(&mut output, 24, 1, 0x30, manifest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &[u8] = include_bytes!("../assets/icon.ico");
    const MANIFEST: &[u8] = include_bytes!("../assets/app.manifest");

    fn records(bytes: &[u8]) -> Vec<(u16, u16, &[u8])> {
        let mut records = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            assert_eq!(offset % 4, 0);
            let size = dword(bytes, offset) as usize;
            let header = dword(bytes, offset + 4) as usize;
            assert_eq!(header, 32);
            assert_eq!(word(bytes, offset + 8), u16::MAX);
            assert_eq!(word(bytes, offset + 12), u16::MAX);
            let end = offset + header + size;
            records.push((
                word(bytes, offset + 10),
                word(bytes, offset + 14),
                &bytes[offset + header..end],
            ));
            offset = (end + 3) & !3;
        }
        assert_eq!(offset, bytes.len());
        records
    }

    #[test]
    fn resources_preserve_all_icon_sizes_and_manifest() {
        let output = compile(ICON, MANIFEST, "1.0.0").unwrap();
        let records = records(&output);
        let count = word(ICON, 4);
        assert_eq!(records.len(), usize::from(count) + 4);
        for index in 0..count {
            let entry = 6 + usize::from(index) * 16;
            let size = dword(ICON, entry + 8) as usize;
            let offset = dword(ICON, entry + 12) as usize;
            assert_eq!(
                records[usize::from(index) + 1],
                (3, index + 1, &ICON[offset..offset + size])
            );
        }
        let group = records.iter().find(|record| record.0 == 14).unwrap().2;
        assert_eq!(word(group, 4), count);
        assert_eq!(group.len(), 6 + usize::from(count) * 14);
        for index in 0..count {
            assert_eq!(word(group, 6 + usize::from(index) * 14 + 12), index + 1);
        }
        assert_eq!(records.last().unwrap(), &(24, 1, MANIFEST));
    }

    #[test]
    fn malformed_assets_are_rejected() {
        assert!(compile(&[], MANIFEST, "1.0.0").is_err());
        assert!(compile(&ICON[..10], MANIFEST, "1.0.0").is_err());
        assert!(compile(&ICON[..ICON.len() - 1], MANIFEST, "1.0.0").is_err());
        assert!(compile(ICON, &[], "1.0.0").is_err());
        assert!(compile(ICON, &[0xff], "1.0.0").is_err());
        assert!(compile(ICON, MANIFEST, "not-a-version").is_err());
    }

    #[test]
    fn version_resource_contains_fixed_and_string_information() {
        let version = version_info("1.2.3").unwrap();
        assert_eq!(usize::from(word(&version, 0)), version.len());
        assert_eq!(word(&version, 2), 52);
        let fixed_offset = (6 + utf16("VS_VERSION_INFO").len() + 3) & !3;
        assert_eq!(dword(&version, fixed_offset), 0xfeef04bd);
        assert_eq!(dword(&version, fixed_offset + 8), 0x00010002);
        assert_eq!(dword(&version, fixed_offset + 12), 0x00030000);
        for expected in ["AuroraExport", "aurora-export.exe", "1.2.3", "原神祈愿"] {
            let text = expected
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            assert!(version.windows(text.len()).any(|bytes| bytes == text));
        }
    }
}
