//! Per-corpus web UI settings.
//!
//! The frontend keeps a small document — pane layout, camera, wires, editor
//! preferences — and expects it back verbatim on the next load. It lives in one
//! JSON file next to the corpus so a browser profile is not the source of
//! truth: two windows, or a wiped profile, still open the same workspace.
//!
//! Saving is a whole-document write to a temporary file followed by a rename,
//! so a crash mid-write cannot leave a half-written layout behind. Keys the
//! browser sends as `null` are deletions, which is how "reset layout" works.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Where the document lives.
///
/// A single executable runs from anywhere, so the workspace document belongs to the
/// USER, not to whatever directory the server was started in:
///
///   1. `QUBITICK_SETTINGS=<file|dir>` — explicit, for tests and packagers.
///   2. `<root>/web-settings.json` — when it already exists (a development checkout,
///      or an install that has been using it) so nobody's workspace moves under them.
///   3. `$XDG_CONFIG_HOME/qubitick/web-settings.json` (else `~/.config/qubitick/…`).
///   4. `<root>/web-settings.json` — last resort, when there is no home directory.
pub fn path_for(root: &Path) -> PathBuf {
    if let Ok(explicit) = std::env::var("QUBITICK_SETTINGS")
        && !explicit.is_empty()
    {
        let path = PathBuf::from(explicit);
        return if path.extension().is_some() {
            path
        } else {
            path.join("web-settings.json")
        };
    }
    let beside = root.join("web-settings.json");
    if beside.is_file() {
        return beside;
    }
    if let Some(dir) = config_dir() {
        return dir.join("web-settings.json");
    }
    beside
}

/// The user's config directory for this application.
fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("qubitick"));
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("qubitick"))
}

/// Read the stored document, or an empty object when there is none (or it is
/// unreadable — a corrupt settings file must not stop the workspace opening).
pub fn load(root: &Path) -> Value {
    match std::fs::read_to_string(path_for(root)) {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

/// Shallow merge of `patch` into `current`. Top-level `null` removes a key.
pub fn merge(current: &Value, patch: &Value) -> Value {
    let mut out: Map<String, Value> = match current {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    if let Value::Object(patch) = patch {
        for (key, value) in patch {
            if value.is_null() {
                out.remove(key);
            } else {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

/// Write the document atomically (temp file + rename inside the same directory).
pub fn save(root: &Path, doc: &Value) -> anyhow::Result<()> {
    let path = path_for(root);
    // The document may live in a directory that does not exist yet (a first run with
    // the engine installed as a single executable).
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(doc)?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_replaces_and_merges_top_level_keys() {
        let current = json!({ "layout": { "zoom": 1 }, "editor": { "fontSize": 13 } });
        let patch = json!({ "editor": { "fontSize": 15 } });
        let next = merge(&current, &patch);
        assert_eq!(next["editor"]["fontSize"], 15);
        // untouched keys survive
        assert_eq!(next["layout"]["zoom"], 1);
    }

    #[test]
    fn null_deletes_a_key() {
        let current = json!({ "layout": { "zoom": 1 }, "editor": {} });
        let next = merge(&current, &json!({ "layout": null }));
        assert!(next.get("layout").is_none());
        assert!(next.get("editor").is_some());
    }

    #[test]
    fn a_non_object_document_is_treated_as_empty() {
        let next = merge(&json!("garbage"), &json!({ "layout": { "zoom": 2 } }));
        assert_eq!(next["layout"]["zoom"], 2);
    }

    #[test]
    fn save_then_load_round_trips_and_replaces() {
        let dir = std::env::temp_dir().join(format!("qb-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        /* THE FILE IS PUT BESIDE THE ROOT FIRST. `save` writes wherever `path_for` says, and with
           no `<root>/web-settings.json` in existence that is rule 3 — `$XDG_CONFIG_HOME/qubitick`
           or `~/.config/qubitick`, i.e. the DEVELOPER's workspace. This test used to run against
           the fallback: it passed, because `load` reads the same place, while writing the value
           below into the real user's settings on every run. Pinning the file makes the rule under
           test the round trip, and this assertion is the pin. */
        std::fs::write(dir.join("web-settings.json"), "{}").unwrap();
        assert_eq!(path_for(&dir), dir.join("web-settings.json"));
        save(&dir, &json!({ "layout": { "zoom": 0.5 } })).unwrap();
        assert_eq!(load(&dir)["layout"]["zoom"], 0.5);
        save(&dir, &json!({ "editor": { "tabSize": 2 } })).unwrap();
        let back = load(&dir);
        assert_eq!(back["editor"]["tabSize"], 2);
        assert!(back.get("layout").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}