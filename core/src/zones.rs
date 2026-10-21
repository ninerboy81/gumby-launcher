//! Terror zones on the home screen: the current and next zone with names, from the owner's own zone server
//! (read-only, no key: `GET /launcher/zones`). Fetched in the background once a minute and right after a switch.
use serde_json::{json, Value};

pub const SERVER: &str = "http://144.217.29.142:8190";

/// Act of an area id (D2R level ids: Act 1 1-39, Act 2 40-74, Act 3 75-102, Act 4 103-108, Act 5 109+).
pub fn act_of(id: i64) -> Option<u8> {
    match id { 1..=39 => Some(1), 40..=74 => Some(2), 75..=102 => Some(3), 103..=108 => Some(4), 109..=136 => Some(5), _ => None }
}

/// Area names for the card: "Halls of the Dead Level 1, Level 2, Level 3" become one "Halls of the Dead", repeats go.
pub fn short_names(v: &Value) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for a in v.as_array()? {
        let Some(name) = a.get("name").and_then(|n| n.as_str()) else { continue };
        let base = match name.rfind(" Level ") { Some(i) if name[i + 7..].chars().all(|c| c.is_ascii_digit()) => &name[..i], _ => name };
        if !out.iter().any(|o| o == base) { out.push(base.to_string()); }
    }
    if out.is_empty() { None } else { Some(out.join(", ")) }
}

/// The server's answer turned into what the card shows: {now, now_act, next, switch_at, updated}; `now`/`next` are
/// null while the server does not know them yet.
pub fn parse(body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "The zone server sent something unexpected".to_string())?;
    let names = |list: &str, joined: &str| short_names(&v[list]).or_else(|| v.get(joined).and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()));
    let act = v.get("current").and_then(|c| c.as_array()).and_then(|c| c.first()).and_then(|c| c.get("id")).and_then(|i| i.as_i64()).and_then(act_of);
    Ok(json!({"now": names("current", "current_names"), "now_act": act, "next": names("next", "next_names"),
        "switch_at": v.get("switch_at").and_then(|s| s.as_i64()), "updated": v.get("updated").and_then(|s| s.as_i64())}))
}

/// One fetch (blocking, a few seconds at most).
pub fn fetch() -> Result<Value, String> {
    let r = crate::http::request("GET", &format!("{}/launcher/zones", SERVER), &[("User-Agent", "GumbyLauncher"), ("Accept", "application/json")],
        None, 6000, 64 * 1024, None).map_err(|_| "Terror zones are not reachable right now".to_string())?;
    if r.status != 200 { return Err("Terror zones are not reachable right now".into()); }
    parse(&r.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_the_server_answer() {
        let body = br#"{"current":[{"id":108,"name":"The Chaos Sanctuary"}],"current_names":"The Chaos Sanctuary",
            "next":[{"id":66,"name":"Tal Rasha's Tomb"},{"id":46,"name":"Canyon of the Magi"}],"next_names":"Tal Rasha's Tomb, Canyon of the Magi",
            "switch_at":1792500000,"minutes_left":23,"updated":1792498200}"#;
        let v = parse(body).unwrap();
        assert_eq!(v["now"], "The Chaos Sanctuary");
        assert_eq!(v["now_act"], 4);
        assert_eq!(v["next"], "Tal Rasha's Tomb, Canyon of the Magi");
        // the server's real answer from 2026-10-20: levels fold into one name
        let live = br#"{"current":[{"id":113,"name":"Crystalline Passage"},{"id":114,"name":"Frozen River"}],"current_names":"Crystalline Passage, Frozen River",
            "next":[{"id":42,"name":"Dry Hills"},{"id":56,"name":"Halls of the Dead Level 1"},{"id":57,"name":"Halls of the Dead Level 2"},{"id":60,"name":"Halls of the Dead Level 3"}],
            "next_names":"Dry Hills, Halls of the Dead Level 1, Halls of the Dead Level 2, Halls of the Dead Level 3","switch_at":1792564200,"minutes_left":2,"updated":null}"#;
        assert_eq!(v["switch_at"], 1792500000);
        let v = parse(live).unwrap();
        assert_eq!((v["now"].as_str(), v["now_act"].as_i64(), v["next"].as_str()), (Some("Crystalline Passage, Frozen River"), Some(5), Some("Dry Hills, Halls of the Dead")));
        let unknown = parse(br#"{"current":[],"current_names":"","next":[],"next_names":"","switch_at":1792500000}"#).unwrap();
        assert!(unknown["now"].is_null() && unknown["next"].is_null());
        assert!(parse(b"<html>").is_err());
        assert_eq!((act_of(1), act_of(74), act_of(75), act_of(103), act_of(131), act_of(0)), (Some(1), Some(2), Some(3), Some(4), Some(5), None));
    }
}
