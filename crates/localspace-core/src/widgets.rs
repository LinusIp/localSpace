//! Parse the authoring form of a `widgets` view.
//!
//! Harness authors write the tree the way the WIT documents it — `{"w": "column",
//! "children": [...]}` — which is an internally tagged enum. `proto::Widget` is
//! externally tagged instead, because postcard has no `deserialize_any` and the
//! wire format has to work on both transports. This is the one place the two meet.

use anyhow::{bail, Context, Result};
use localspace_proto as proto;
use serde_json::Value as J;

pub fn parse(v: &J) -> Result<proto::Widget> {
    let obj = v.as_object().context("a widget must be a JSON object")?;
    let kind = obj
        .get("w")
        .and_then(|k| k.as_str())
        .context("a widget needs a \"w\" field naming its kind")?;

    let text = |key: &str| -> String {
        obj.get(key)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let flag = |key: &str| -> bool { obj.get(key).and_then(|x| x.as_bool()).unwrap_or(false) };
    let number = |key: &str, default: f64| -> f64 {
        obj.get(key).and_then(|x| x.as_f64()).unwrap_or(default)
    };
    let children = || -> Result<Vec<proto::Widget>> {
        obj.get("children")
            .and_then(|c| c.as_array())
            .map(|a| a.iter().map(parse).collect::<Result<Vec<_>>>())
            .unwrap_or_else(|| Ok(Vec::new()))
    };
    let strings = |key: &str| -> Vec<String> {
        obj.get(key)
            .and_then(|c| c.as_array())
            .map(|a| {
                a.iter()
                    .map(|x| match x {
                        J::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    Ok(match kind {
        "column" => proto::Widget::Column {
            children: children()?,
        },
        "row" => proto::Widget::Row {
            children: children()?,
        },
        "text" => proto::Widget::Text {
            text: text("text"),
            strong: flag("strong"),
            muted: flag("muted"),
        },
        "heading" => proto::Widget::Heading { text: text("text") },
        "separator" => proto::Widget::Separator,
        "space" => proto::Widget::Space {
            size: number("size", 8.0) as f32,
        },
        "button" => proto::Widget::Button {
            id: text("id"),
            label: text("label"),
            enabled: obj
                .get("enabled")
                .and_then(|x| x.as_bool())
                .unwrap_or(true),
        },
        "input" => proto::Widget::Input {
            id: text("id"),
            label: text("label"),
            value: text("value"),
            multiline: flag("multiline"),
        },
        "checkbox" => proto::Widget::Checkbox {
            id: text("id"),
            label: text("label"),
            value: flag("value"),
        },
        "select" => proto::Widget::Select {
            id: text("id"),
            label: text("label"),
            value: text("value"),
            options: strings("options"),
        },
        "slider" => proto::Widget::Slider {
            id: text("id"),
            label: text("label"),
            value: number("value", 0.0),
            min: number("min", 0.0),
            max: number("max", 1.0),
        },
        "list" => proto::Widget::List {
            items: strings("items"),
        },
        "table" => proto::Widget::Table {
            headers: strings("headers"),
            rows: obj
                .get("rows")
                .and_then(|r| r.as_array())
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| {
                                    cells
                                        .iter()
                                        .map(|c| match c {
                                            J::String(s) => s.clone(),
                                            other => other.to_string(),
                                        })
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        "badge" => proto::Widget::Badge {
            text: text("text"),
            tone: match obj.get("tone").and_then(|t| t.as_str()).unwrap_or("neutral") {
                "good" => proto::Tone::Good,
                "warn" => proto::Tone::Warn,
                "bad" => proto::Tone::Bad,
                _ => proto::Tone::Neutral,
            },
        },
        other => bail!("`{other}` is not a widget kind"),
    })
}

/// The reverse: what a widget event looks like to a harness's `event` export.
pub fn event_to_json(event: &proto::WidgetEvent) -> J {
    let value = match &event.value {
        proto::WidgetValue::Clicked => serde_json::json!({"Clicked": null}),
        proto::WidgetValue::Text(t) => serde_json::json!({"Text": t}),
        proto::WidgetValue::Bool(b) => serde_json::json!({"Bool": b}),
        proto::WidgetValue::Number(n) => serde_json::json!({"Number": n}),
    };
    serde_json::json!({"id": event.id, "value": value})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_authoring_form_parses_into_the_wire_form() {
        let tree = json!({
            "w": "column",
            "children": [
                {"w": "heading", "text": "Board"},
                {"w": "input", "id": "title", "label": "Title", "value": "Q4"},
                {"w": "separator"},
                {"w": "table", "headers": ["kind", "count"], "rows": [["sticky", "3"]]},
                {"w": "badge", "text": "3 shapes", "tone": "good"},
                {"w": "button", "id": "go", "label": "Go"}
            ]
        });
        let parsed = parse(&tree).unwrap();
        match parsed {
            proto::Widget::Column { children } => {
                assert_eq!(children.len(), 6);
                assert!(matches!(children[0], proto::Widget::Heading { .. }));
                match &children[4] {
                    proto::Widget::Badge { tone, .. } => assert_eq!(*tone, proto::Tone::Good),
                    other => panic!("wrong widget: {other:?}"),
                }
                // A button with no `enabled` key is enabled: authors should not
                // have to say so.
                match &children[5] {
                    proto::Widget::Button { enabled, .. } => assert!(*enabled),
                    other => panic!("wrong widget: {other:?}"),
                }
            }
            other => panic!("wrong root: {other:?}"),
        }
    }

    #[test]
    fn the_parsed_tree_survives_the_wire() {
        let parsed = parse(&json!({"w": "text", "text": "hello", "strong": true})).unwrap();
        let bytes = postcard::to_allocvec(&parsed).unwrap();
        let back: proto::Widget = postcard::from_bytes(&bytes).unwrap();
        match back {
            proto::Widget::Text { text, strong, .. } => {
                assert_eq!(text, "hello");
                assert!(strong);
            }
            other => panic!("wrong widget: {other:?}"),
        }
    }

    #[test]
    fn an_unknown_kind_is_reported_rather_than_dropped() {
        let err = parse(&json!({"w": "hologram"})).unwrap_err().to_string();
        assert!(err.contains("hologram"), "{err}");
        assert!(parse(&json!({"text": "no kind"})).is_err());
    }
}
