//! The plugin's custom section: how its protocol version and manifest are stored in
//! the `.wasm`, and read back.

use anyhow::{Context as _, Result, bail};

use crate::{Manifest, Operation, PROTOCOL_VERSION, PluginProperties, ProtocolVersion};

/// The custom section of a plugin's `.wasm` that says what it is: three lines of
/// JSON, the protocol version, the plugin's properties and its operations. `#[plugin]`
/// writes the first two ([`encode_properties`]) and `#[derive(Operations)]` the third
/// ([`encode_operations`]); the plugin's entry point joins them at compile time.
pub const SECTION: &str = "delight-plugin";

/// The first two lines of a plugin's [`SECTION`]: [`PROTOCOL_VERSION`] and the
/// plugin's properties, each ending in a newline.
pub fn encode_properties(properties: &PluginProperties) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&PROTOCOL_VERSION).expect("a version serializes");
    bytes.push(b'\n');
    bytes.extend(serde_json::to_vec(properties).expect("properties serialize"));
    bytes.push(b'\n');
    bytes
}

/// The last line of a plugin's [`SECTION`]: its operations.
pub fn encode_operations(operations: &[Operation]) -> Vec<u8> {
    serde_json::to_vec(operations).expect("operations serialize")
}

/// Read a plugin's [`SECTION`]: refuse a protocol version this app doesn't run before
/// looking at the rest (whose shape may differ there), then parse and check the
/// manifest. Compact JSON has no raw newlines, so the lines split cleanly.
pub fn decode_section(bytes: &[u8]) -> Result<Manifest> {
    let lines: Vec<&[u8]> = bytes.split(|&byte| byte == b'\n').collect();
    let [protocol, plugin, operations] = lines[..] else {
        bail!("the plugin's section isn't valid: expected 3 lines");
    };
    let protocol: ProtocolVersion =
        serde_json::from_slice(protocol).context("the plugin's protocol version isn't valid")?;
    if !PROTOCOL_VERSION.supports(protocol) {
        bail!(
            "built for protocol {protocol}, and this Delight runs {}.0 to {PROTOCOL_VERSION}",
            PROTOCOL_VERSION.major,
        );
    }
    let manifest = Manifest {
        plugin: serde_json::from_slice(plugin).context("the plugin's properties aren't valid")?,
        operations: serde_json::from_slice(operations)
            .context("the plugin's operations aren't valid")?,
    };
    manifest.validate()?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::sample;

    fn section(manifest: &Manifest) -> Vec<u8> {
        let mut bytes = encode_properties(&manifest.plugin);
        bytes.extend(encode_operations(&manifest.operations));
        bytes
    }

    #[test]
    fn the_section_round_trips() {
        let manifest = sample();
        assert_eq!(decode_section(&section(&manifest)).unwrap(), manifest);
    }

    #[test]
    fn the_section_is_three_lines_of_json() {
        let text = String::from_utf8(section(&sample())).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 3);
        let version = format!(r#"{{"major":{},"minor":{}}}"#, PROTOCOL_VERSION.major, PROTOCOL_VERSION.minor);
        assert_eq!(lines[0], version);
        assert!(lines[1].starts_with(r#"{"id":"dev.delight.json","#), "{}", lines[1]);
        assert!(lines[2].starts_with(r#"[{"id":"format","#), "{}", lines[2]);
    }

    #[test]
    fn a_section_for_an_unsupported_protocol_is_refused_before_the_rest_is_read() {
        let newer = format!(
            "{{\"major\":{},\"minor\":{}}}\nnot a plugin this app knows\nnor operations",
            PROTOCOL_VERSION.major,
            PROTOCOL_VERSION.minor + 1
        );
        let error = decode_section(newer.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("built for protocol"), "{error:#}");
    }

    #[test]
    fn a_broken_or_invalid_section_is_refused() {
        let mut bad_id = sample();
        bad_id.plugin.id = ".hidden".into();
        assert!(decode_section(&section(&bad_id)).is_err());
        assert!(decode_section(b"not json").is_err());
        let mut extra_line = section(&sample());
        extra_line.extend(b"\n[]");
        assert!(decode_section(&extra_line).is_err());
    }

    #[test]
    fn optional_fields_default() {
        let manifest = decode_section(
            b"{\"major\":1,\"minor\":0}\n\
              {\"id\":\"a\",\"name\":\"A\",\"version\":\"1\",\"icon\":\"<svg/>\"}\n\
              [{\"id\":\"x\",\"title\":\"X\"}]",
        )
        .unwrap();
        assert_eq!(manifest.plugin.description, "");
        assert!(manifest.plugin.permissions.is_empty());
        assert!(manifest.operations[0].tags.is_empty());
        assert_eq!(manifest.operations[0].icon, None);
    }
}
