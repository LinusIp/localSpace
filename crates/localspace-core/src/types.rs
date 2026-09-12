//! `types.toml` — what a `kind = "types"` package declares (plugin spec §18.3):
//! the interchange types harnesses hand each other, each with a MIME type, a
//! file extension and the fields an artifact of it carries. Core refuses an
//! artifact of a kind no installed types package declares, and one that lacks
//! a required field, so a producer and a consumer never guess at a format.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// One interchange type, e.g. `image.v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The MIME type of an artifact's content, e.g. `image/png`.
    pub mime: String,
    /// The extension a download of it gets, with its dot.
    pub extension: String,
    /// Fields every artifact of this type carries: for a rendering, the
    /// `document` and `commit` it was made from.
    #[serde(default)]
    pub required: Vec<String>,
}

impl TypeDecl {
    /// The required fields `fields` lacks, in declared order.
    pub fn missing(&self, fields: &serde_json::Map<String, serde_json::Value>) -> Vec<&str> {
        self.required
            .iter()
            .filter(|f| fields.get(f.as_str()).is_none_or(|v| v.is_null()))
            .map(String::as_str)
            .collect()
    }
}

/// The declarations of one types package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeSet {
    #[serde(default)]
    pub types: BTreeMap<String, TypeDecl>,
}

impl TypeSet {
    pub fn load(path: &Path) -> Result<TypeSet> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        TypeSet::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<TypeSet> {
        let set: TypeSet = toml::from_str(text).context("parsing types.toml")?;
        if set.types.is_empty() {
            bail!("types.toml declares no types");
        }
        for (kind, decl) in &set.types {
            if !crate::manifest::valid_interchange_kind(kind) {
                bail!(
                    "`{kind}` is not an interchange type; expected the form `name.vN`, e.g. image.v1"
                );
            }
            if decl.title.trim().is_empty() {
                bail!("`{kind}` needs a title");
            }
            if !decl.mime.contains('/') || decl.mime.contains(char::is_whitespace) {
                bail!("`{kind}`: `{}` is not a MIME type", decl.mime);
            }
            if decl.extension.len() < 2 || !decl.extension.starts_with('.') {
                bail!(
                    "`{kind}`: the extension `{}` should be like `.png`",
                    decl.extension
                );
            }
            for field in &decl.required {
                let identifier = !field.is_empty()
                    && field
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                if !identifier {
                    bail!(
                        "`{kind}`: the required field `{field}` should be a lower-case identifier"
                    );
                }
            }
        }
        Ok(set)
    }

    pub fn get(&self, kind: &str) -> Option<&TypeDecl> {
        self.types.get(kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[types."image.v1"]
title = "Raster image"
mime = "image/png"
extension = ".png"
required = ["document", "commit"]

[types."outline.v1"]
title = "Outline"
mime = "application/json"
extension = ".json"
"#;

    #[test]
    fn parses_declarations_and_their_required_fields() {
        let set = TypeSet::parse(SAMPLE).unwrap();
        assert_eq!(
            set.kinds().collect::<Vec<_>>(),
            vec!["image.v1", "outline.v1"]
        );
        let image = set.get("image.v1").unwrap();
        assert_eq!(image.mime, "image/png");
        assert_eq!(image.extension, ".png");
        assert!(set.get("outline.v1").unwrap().required.is_empty());
        assert!(set.get("mesh.v1").is_none());
    }

    #[test]
    fn missing_lists_what_an_artifact_lacks_in_declared_order() {
        let set = TypeSet::parse(SAMPLE).unwrap();
        let image = set.get("image.v1").unwrap();
        let mut fields = serde_json::Map::new();
        assert_eq!(image.missing(&fields), vec!["document", "commit"]);
        fields.insert("commit".into(), "abc".into());
        assert_eq!(image.missing(&fields), vec!["document"]);
        fields.insert("document".into(), serde_json::Value::Null);
        assert_eq!(image.missing(&fields), vec!["document"], "null is absent");
        fields.insert("document".into(), "io_localspace_whiteboard".into());
        assert!(image.missing(&fields).is_empty());
    }

    #[test]
    fn refuses_a_malformed_declaration() {
        let bad_kind = SAMPLE.replace("image.v1", "Image");
        assert!(
            TypeSet::parse(&bad_kind)
                .unwrap_err()
                .to_string()
                .contains("interchange type")
        );
        let bad_extension = SAMPLE.replace("\".png\"", "\"png\"");
        assert!(
            TypeSet::parse(&bad_extension)
                .unwrap_err()
                .to_string()
                .contains(".png")
        );
        let bad_mime = SAMPLE.replace("image/png", "png");
        assert!(
            TypeSet::parse(&bad_mime)
                .unwrap_err()
                .to_string()
                .contains("MIME")
        );
        let bad_field = SAMPLE.replace("\"commit\"", "\"Commit Id\"");
        assert!(
            TypeSet::parse(&bad_field)
                .unwrap_err()
                .to_string()
                .contains("identifier")
        );
        assert!(
            TypeSet::parse("")
                .unwrap_err()
                .to_string()
                .contains("no types")
        );
    }
}
