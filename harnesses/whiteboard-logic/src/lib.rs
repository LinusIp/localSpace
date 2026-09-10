//! Reference harness #1 — whiteboard logic (Tier A, `doc = "crdt"`).
//!
//! Proves the context provider and the surface ABI together: twelve tools, two of
//! them front door, and a provider that serialises the board to text at whatever
//! budget Core hands it.
//!
//! The document is the single source of truth. Every tool reads it with `doc-get`,
//! edits the JSON, and writes it back with `doc-put`; Core reconciles that into
//! the CRDT field by field, commits it, and pushes the patch to the surface.

wit_bindgen::generate!({
    path: "../../wit",
    world: "harness",
});

use serde_json::{json, Value};

struct Whiteboard;

export!(Whiteboard);

// ---------------------------------------------------------------------------
// Document helpers
// ---------------------------------------------------------------------------

fn load() -> Value {
    let raw = localspace::harness::host::doc_get();
    let mut doc: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if !doc.is_object() {
        doc = json!({});
    }
    for (key, default) in [
        ("title", json!("Board")),
        ("frames", json!([])),
        ("shapes", json!([])),
        ("selection", json!([])),
    ] {
        if doc.get(key).is_none() {
            doc[key] = default;
        }
    }
    doc
}

fn save(doc: &Value) -> Result<(), String> {
    localspace::harness::host::doc_put(&doc.to_string())
}

fn next_id(doc: &Value, prefix: &str) -> String {
    let n = doc["shapes"].as_array().map(|a| a.len()).unwrap_or(0)
        + doc["frames"].as_array().map(|a| a.len()).unwrap_or(0)
        + 1;
    format!(
        "{prefix}{n}_{}",
        localspace::harness::host::now_ms() % 100_000
    )
}

fn ok(result: Value, summary: &str) -> String {
    json!({"ok": true, "result": result, "diff-summary": summary}).to_string()
}

fn fail(message: impl Into<String>) -> String {
    json!({"ok": false, "error": message.into()}).to_string()
}

fn shape_index(doc: &Value, id: &str) -> Option<usize> {
    doc["shapes"]
        .as_array()?
        .iter()
        .position(|s| s["id"].as_str() == Some(id))
}

/// The `ids` array these tools take.
///
/// Only `ids`, never a bare `id`: the declared schema marks it required, so Core
/// rejects a call without it before this code runs and the grammar built from
/// that schema will not let a model emit one. An alias here would be unreachable.
fn ids_of(params: &Value) -> Vec<String> {
    params
        .get("ids")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// The next free stacking index. Shapes are drawn in `z` order.
fn next_z(doc: &Value) -> i64 {
    doc["shapes"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s["z"].as_i64()).max().unwrap_or(0) + 1)
        .unwrap_or(1)
}

/// Bounding box of a shape, or `None` for a connector, which has no box of its own.
fn bounds(sh: &Value) -> Option<(f64, f64, f64, f64)> {
    if sh["kind"].as_str() == Some("arrow") {
        return None;
    }
    Some((
        sh["x"].as_f64().unwrap_or(0.0),
        sh["y"].as_f64().unwrap_or(0.0),
        sh["w"].as_f64().unwrap_or(0.0),
        sh["h"].as_f64().unwrap_or(0.0),
    ))
}

fn locked(sh: &Value) -> bool {
    sh["locked"].as_bool().unwrap_or(false)
}

/// Apply `edit` to every named, unlocked shape. Returns how many changed and
/// the first id that was refused because it is locked.
fn edit_each(
    doc: &mut Value,
    ids: &[String],
    mut edit: impl FnMut(&mut Value),
) -> Result<usize, String> {
    let mut changed = 0;
    for id in ids {
        let Some(i) = shape_index(doc, id) else {
            return Err(format!("no shape `{id}`"));
        };
        let list = doc["shapes"].as_array_mut().unwrap();
        if locked(&list[i]) {
            return Err(format!("`{id}` is locked; unlock it first"));
        }
        edit(&mut list[i]);
        changed += 1;
    }
    Ok(changed)
}

