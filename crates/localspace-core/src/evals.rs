//! The agent-compatibility score (spec §12).
//!
//! Every package ships `evals.json`: tasks phrased as a user would phrase them,
//! each with an assertion on the resulting document. Core runs them against the
//! model the environment actually has loaded and reports a pass rate. A plugin a
//! human can use but a model cannot drive is a broken plugin here, and nothing
//! else surfaces that.

use crate::{agent, Core};
use anyhow::{Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use serde_json::Value as J;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    /// Phrased as a user would phrase it.
    pub prompt: String,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

/// Assertions run against the harness document after the turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assertion {
    /// Dot path into the document, e.g. `shapes.0.kind`. Empty means the root.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub equals: Option<J>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub min_len: Option<usize>,
    #[serde(default)]
    pub max_len: Option<usize>,
    #[serde(default)]
    pub exists: Option<bool>,
}

impl Assertion {
    pub fn check(&self, doc: &J) -> std::result::Result<(), String> {
        let found = resolve(doc, &self.path);

        if let Some(want) = self.exists {
            let is = found.is_some();
            if is != want {
                return Err(format!(
                    "`{}` {} exist",
                    display_path(&self.path),
                    if want { "should" } else { "should not" }
                ));
            }
            if !want {
                return Ok(());
            }
        }

        let Some(value) = found else {
            return Err(format!("`{}` is not in the document", display_path(&self.path)));
        };

        if let Some(want) = &self.equals
            && value != want {
                return Err(format!(
                    "`{}` is {value}, expected {want}",
                    display_path(&self.path)
                ));
            }
        if let Some(needle) = &self.contains {
            let hay = match value {
                J::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !hay.to_lowercase().contains(&needle.to_lowercase()) {
                return Err(format!(
                    "`{}` does not contain `{needle}`",
                    display_path(&self.path)
                ));
            }
        }
        if let Some(min) = self.min_len {
            let len = len_of(value);
            if len < min {
                return Err(format!(
                    "`{}` has {len} item(s), expected at least {min}",
                    display_path(&self.path)
                ));
            }
        }
        if let Some(max) = self.max_len {
            let len = len_of(value);
            if len > max {
                return Err(format!(
                    "`{}` has {len} item(s), expected at most {max}",
                    display_path(&self.path)
                ));
            }
        }
        Ok(())
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() {
        "<document>"
    } else {
        path
    }
}

fn len_of(v: &J) -> usize {
    match v {
        J::Array(a) => a.len(),
        J::Object(o) => o.len(),
        J::String(s) => s.chars().count(),
        _ => 0,
    }
}

/// Resolve a dot path, treating numeric segments as array indices.
pub fn resolve<'a>(doc: &'a J, path: &str) -> Option<&'a J> {
    if path.is_empty() {
        return Some(doc);
    }
    let mut cur = doc;
    for seg in path.split('.') {
        cur = match cur {
            J::Object(map) => map.get(seg)?,
            J::Array(arr) => arr.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Run a harness's suite against the model this environment has loaded.
pub fn run(core: &mut Core, harness_id: &str) -> Result<proto::EvalReport> {
    let (dir, doc_id) = {
        let h = core
            .registry
            .get(harness_id)
            .with_context(|| format!("no harness `{harness_id}`"))?;
        (h.dir.clone(), h.doc_id.clone())
    };
    let path = dir.join("evals.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("`{harness_id}` ships no evals.json at {}", path.display()))?;
    let suite: EvalSuite = parse_suite(&text)?;

    let model = core
        .router
        .read()
        .unwrap()
        .info()
        .map(|m| m.id)
        .unwrap_or_else(|| "no model loaded".into());

    let mut cases = Vec::new();
    let mut passed = 0usize;

    for case in &suite.cases {
        // Each case starts from a clean document, so cases cannot depend on order.
        let kind = core
            .registry
            .get(harness_id)
            .map(|h| h.doc_kind())
            .unwrap_or(proto::DocKind::Crdt);
        core.docs.restore(&doc_id, None).ok();
        core.docs.ensure(&doc_id, kind);
        core.transcript.clear();
        core.focus = Some(harness_id.to_string());
        core.providers = crate::context::ProviderCache::new();

        agent::turn(core, &case.prompt);

        let doc = core.docs.json(&doc_id).unwrap_or(J::Null);
        let mut failures = Vec::new();
        for a in &case.assertions {
            if let Err(e) = a.check(&doc) {
                failures.push(e);
            }
        }
        let ok = failures.is_empty();
        if ok {
            passed += 1;
        }
        cases.push(proto::EvalCase {
            name: case.name.clone(),
            prompt: case.prompt.clone(),
            passed: ok,
            detail: if ok {
                "passed".into()
            } else {
                failures.join("; ")
            },
        });
    }

    Ok(proto::EvalReport {
        harness: harness_id.to_string(),
        model,
        passed,
        total: suite.cases.len(),
        cases,
    })
}

/// Accepts either `{"cases": [...]}` or a bare array of cases.
pub fn parse_suite(text: &str) -> Result<EvalSuite> {
    if let Ok(s) = serde_json::from_str::<EvalSuite>(text) {
        return Ok(s);
    }
    let cases: Vec<EvalCase> =
        serde_json::from_str(text).context("evals.json must be a case array or {\"cases\": [...]}")?;
    Ok(EvalSuite { cases })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc() -> J {
        json!({
            "title": "Q4 Risks",
            "shapes": [
                {"id": "s1", "kind": "rect", "fill": "red", "text": "Supply chain"},
                {"id": "s2", "kind": "rect", "fill": "red", "text": "Hiring"},
                {"id": "s3", "kind": "rect", "fill": "red", "text": "FX"}
            ]
        })
    }

    fn assertion(path: &str) -> Assertion {
        Assertion {
            path: path.into(),
            equals: None,
            contains: None,
            min_len: None,
            max_len: None,
            exists: None,
        }
    }

    #[test]
    fn paths_resolve_through_objects_and_arrays() {
        assert_eq!(resolve(&doc(), "title").unwrap(), &json!("Q4 Risks"));
        assert_eq!(resolve(&doc(), "shapes.1.text").unwrap(), &json!("Hiring"));
        assert!(resolve(&doc(), "shapes.9.text").is_none());
        assert!(resolve(&doc(), "nope").is_none());
    }

    #[test]
    fn the_spec_s_example_task_passes_when_the_document_matches() {
        // "put the three risks on the board as red stickies"
        let checks = vec![
            Assertion {
                min_len: Some(3),
                ..assertion("shapes")
            },
            Assertion {
                equals: Some(json!("red")),
                ..assertion("shapes.0.fill")
            },
        ];
        for c in &checks {
            c.check(&doc()).unwrap();
        }
    }

    #[test]
    fn a_failing_assertion_says_what_was_wrong() {
        let a = Assertion {
            min_len: Some(5),
            ..assertion("shapes")
        };
        let err = a.check(&doc()).unwrap_err();
        assert!(err.contains("3 item(s)"), "{err}");
        assert!(err.contains("at least 5"), "{err}");

        let b = Assertion {
            equals: Some(json!("blue")),
            ..assertion("shapes.0.fill")
        };
        assert!(b.check(&doc()).unwrap_err().contains("expected"));

        let c = Assertion {
            contains: Some("hiring".into()),
            ..assertion("shapes.1.text")
        };
        c.check(&doc()).unwrap(); // case-insensitive
    }

    #[test]
    fn a_missing_path_fails_rather_than_passing_vacuously() {
        let a = Assertion {
            min_len: Some(1),
            ..assertion("frames")
        };
        assert!(a.check(&doc()).unwrap_err().contains("not in the document"));
    }

    #[test]
    fn exists_false_passes_when_the_path_is_absent() {
        let a = Assertion {
            exists: Some(false),
            ..assertion("frames")
        };
        a.check(&doc()).unwrap();

        let b = Assertion {
            exists: Some(false),
            ..assertion("title")
        };
        assert!(b.check(&doc()).is_err());
    }

    #[test]
    fn both_suite_shapes_parse() {
        let wrapped = r#"{"cases":[{"name":"a","prompt":"p","assertions":[]}]}"#;
        let bare = r#"[{"name":"a","prompt":"p","assertions":[]}]"#;
        assert_eq!(parse_suite(wrapped).unwrap().cases.len(), 1);
        assert_eq!(parse_suite(bare).unwrap().cases.len(), 1);
        assert!(parse_suite("not json").is_err());
    }
}
