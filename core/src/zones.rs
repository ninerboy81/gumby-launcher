//! Terror zones on the home screen: the current and next zone with names, from the owner's own zone server
//! (read-only, no key: `GET /launcher/zones`). Fetched in the background once a minute and right after a switch.
use serde_json::{json, Value};

pub const SERVER: &str = "http://144.217.29.142:8190";

/// Act of an area id (D2R level ids: Act 1 1-39, Act 2 40-74, Act 3 75-102, Act 4 103-108, Act 5 109+).
pub fn act_of(id: i64) -> Option<u8> {
    match id { 1..=39 => Some(1), 40..=74 => Some(2), 75..=102 => Some(3), 103..=108 => Some(4), 109..=136 => Some(5), _ => None }
}

/// The server's answer turned into what the card shows: {now, now_act, next, switch_at, updated}; `now`/`next` are
/// null while the server does not know them yet.
pub fn parse(body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "The zone server sent something unexpected".to_string())?;
    let names = |k: &str| v.get(k).and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string());
    let act = v.get("current").and_then(|c| c.as_array()).and_then(|c| c.first()).and_then(|c| c.get("id")).and_then(|i| i.as_i64()).and_then(act_of);
    Ok(json!({"now": names("current_names"), "now_act": act, "next": names("next_names"),
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
        assert_eq!(v["switch_at"], 1792500000);
        let unknown = parse(br#"{"current":[],"current_names":"","next":[],"next_names":"","switch_at":1792500000}"#).unwrap();
        assert!(unknown["now"].is_null() && unknown["next"].is_null());
        assert!(parse(b"<html>").is_err());
        assert_eq!((act_of(1), act_of(74), act_of(75), act_of(103), act_of(131), act_of(0)), (Some(1), Some(2), Some(3), Some(4), Some(5), None));
    }
}