fn f(params: &Value, key: &str, default: f64) -> f64 {
    params.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

fn s<'a>(params: &'a Value, key: &str, default: &'a str) -> &'a str {
    params.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

const TOOLS: &str = include_str!("../../whiteboard/tools.json");

impl Guest for Whiteboard {
    fn tools() -> String {
        TOOLS.to_string()
    }

    fn call(name: String, params_json: String) -> String {
        let params: Value = serde_json::from_str(&params_json).unwrap_or_else(|_| json!({}));
        let mut doc = load();

        match name.as_str() {
            "canvas.list" => {
                let frames: Vec<Value> = doc["frames"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|fr| json!({"id": fr["id"], "name": fr["name"]}))
                    .collect();
                let shapes: Vec<Value> = doc["shapes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|sh| {
                        json!({
                            "id": sh["id"], "kind": sh["kind"], "text": sh["text"],
                            "fill": sh["fill"], "frame": sh["frame"]
                        })
                    })
                    .collect();
                ok(
                    json!({"title": doc["title"], "frames": frames, "shapes": shapes}),
                    &format!("{} frame(s), {} shape(s)", frames.len(), shapes.len()),
                )
            }

            "canvas.add_shape" | "canvas.add_sticky" => {
                let kind = if name.ends_with("sticky") {
                    "sticky"
                } else {
                    s(&params, "kind", "rect")
                };
                if !["rect", "ellipse", "arrow", "sticky"].contains(&kind) {
                    return fail(format!("`{kind}` is not a shape kind"));
                }
                let id = next_id(&doc, "s");
                let count = doc["shapes"].as_array().map(|a| a.len()).unwrap_or(0);
                // Lay stickies out in a row so a model does not have to do geometry.
                let default_x = 40.0 + (count % 6) as f64 * 150.0;
                let default_y = 40.0 + (count / 6) as f64 * 130.0;
                let shape = json!({
                    "id": id,
                    "kind": kind,
                    "x": f(&params, "x", default_x),
                    "y": f(&params, "y", default_y),
                    "w": f(&params, "w", if kind == "sticky" { 130.0 } else { 160.0 }),
                    "h": f(&params, "h", if kind == "sticky" { 110.0 } else { 90.0 }),
                    "fill": s(&params, "fill", if kind == "sticky" { "yellow" } else { "grey" }),
                    "text": s(&params, "text", ""),
                    "frame": params.get("frame").cloned().unwrap_or(Value::Null),
                    "z": next_z(&doc),
                    "locked": false,
                });
                doc["shapes"].as_array_mut().unwrap().push(shape);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), &format!("added 1 {kind}"))
            }

            "canvas.move" => {
                let id = s(&params, "id", "");
                let Some(i) = shape_index(&doc, id) else {
                    return fail(format!("no shape `{id}`"));
                };
                if locked(&doc["shapes"].as_array().unwrap()[i]) {
                    return fail(format!("`{id}` is locked; unlock it first"));
                }
                let shapes = doc["shapes"].as_array_mut().unwrap();
                shapes[i]["x"] = json!(f(&params, "x", shapes[i]["x"].as_f64().unwrap_or(0.0)));
                shapes[i]["y"] = json!(f(&params, "y", shapes[i]["y"].as_f64().unwrap_or(0.0)));
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "moved 1 shape")
            }

            "canvas.resize" => {
                let id = s(&params, "id", "");
                let Some(i) = shape_index(&doc, id) else {
                    return fail(format!("no shape `{id}`"));
                };
                if locked(&doc["shapes"].as_array().unwrap()[i]) {
                    return fail(format!("`{id}` is locked; unlock it first"));
                }
                let shapes = doc["shapes"].as_array_mut().unwrap();
                shapes[i]["w"] = json!(f(&params, "w", 160.0).max(8.0));
                shapes[i]["h"] = json!(f(&params, "h", 90.0).max(8.0));
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "resized 1 shape")
            }

            "canvas.set_text" => {
                let id = s(&params, "id", "");
                let Some(i) = shape_index(&doc, id) else {
                    return fail(format!("no shape `{id}`"));
                };
                let text = s(&params, "text", "").to_string();
                doc["shapes"].as_array_mut().unwrap()[i]["text"] = json!(text);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "relabelled 1 shape")
            }

            "canvas.set_fill" => {
                let id = s(&params, "id", "");
                let fill = s(&params, "fill", "grey").to_string();
                if !["red", "amber", "green", "blue", "yellow", "grey"].contains(&fill.as_str()) {
                    return fail(format!(
                        "`{fill}` is not one of red, amber, green, blue, yellow, grey"
                    ));
                }
                let Some(i) = shape_index(&doc, id) else {
                    return fail(format!("no shape `{id}`"));
                };
                doc["shapes"].as_array_mut().unwrap()[i]["fill"] = json!(fill);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "recoloured 1 shape")
            }

            "canvas.delete" => {
                let id = s(&params, "id", "");
                let Some(i) = shape_index(&doc, id) else {
                    return fail(format!("no shape `{id}`"));
                };
                if locked(&doc["shapes"].as_array().unwrap()[i]) {
                    return fail(format!("`{id}` is locked; unlock it first"));
                }
                doc["shapes"].as_array_mut().unwrap().remove(i);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "removed 1 shape")
            }

            "canvas.connect" => {
                let from = s(&params, "from", "").to_string();
                let to = s(&params, "to", "").to_string();
                if shape_index(&doc, &from).is_none() {
                    return fail(format!("no shape `{from}`"));
                }
                if shape_index(&doc, &to).is_none() {
                    return fail(format!("no shape `{to}`"));
                }
                let id = next_id(&doc, "a");
                let arrow = json!({
                    "id": id, "kind": "arrow", "from": from, "to": to,
                    "text": s(&params, "label", ""), "fill": "grey",
                    "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0,
                    "frame": Value::Null,
                    "z": next_z(&doc),
                    "locked": false,
                });
                doc["shapes"].as_array_mut().unwrap().push(arrow);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "connected 2 shapes")
            }

            "canvas.add_frame" => {
                let id = next_id(&doc, "f");
                let count = doc["frames"].as_array().map(|a| a.len()).unwrap_or(0);
                let frame = json!({
                    "id": id,
                    "name": s(&params, "name", "Frame"),
                    "x": f(&params, "x", 20.0 + count as f64 * 640.0),
                    "y": f(&params, "y", 20.0),
                    "w": f(&params, "w", 600.0),
                    "h": f(&params, "h", 420.0),
                });
                doc["frames"].as_array_mut().unwrap().push(frame);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "added 1 frame")
            }

            "canvas.set_title" => {
                let title = s(&params, "title", "Board").to_string();
                doc["title"] = json!(title);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"title": doc["title"]}), "retitled the board")
            }

            "canvas.select" => {
                let ids: Vec<Value> = params
                    .get("ids")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for id in &ids {
                    if let Some(id) = id.as_str() {
                        if shape_index(&doc, id).is_none() {
                            return fail(format!("no shape `{id}`"));
                        }
                    }
                }
                let n = ids.len();
                doc["selection"] = Value::Array(ids);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"selected": n}), &format!("selected {n} shape(s)"))
            }

            // The companion to the context provider: full detail for one region,
            // so the provider can stay a summary.
            "canvas.zoom" => {
                let frame = params.get("frame").and_then(|v| v.as_str());
                let shapes: Vec<Value> = doc["shapes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|sh| match frame {
                        Some(f) => sh["frame"].as_str() == Some(f),
                        None => true,
                    })
                    .collect();
                let n = shapes.len();
                ok(
                    json!({"shapes": shapes}),
                    &format!("{n} shape(s) in full detail"),
                )
            }

            "canvas.add_text" => {
                let text = s(&params, "text", "").to_string();
                if text.trim().is_empty() {
                    return fail("a text label needs some text");
                }
                let id = next_id(&doc, "t");
                let size = f(&params, "size", 16.0).clamp(8.0, 96.0);
                let label = json!({
                    "id": id,
                    "kind": "text",
                    "x": f(&params, "x", 40.0),
                    "y": f(&params, "y", 40.0),
                    "w": (text.chars().count() as f64 * size * 0.55).max(60.0),
                    "h": size * 1.6,
                    "size": size,
                    "fill": "none",
                    "text": text,
                    "frame": Value::Null,
                    "z": next_z(&doc),
                    "locked": false,
                });
                doc["shapes"].as_array_mut().unwrap().push(label);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "added 1 text label")
            }

            "canvas.duplicate" => {
                let ids = ids_of(&params);
                if ids.is_empty() {
                    return fail("nothing to duplicate");
                }
                let mut made = Vec::new();
                for id in &ids {
                    let Some(i) = shape_index(&doc, id) else {
                        return fail(format!("no shape `{id}`"));
                    };
                    let mut copy = doc["shapes"].as_array().unwrap()[i].clone();
                    // A connector's endpoints would still point at the originals,
                    // so copying one would draw a second line over the first.
                    if copy["kind"].as_str() == Some("arrow") {
                        continue;
                    }
                    let new_id = format!("{}_{}", id, made.len() + 1);
                    copy["id"] = json!(new_id);
                    copy["x"] = json!(copy["x"].as_f64().unwrap_or(0.0) + 24.0);
                    copy["y"] = json!(copy["y"].as_f64().unwrap_or(0.0) + 24.0);
                    copy["z"] = json!(next_z(&doc) + made.len() as i64);
                    copy["locked"] = json!(false);
                    made.push(new_id);
                    doc["shapes"].as_array_mut().unwrap().push(copy);
                }
                if made.is_empty() {
                    return fail(
                        "nothing was duplicated (connectors cannot be copied on their own)",
                    );
                }
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                let n = made.len();
                doc["selection"] = json!(made.clone());
                let _ = save(&doc);
                ok(json!({"ids": made}), &format!("duplicated {n} shape(s)"))
            }

            "canvas.order" => {
                let ids = ids_of(&params);
                let to = s(&params, "to", "front").to_string();
                if ids.is_empty() {
                    return fail("nothing to reorder");
                }
                let top = next_z(&doc);
                let bottom = doc["shapes"]
                    .as_array()
                    .and_then(|a| a.iter().filter_map(|s| s["z"].as_i64()).min())
                    .unwrap_or(0);
                let result = edit_each(&mut doc, &ids, |sh| {
                    let z = sh["z"].as_i64().unwrap_or(0);
                    sh["z"] = json!(match to.as_str() {
                        "front" => top,
                        "back" => bottom - 1,
                        "forward" => z + 1,
                        _ => z - 1,
                    });
                });
                match result {
                    Ok(n) => {
                        if let Err(e) = save(&doc) {
                            return fail(e);
                        }
                        ok(json!({"ids": ids}), &format!("moved {n} shape(s) {to}"))
                    }
                    Err(e) => fail(e),
                }
            }

            "canvas.lock" => {
                let ids = ids_of(&params);
                let want = params
                    .get("locked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                if ids.is_empty() {
                    return fail("nothing to lock");
                }
                // Locking is the one edit a locked shape still accepts, or it
                // could never be unlocked.
                for id in &ids {
                    let Some(i) = shape_index(&doc, id) else {
                        return fail(format!("no shape `{id}`"));
                    };
                    doc["shapes"].as_array_mut().unwrap()[i]["locked"] = json!(want);
                }
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(
                    json!({"ids": ids}),
                    &format!(
                        "{} {} shape(s)",
                        if want { "locked" } else { "unlocked" },
                        ids.len()
                    ),
                )
            }

            "canvas.align" => {
                let ids = ids_of(&params);
                let edge = s(&params, "edge", "left").to_string();
                if ids.len() < 2 {
                    return fail("aligning needs at least two shapes");
                }
                let boxes: Vec<(f64, f64, f64, f64)> = ids
                    .iter()
                    .filter_map(|id| shape_index(&doc, id))
                    .filter_map(|i| bounds(&doc["shapes"].as_array().unwrap()[i]))
                    .collect();
                if boxes.len() < 2 {
                    return fail("aligning needs at least two shapes that have a box");
                }
                let left = boxes.iter().map(|b| b.0).fold(f64::MAX, f64::min);
                let right = boxes.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
                let top = boxes.iter().map(|b| b.1).fold(f64::MAX, f64::min);
                let bottom = boxes.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);

                let result = edit_each(&mut doc, &ids, |sh| {
                    let Some((x, y, w, h)) = bounds(sh) else {
                        return;
                    };
                    match edge.as_str() {
                        "left" => sh["x"] = json!(left),
                        "right" => sh["x"] = json!(right - w),
                        "top" => sh["y"] = json!(top),
                        "bottom" => sh["y"] = json!(bottom - h),
                        "centre_x" => sh["x"] = json!((left + right) / 2.0 - w / 2.0),
                        "centre_y" => sh["y"] = json!((top + bottom) / 2.0 - h / 2.0),
                        _ => {
                            let _ = (x, y);
                        }
                    }
                });
                match result {
                    Ok(n) => {
                        if let Err(e) = save(&doc) {
                            return fail(e);
                        }
                        ok(json!({"ids": ids}), &format!("aligned {n} shape(s) {edge}"))
                    }
                    Err(e) => fail(e),
                }
            }

            "canvas.distribute" => {
                let ids = ids_of(&params);
                let axis = s(&params, "axis", "x").to_string();
                if ids.len() < 3 {
                    return fail("distributing needs at least three shapes");
                }
                // Sort by current position, then space the gaps evenly between the
                // two outermost, which stay where they are.
                let mut placed: Vec<(String, f64, f64)> = Vec::new();
                for id in &ids {
                    let Some(i) = shape_index(&doc, id) else {
                        return fail(format!("no shape `{id}`"));
                    };
                    let sh = &doc["shapes"].as_array().unwrap()[i];
                    let Some((x, y, w, h)) = bounds(sh) else {
                        continue;
                    };
                    if axis == "x" {
                        placed.push((id.clone(), x, w));
                    } else {
                        placed.push((id.clone(), y, h));
                    }
                }
                if placed.len() < 3 {
                    return fail("distributing needs at least three shapes that have a box");
                }
                placed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                let first = placed.first().unwrap().clone();
                let last = placed.last().unwrap().clone();
                let span = (last.1 + last.2) - first.1;
                let total: f64 = placed.iter().map(|p| p.2).sum();
                let gap = (span - total) / (placed.len() - 1) as f64;

                let mut cursor = first.1;
                let mut moved = 0;
                for (id, _, size) in &placed {
                    let Some(i) = shape_index(&doc, id) else {
                        continue;
                    };
                    let list = doc["shapes"].as_array_mut().unwrap();
                    if !locked(&list[i]) {
                        list[i][if axis == "x" { "x" } else { "y" }] = json!(cursor.round());
                        moved += 1;
                    }
                    cursor += size + gap;
                }
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(
                    json!({"ids": ids}),
                    &format!("spaced {moved} shape(s) evenly along {axis}"),
                )
            }

            "canvas.export_outline" => {
                // outline.v1: `{title, items: [{id, text, kind, fill, frame}]}`.
                // Written into the document so the artifact Core registers is a
                // DAG reference to a version that contains it — not a copy.
                let frame = params
                    .get("frame")
                    .and_then(|f| f.as_str())
                    .map(|s| s.to_string());
                let items: Vec<Value> = doc["shapes"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|sh| {
                        sh["kind"].as_str() != Some("arrow") && sh["kind"].as_str() != Some("ink")
                    })
                    .filter(|sh| !sh["text"].as_str().unwrap_or("").trim().is_empty())
                    .filter(|sh| match &frame {
                        Some(f) => sh["frame"].as_str() == Some(f),
                        None => true,
                    })
                    .map(|sh| {
                        json!({
                            "id": sh["id"],
                            "text": sh["text"],
                            "kind": sh["kind"],
                            "fill": sh["fill"],
                            "frame": sh["frame"],
                        })
                    })
                    .collect();
                if items.is_empty() {
                    return fail("nothing on the board has text to export");
                }
                let n = items.len();
                let title = doc["title"].as_str().unwrap_or("Board").to_string();
                doc["outline"] = json!({
                    "kind": "outline.v1",
                    "title": title,
                    "items": items,
                });
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                let summary = format!("{n} item(s) from board \"{title}\"");
                ok(
                    json!({
                        "items": n,
                        "artifact": {"kind": "outline.v1", "summary": summary},
                    }),
                    &format!("exported {n} item(s) as an outline"),
                )
            }

            other => fail(format!("`{other}` is not a whiteboard tool")),
        }
    }

    /// Text serialization of the board, sized to the budget.
    ///
    /// Outline of frames, then shapes with their text and rough coordinates, with
    /// full detail for the selection. Never the whole document.
    fn context(budget_tokens: u32, focused: bool) -> String {
        let doc = load();
        let budget = budget_tokens as usize;
        let mut out = String::new();

        let frames = doc["frames"].as_array().cloned().unwrap_or_default();
        let shapes = doc["shapes"].as_array().cloned().unwrap_or_default();
        let selection: Vec<&str> = doc["selection"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        out.push_str(&format!(
            "board \"{}\": {} frame(s), {} shape(s)\n",
            doc["title"].as_str().unwrap_or("Board"),
            frames.len(),
            shapes.len()
        ));

        // Unfocused: the headline only, so a pinned harness costs almost nothing.
        if !focused {
            return json!({"text": out, "expandable": true}).to_string();
        }

        for fr in &frames {
            let name = fr["name"].as_str().unwrap_or("frame");
            let id = fr["id"].as_str().unwrap_or("");
            let inside: Vec<&Value> = shapes
                .iter()
                .filter(|sh| sh["frame"].as_str() == Some(id))
                .collect();
            out.push_str(&format!(
                "frame {id} \"{name}\" ({} shapes)\n",
                inside.len()
            ));
            for sh in inside {
                out.push_str(&line(sh, false));
                if estimate(&out) > budget {
                    break;
                }
            }
        }

        let loose: Vec<&Value> = shapes
            .iter()
            .filter(|sh| sh["frame"].as_str().is_none())
            .collect();
        if !loose.is_empty() {
            out.push_str(&format!("outside any frame ({}):\n", loose.len()));
            for sh in loose {
                if estimate(&out) > budget {
                    out.push_str("  … (truncated; call canvas.zoom for the rest)\n");
                    break;
                }
                out.push_str(&line(sh, false));
            }
        }

        if !selection.is_empty() {
            out.push_str("selection, in full:\n");
            for id in &selection {
                if let Some(sh) = shapes.iter().find(|s| s["id"].as_str() == Some(*id)) {
                    out.push_str(&line(sh, true));
                }
            }
        }

        json!({"text": out, "expandable": true}).to_string()
    }

    /// The `settings` view, rendered by the Client in the host theme.
    fn view(view_id: String) -> String {
        if view_id != "settings" {
            return json!({"w": "text", "text": format!("no view `{view_id}`")}).to_string();
        }
        let doc = load();
        let shapes = doc["shapes"].as_array().cloned().unwrap_or_default();
        let mut counts = std::collections::BTreeMap::new();
        for sh in &shapes {
            *counts
                .entry(sh["kind"].as_str().unwrap_or("?").to_string())
                .or_insert(0usize) += 1;
        }

        json!({
            "w": "column",
            "children": [
                {"w": "heading", "text": doc["title"]},
                {"w": "input", "id": "title", "label": "Title",
                 "value": doc["title"].as_str().unwrap_or("Board"), "multiline": false},
                {"w": "separator"},
                {"w": "text", "text": "Contents", "strong": true},
                {"w": "table",
                 "headers": ["kind", "count"],
                 "rows": counts.iter().map(|(k, v)| vec![k.clone(), v.to_string()])
                          .collect::<Vec<_>>()},
                {"w": "separator"},
                {"w": "row", "children": [
                    {"w": "badge", "text": format!("{} shapes", shapes.len()), "tone": "neutral"},
                    {"w": "badge",
                     "text": format!("{} frames", doc["frames"].as_array().map(|a| a.len()).unwrap_or(0)),
                     "tone": "neutral"}
                ]},
                {"w": "button", "id": "clear_selection", "label": "Clear selection", "enabled": true}
            ]
        })
        .to_string()
    }

    /// Opaque commands from the surface. The document carries state; messages
    /// carry only commands, which is what keeps this correct over a WAN.
    fn event(_view_id: String, payload: Vec<u8>) -> Vec<u8> {
        let Ok(text) = String::from_utf8(payload) else {
            return Vec::new();
        };
        let Ok(msg): Result<Value, _> = serde_json::from_str(&text) else {
            return Vec::new();
        };

        // The Client forwards a surface's document edits as {"doc": {...}}.
        if let Some(doc) = msg.get("doc") {
            let _ = save(doc);
            return json!({"applied": true}).to_string().into_bytes();
        }

        // Widget events arrive with an id and a value.
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            let mut doc = load();
            match id {
                "title" => {
                    if let Some(Value::String(t)) = msg.get("value").and_then(|v| v.get("Text")) {
                        doc["title"] = json!(t);
                        let _ = save(&doc);
                    }
                }
                "clear_selection" => {
                    doc["selection"] = json!([]);
                    let _ = save(&doc);
                }
                _ => {}
            }
        }
        Vec::new()
    }
}

fn line(sh: &Value, full: bool) -> String {
    let id = sh["id"].as_str().unwrap_or("?");
    let kind = sh["kind"].as_str().unwrap_or("?");
    let text = sh["text"].as_str().unwrap_or("");
    let fill = sh["fill"].as_str().unwrap_or("grey");
    if kind == "arrow" {
        return format!(
            "  {id} arrow {} -> {} \"{text}\"\n",
            sh["from"].as_str().unwrap_or("?"),
            sh["to"].as_str().unwrap_or("?")
        );
    }
    if full {
        format!(
            "  {id} {kind} {fill} at ({:.0},{:.0}) {:.0}x{:.0} \"{text}\"\n",
            sh["x"].as_f64().unwrap_or(0.0),
            sh["y"].as_f64().unwrap_or(0.0),
            sh["w"].as_f64().unwrap_or(0.0),
            sh["h"].as_f64().unwrap_or(0.0),
        )
    } else {
        format!(
            "  {id} {kind} {fill} at ({:.0},{:.0}) \"{text}\"\n",
            sh["x"].as_f64().unwrap_or(0.0),
            sh["y"].as_f64().unwrap_or(0.0),
        )
    }
}

fn estimate(s: &str) -> usize {
    s.len().div_ceil(4)
}
