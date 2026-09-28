//! What a plugin file is, read without compiling or running any of it.

use anyhow::{Context as _, Result, bail};
use delight_manifest::{Manifest, SECTION, decode_section};
use wasmparser::{Parser, Payload};

/// The manifest in a plugin's `.wasm`: its [`SECTION`] custom section, decoded and
/// checked (see `delight_manifest::decode_section`).
///
/// The section sits in the component's core module, not at its top level, so nested
/// modules and components are searched too. Nothing is compiled or validated beyond
/// finding it: that happens only when a plugin is started.
pub fn read_manifest(wasm: &[u8]) -> Result<Manifest> {
    let mut found = None;
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.context("this isn't a WebAssembly file")?;
        if let Payload::CustomSection(section) = payload
            && section.name() == SECTION
            && found.replace(section.data()).is_some()
        {
            bail!("the plugin has more than one {SECTION:?} section");
        }
    }
    let Some(section) = found else {
        bail!("this isn't a Delight plugin: it has no {SECTION:?} section");
    };
    decode_section(section)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use delight_manifest::{Operation, PluginProperties, encode_operations, encode_properties};
    use wasm_encoder::{Component, CustomSection, Module, ModuleSection};

    use super::*;

    fn section() -> Vec<u8> {
        let properties = PluginProperties {
            id: "dev.delight.test".into(),
            name: "Test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            author: String::new(),
            icon: "<svg/>".into(),
            tags: Vec::new(),
            permissions: Vec::new(),
        };
        let operations = [Operation {
            id: "echo".into(),
            title: "Echo".into(),
            description: String::new(),
            icon: None,
            tags: Vec::new(),
        }];
        let mut bytes = encode_properties(&properties);
        bytes.extend(encode_operations(&operations));
        bytes
    }

    /// A core module holding these custom sections, as a plugin's compiled code does.
    fn module(sections: &[(&str, &[u8])]) -> Module {
        let mut module = Module::new();
        for (name, data) in sections {
            module.section(&CustomSection {
                name: Cow::Borrowed(name),
                data: Cow::Borrowed(data),
            });
        }
        module
    }

    /// A component around that module: the shape of a plugin's `.wasm`.
    fn component(module: &Module) -> Vec<u8> {
        let mut component = Component::new();
        component.section(&ModuleSection(module));
        component.finish()
    }

    #[test]
    fn finds_the_section_inside_the_components_core_module() {
        let section = section();
        let wasm = component(&module(&[("name", b"debug names"), (SECTION, &section)]));
        let manifest = read_manifest(&wasm).unwrap();
        assert_eq!(manifest.plugin.id, "dev.delight.test");
        assert_eq!(manifest.operations[0].id, "echo");
    }

    #[test]
    fn finds_it_at_the_top_of_a_plain_module_too() {
        let section = section();
        let wasm = module(&[(SECTION, &section)]).finish();
        assert_eq!(read_manifest(&wasm).unwrap().plugin.name, "Test");
    }

    #[test]
    fn a_wasm_without_the_section_is_not_a_delight_plugin() {
        let wasm = component(&module(&[("name", b"debug names")]));
        let error = read_manifest(&wasm).unwrap_err();
        assert!(error.to_string().contains("isn't a Delight plugin"), "{error:#}");
    }

    #[test]
    fn two_sections_are_refused() {
        let section = section();
        let wasm = component(&module(&[(SECTION, &section), (SECTION, &section)]));
        let error = read_manifest(&wasm).unwrap_err();
        assert!(error.to_string().contains("more than one"), "{error:#}");
    }

    #[test]
    fn not_webassembly_is_refused() {
        let error = read_manifest(b"just some text").unwrap_err();
        assert!(error.to_string().contains("isn't a WebAssembly file"), "{error:#}");
    }

    #[test]
    fn a_bad_section_is_refused_with_the_decoders_reason() {
        let wasm = component(&module(&[(SECTION, b"{\"major\":99,\"minor\":0}\n{}\n[]")]));
        let error = read_manifest(&wasm).unwrap_err();
        assert!(error.to_string().contains("built for protocol 99.0"), "{error:#}");
    }
}
