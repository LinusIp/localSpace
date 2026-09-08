//! Reference harness #2 — planning board logic (Tier A, `doc = "crdt"`).
//!
//! Deliberately a `widgets` surface rather than an `egui` one: most harnesses do
//! not need a custom canvas, and this is what that path looks like end to end.
//! The Client renders the tree in the host theme, so the board looks native
//! without the harness shipping a single pixel.

wit_bindgen::generate!({
    path: "../../wit",
    world: "harness",
});

use serde_json::{json, Value};

struct Planner;

export!(Planner);

const TOOLS: &str = include_str!("../../../registry/planner/tools.json");

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

fn load() -> Value {
    let raw = localspace::harness::host::doc_get();
    let mut doc: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    if !doc.is_object() {
        doc = json!({});
    }
    if doc.get("columns").is_none() {
        doc["columns"] = json!([
            {"name": "Backlog", "wip": 0},
            {"name": "In progress", "wip": 3},
            {"name": "Done", "wip": 0}
        ]);
    }
    if doc.get("cards").is_none() {
        doc["cards"] = json!([]);
    }
    if doc.get("title").is_none() {
        doc["title"] = json!("Planning board");
    }
    doc
}

fn save(doc: &Value) -> Result<(), String> {
    localspace::harness::host::doc_put(&doc.to_string())
}

fn ok(result: Value, summary: &str) -> String {
    json!({"ok": true, "result": result, "diff-summary": summary}).to_string()
}

fn fail(message: impl Into<String>) -> String {
    json!({"ok": false, "error": message.into()}).to_string()
}

fn columns(doc: &Value) -> Vec<String> {
    doc["columns"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn wip_of(doc: &Value, column: &str) -> u64 {
    doc["columns"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["name"].as_str() == Some(column)))
        .and_then(|c| c["wip"].as_u64())
        .unwrap_or(0)
}

fn count_in(doc: &Value, column: &str) -> usize {
    doc["cards"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|c| c["column"].as_str() == Some(column))
                .count()
        })
        .unwrap_or(0)
}

fn card_index(doc: &Value, id: &str) -> Option<usize> {
    doc["cards"]
        .as_array()?
        .iter()
        .position(|c| c["id"].as_str() == Some(id))
}

fn s<'a>(params: &'a Value, key: &str, default: &'a str) -> &'a str {
    params.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

/// Cards that would sit above their column's WIP limit. The provider reports
/// this because it is the single thing a planning board exists to tell you.
fn over_limit(doc: &Value) -> Vec<(String, usize, u64)> {
    doc["columns"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let name = c["name"].as_str()?.to_string();
                    let limit = c["wip"].as_u64().unwrap_or(0);
                    let count = count_in(doc, &name);
                    if limit > 0 && count as u64 > limit {
                        Some((name, count, limit))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Guest for Planner {
    fn tools() -> String {
        TOOLS.to_string()
    }

    fn call(name: String, params_json: String) -> String {
        let params: Value = serde_json::from_str(&params_json).unwrap_or_else(|_| json!({}));
        let mut doc = load();

        match name.as_str() {
            "board.columns" => {
                let cols: Vec<Value> = columns(&doc)
                    .iter()
                    .map(|name| {
                        json!({
                            "name": name,
                            "cards": count_in(&doc, name),
                            "wip": wip_of(&doc, name),
                        })
                    })
                    .collect();
                let n = cols.len();
                ok(json!({"columns": cols}), &format!("{n} column(s)"))
            }

            "board.add_card" => {
                let text = s(&params, "text", "").to_string();
                if text.trim().is_empty() {
                    return fail("a card needs some text");
                }
                let all = columns(&doc);
                let column = match params.get("column").and_then(|c| c.as_str()) {
                    Some(c) if all.iter().any(|x| x == c) => c.to_string(),
                    Some(c) => return fail(format!("no column `{c}`; this board has {}", all.join(", "))),
                    None => all.first().cloned().unwrap_or_else(|| "Backlog".into()),
                };
                let id = format!(
                    "c{}",
                    doc["cards"].as_array().map(|a| a.len()).unwrap_or(0) + 1
                );
                let card = json!({
                    "id": id,
                    "text": text,
                    "column": column,
                    "assignee": params.get("assignee").cloned().unwrap_or(Value::Null),
                });
                doc["cards"].as_array_mut().unwrap().push(card);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }

                // A write that pushes a column over its limit says so in the one
                // line the model sees, rather than leaving it to notice later.
                let limit = wip_of(&doc, &column);
                let count = count_in(&doc, &column);
                let summary = if limit > 0 && count as u64 > limit {
                    format!("added 1 card; `{column}` is now {count} over a limit of {limit}")
                } else {
                    format!("added 1 card to `{column}`")
                };
                ok(json!({"id": id, "column": column}), &summary)
            }

            "board.move_card" => {
                let id = s(&params, "id", "");
                let column = s(&params, "column", "").to_string();
                if !columns(&doc).iter().any(|x| *x == column) {
                    return fail(format!("no column `{column}`"));
                }
                let Some(i) = card_index(&doc, id) else {
                    return fail(format!("no card `{id}`"));
                };
                doc["cards"].as_array_mut().unwrap()[i]["column"] = json!(column);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                let count = count_in(&doc, &column);
                let limit = wip_of(&doc, &column);
                let summary = if limit > 0 && count as u64 > limit {
                    format!("moved 1 card; `{column}` is now {count} over a limit of {limit}")
                } else {
                    format!("moved 1 card to `{column}`")
                };
                ok(json!({"id": id}), &summary)
            }

            "board.assign" => {
                let id = s(&params, "id", "");
                let Some(i) = card_index(&doc, id) else {
                    return fail(format!("no card `{id}`"));
                };
                let who = params.get("assignee").cloned().unwrap_or(Value::Null);
                let cleared = who.is_null();
                doc["cards"].as_array_mut().unwrap()[i]["assignee"] = who;
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(
                    json!({"id": id}),
                    if cleared {
                        "unassigned 1 card"
                    } else {
                        "assigned 1 card"
                    },
                )
            }

            "board.set_text" => {
                let id = s(&params, "id", "");
                let Some(i) = card_index(&doc, id) else {
                    return fail(format!("no card `{id}`"));
                };
                let text = s(&params, "text", "").to_string();
                doc["cards"].as_array_mut().unwrap()[i]["text"] = json!(text);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "rewrote 1 card")
            }

            "board.add_column" => {
                let name = s(&params, "name", "").to_string();
                if name.trim().is_empty() {
                    return fail("a column needs a name");
                }
                if columns(&doc).iter().any(|c| *c == name) {
                    return fail(format!("`{name}` is already a column"));
                }
                let wip = params.get("wip").and_then(|w| w.as_u64()).unwrap_or(0);
                doc["columns"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": name, "wip": wip}));
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"name": name}), "added 1 column")
            }

            "board.set_wip" => {
                let column = s(&params, "column", "").to_string();
                let wip = params.get("wip").and_then(|w| w.as_u64()).unwrap_or(0);
                let Some(list) = doc["columns"].as_array_mut() else {
                    return fail("this board has no columns");
                };
                let Some(c) = list.iter_mut().find(|c| c["name"].as_str() == Some(&column)) else {
                    return fail(format!("no column `{column}`"));
                };
                c["wip"] = json!(wip);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(
                    json!({"column": column, "wip": wip}),
                    &if wip == 0 {
                        format!("removed the limit on `{column}`")
                    } else {
                        format!("limit on `{column}` is now {wip}")
                    },
                )
            }

            "board.delete_card" => {
                let id = s(&params, "id", "");
                let Some(i) = card_index(&doc, id) else {
                    return fail(format!("no card `{id}`"));
                };
                doc["cards"].as_array_mut().unwrap().remove(i);
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(json!({"id": id}), "removed 1 card")
            }

            "board.zoom" => {
                let column = params.get("column").and_then(|c| c.as_str());
                let cards: Vec<Value> = doc["cards"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| match column {
                        Some(name) => c["column"].as_str() == Some(name),
                        None => true,
                    })
                    .collect();
                let n = cards.len();
                ok(json!({"cards": cards}), &format!("{n} card(s) in full"))
            }

            "board.import_outline" => {
                // The handoff (spec §18.2): Core has resolved the artifact, checked
                // this harness accepts outline.v1, and pinned the producer's
                // document at the commit the artifact names. This reads that.
                let id = s(&params, "artifact", "").to_string();
                let raw = match localspace::harness::host::artifact_get(&id) {
                    Ok(r) => r,
                    Err(e) => return fail(e),
                };
                let art: Value = match serde_json::from_str(&raw) {
                    Ok(v) => v,
                    Err(e) => return fail(format!("artifact payload was not JSON: {e}")),
                };
                if art["kind"].as_str() != Some("outline.v1") {
                    return fail(format!(
                        "`{id}` is {}, not outline.v1",
                        art["kind"].as_str().unwrap_or("untyped")
                    ));
                }
                let content = &art["content"];
                // The outline the producer wrote, or — for a producer that did not
                // write one — anything with text, so a plain board still imports.
                let items: Vec<Value> = content["outline"]["items"]
                    .as_array()
                    .cloned()
                    .or_else(|| content["shapes"].as_array().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|it| !it["text"].as_str().unwrap_or("").trim().is_empty())
                    .collect();
                if items.is_empty() {
                    return fail(format!("`{id}` has no items with text to import"));
                }

                let all = columns(&doc);
                let column = match params.get("column").and_then(|c| c.as_str()) {
                    Some(c) if all.iter().any(|x| x == c) => c.to_string(),
                    Some(c) => return fail(format!("no column `{c}`; this board has {}", all.join(", "))),
                    None => all.first().cloned().unwrap_or_else(|| "Backlog".into()),
                };

                let mut made = 0usize;
                for it in &items {
                    let n = doc["cards"].as_array().map(|a| a.len()).unwrap_or(0) + 1;
                    doc["cards"].as_array_mut().unwrap().push(json!({
                        "id": format!("c{n}"),
                        "text": it["text"],
                        "column": column,
                        "assignee": Value::Null,
                        "source": {"artifact": id, "item": it["id"]},
                    }));
                    made += 1;
                }
                if let Err(e) = save(&doc) {
                    return fail(e);
                }
                ok(
                    json!({"cards": made, "column": column}),
                    &format!("imported {made} card(s) from {id} into `{column}`"),
                )
            }

            other => fail(format!("`{other}` is not a planning-board tool")),
        }
    }

    /// Columns, card titles, assignees and WIP counts — exactly what the spec
    /// asks a planning board's provider to return.
    fn context(budget_tokens: u32, focused: bool) -> String {
        let doc = load();
        let budget = budget_tokens as usize;
        let cards = doc["cards"].as_array().cloned().unwrap_or_default();
        let mut out = String::new();

        out.push_str(&format!(
            "board \"{}\": {} column(s), {} card(s)\n",
            doc["title"].as_str().unwrap_or("Planning board"),
            columns(&doc).len(),
            cards.len()
        ));

        // The one thing worth saying even when unfocused.
        let over = over_limit(&doc);
        if !over.is_empty() {
            let lines: Vec<String> = over
                .iter()
                .map(|(n, c, l)| format!("{n} {c}/{l}"))
                .collect();
            out.push_str(&format!("over WIP limit: {}\n", lines.join(", ")));
        }

        if !focused {
            return json!({"text": out, "expandable": true}).to_string();
        }

        for name in columns(&doc) {
            let limit = wip_of(&doc, &name);
            let count = count_in(&doc, &name);
            out.push_str(&format!(
                "{name} ({count}{})\n",
                if limit > 0 {
                    format!("/{limit}")
                } else {
                    String::new()
                }
            ));
            for card in cards.iter().filter(|c| c["column"].as_str() == Some(&name)) {
                if estimate(&out) > budget {
                    out.push_str("  … (truncated; call board.zoom for one column in full)\n");
                    return json!({"text": out, "expandable": true}).to_string();
                }
                let who = card["assignee"].as_str().unwrap_or("");
                out.push_str(&format!(
                    "  {} \"{}\"{}\n",
                    card["id"].as_str().unwrap_or("?"),
                    card["text"].as_str().unwrap_or(""),
                    if who.is_empty() {
                        String::new()
                    } else {
                        format!(" — {who}")
                    }
                ));
            }
        }

        json!({"text": out, "expandable": true}).to_string()
    }

    fn view(view_id: String) -> String {
        if view_id != "board" {
            return json!({"w": "text", "text": format!("no view `{view_id}`")}).to_string();
        }
        let doc = load();
        let cards = doc["cards"].as_array().cloned().unwrap_or_default();
        let over = over_limit(&doc);

        let mut children = vec![
            json!({"w": "heading", "text": doc["title"].as_str().unwrap_or("Planning board")}),
        ];

        if over.is_empty() {
            children.push(json!({
                "w": "row",
                "children": [{"w": "badge", "text": "every column within its limit", "tone": "good"}]
            }));
        } else {
            let badges: Vec<Value> = over
                .iter()
                .map(|(n, c, l)| json!({"w": "badge", "text": format!("{n}: {c}/{l}"), "tone": "warn"}))
                .collect();
            children.push(json!({"w": "row", "children": badges}));
        }

        children.push(json!({"w": "separator"}));
        children.push(json!({
            "w": "table",
            "headers": ["column", "cards", "wip limit"],
            "rows": columns(&doc).iter().map(|name| {
                vec![
                    name.clone(),
                    count_in(&doc, name).to_string(),
                    match wip_of(&doc, name) { 0 => "—".to_string(), n => n.to_string() },
                ]
            }).collect::<Vec<_>>()
        }));

        for name in columns(&doc) {
            let in_column: Vec<String> = cards
                .iter()
                .filter(|c| c["column"].as_str() == Some(&name))
                .map(|c| {
                    let who = c["assignee"].as_str().unwrap_or("");
                    if who.is_empty() {
                        c["text"].as_str().unwrap_or("").to_string()
                    } else {
                        format!("{} — {who}", c["text"].as_str().unwrap_or(""))
                    }
                })
                .collect();
            children.push(json!({"w": "separator"}));
            children.push(json!({"w": "text", "text": name, "strong": true}));
            if in_column.is_empty() {
                children.push(json!({"w": "text", "text": "nothing here", "muted": true}));
            } else {
                children.push(json!({"w": "list", "items": in_column}));
            }
        }

        json!({"w": "column", "children": children}).to_string()
    }

    fn event(_view_id: String, payload: Vec<u8>) -> Vec<u8> {
        let Ok(text) = String::from_utf8(payload) else {
            return Vec::new();
        };
        let Ok(msg): Result<Value, _> = serde_json::from_str(&text) else {
            return Vec::new();
        };
        if let Some(doc) = msg.get("doc") {
            let _ = save(doc);
            return json!({"applied": true}).to_string().into_bytes();
        }
        Vec::new()
    }
}

fn estimate(s: &str) -> usize {
    s.len().div_ceil(4)
}
