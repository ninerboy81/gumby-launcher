//! Terror zones on the home screen: the current and next zone with names, from the owner's own zone server
//! (read-only, no key: `GET /launcher/zones`). Fetched in the background once a minute and right after a switch.
use serde_json::{json, Value};

pub const SERVER: &str = "http://144.217.29.142:8190";

/// Act of an area id (D2R level ids: Act 1 1-39, Act 2 40-74, Act 3 75-102, Act 4 103-108, Act 5 109+).
pub fn act_of(id: i64) -> Option<u8> {
    match id { 1..=39 => Some(1), 40..=74 => Some(2), 75..=102 => Some(3), 103..=108 => Some(4), 109..=136 => Some(5), _ => None }
}

/// The terror zones you can star for alerts: (key, name shown, act, the area ids the zone feed sends for it).
pub const GROUPS: &[(&str, &str, u8, &[i64])] = &[
    ("blood-moor", "Blood Moor & Den of Evil", 1, &[2, 8]),
    ("cold-plains", "Cold Plains & Cave", 1, &[3, 9, 13]),
    ("burial", "Burial Grounds, Crypt & Mausoleum", 1, &[17, 18, 19]),
    ("stony-field", "Stony Field", 1, &[4]),
    ("dark-wood", "Dark Wood & Underground Passage", 1, &[5, 10, 14]),
    ("black-marsh", "Black Marsh & The Hole", 1, &[6, 11, 15]),
    ("tower", "Forgotten Tower (Countess)", 1, &[20, 21, 22, 23, 24, 25]),
    ("jail", "Jail & Barracks", 1, &[28, 29, 30, 31]),
    ("cathedral", "Cathedral & Catacombs", 1, &[33, 34, 35, 36, 37]),
    ("pit", "The Pit (Tamoe Highland)", 1, &[7, 12, 16]),
    ("tristram", "Tristram", 1, &[38]),
    ("cows", "The Secret Cow Level", 1, &[39]),
    ("sewers", "Lut Gholein Sewers", 2, &[47, 48, 49]),
    ("rocky-waste", "Rocky Waste & Stony Tomb", 2, &[41, 55, 59]),
    ("dry-hills", "Dry Hills & Halls of the Dead", 2, &[42, 56, 57, 60]),
    ("far-oasis", "Far Oasis", 2, &[43]),
    ("lost-city", "Lost City, Valley of Snakes & Claw Viper Temple", 2, &[44, 45, 58, 61]),
    ("tunnels", "Ancient Tunnels", 2, &[65]),
    ("arcane", "Arcane Sanctuary", 2, &[74]),
    ("tal-rasha", "Tal Rasha's Tombs & Canyon of the Magi", 2, &[46, 66, 67, 68, 69, 70, 71, 72, 73]),
    ("spider-forest", "Spider Forest & Spider Cavern", 3, &[76, 84, 85]),
    ("great-marsh", "Great Marsh", 3, &[77]),
    ("flayer-jungle", "Flayer Jungle & Flayer Dungeon", 3, &[78, 88, 89, 91]),
    ("kurast", "Kurast Bazaar & the Temples", 3, &[79, 80, 81, 82, 92, 93, 94, 95, 96, 97, 98, 99]),
    ("travincal", "Travincal", 3, &[83]),
    ("durance", "Durance of Hate (Mephisto)", 3, &[100, 101, 102]),
    ("steppes", "Outer Steppes & Plains of Despair", 4, &[104, 105]),
    ("city-damned", "City of the Damned & River of Flame", 4, &[106, 107]),
    ("chaos", "The Chaos Sanctuary", 4, &[108]),
    ("foothills", "Bloody Foothills, Frigid Highlands & Abaddon", 5, &[110, 111, 125]),
    ("arreat", "Arreat Plateau & Pit of Acheron", 5, &[112, 126]),
    ("crystalline", "Crystalline Passage & Frozen River", 5, &[113, 114]),
    ("nihlathak", "Nihlathak's Temple & Halls", 5, &[121, 122, 123, 124]),
    ("glacial", "Glacial Trail & Drifter Cavern", 5, &[115, 116]),
    ("ancients", "The Ancients' Way & Icy Cellar", 5, &[118, 119]),
    ("tundra", "Frozen Tundra & Infernal Pit", 5, &[117, 127]),
    ("wsk", "Worldstone Keep & Throne of Destruction", 5, &[128, 129, 130, 131, 132]),
];

/// Which of `keys` (starred groups) cover any of these area ids.
pub fn starred_in<'a>(ids: &[i64], keys: &'a [String]) -> Vec<&'a String> {
    keys.iter().filter(|k| GROUPS.iter().any(|g| g.0 == k.as_str() && g.3.iter().any(|a| ids.contains(a)))).collect()
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

/// What the card shows for a list of areas: the terror zone's own short name when the areas belong to one of
/// GROUPS (Kurast's 12 areas read "Kurast Bazaar & the Temples"), else the area names with levels folded.
fn display_names(list: &Value) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    let mut loose: Vec<Value> = Vec::new();
    for a in list.as_array()? {
        match a["id"].as_i64().and_then(|id| GROUPS.iter().find(|g| g.3.contains(&id))) {
            Some(g) => { if !out.iter().any(|o| o == g.1) { out.push(g.1.to_string()); } }
            None => loose.push(a.clone()),
        }
    }
    if let Some(rest) = short_names(&Value::Array(loose)) { out.push(rest); }
    if out.is_empty() { None } else { Some(out.join(", ")) }
}

/// The server's answer turned into what the card shows: {now, now_act, next, switch_at, updated}; `now`/`next` are
/// null while the server does not know them yet.
pub fn parse(body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "The zone server sent something unexpected".to_string())?;
    let names = |list: &str, joined: &str| display_names(&v[list]).or_else(|| v.get(joined).and_then(|s| s.as_str()).filter(|s| !s.trim().is_empty()).map(|s| s.trim().to_string()));
    let act = v.get("current").and_then(|c| c.as_array()).and_then(|c| c.first()).and_then(|c| c.get("id")).and_then(|i| i.as_i64()).and_then(act_of);
    let ids = |k: &str| -> Vec<i64> { v[k].as_array().map(|a| a.iter().filter_map(|x| x["id"].as_i64()).collect()).unwrap_or_default() };
    Ok(json!({"now": names("current", "current_names"), "now_act": act, "next": names("next", "next_names"),
        "now_ids": ids("current"), "next_ids": ids("next"),
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
        // the user's screenshot 2026-10-21: Kurast's twelve areas
        let kurast = br#"{"current":[{"id":80,"name":"Kurast Bazaar"},{"id":79,"name":"Lower Kurast"},{"id":81,"name":"Upper Kurast"},{"id":82,"name":"Kurast Causeway"},{"id":92,"name":"Sewers Level 1"},{"id":93,"name":"Sewers Level 2"},{"id":94,"name":"Ruined Temple"},{"id":95,"name":"Disused Fane"},{"id":96,"name":"Forgotten Reliquary"},{"id":97,"name":"Forgotten Temple"},{"id":98,"name":"Ruined Fane"},{"id":99,"name":"Disused Reliquary"}],"next":[{"id":250,"name":"Somewhere New Level 2"}]}"#;
        let k = parse(kurast).unwrap();
        assert_eq!(k["now"], "Kurast Bazaar & the Temples");
        assert_eq!(k["next"], "Somewhere New");   // an area no group knows keeps its own name
        assert_eq!(v["now_act"], 4);
        assert_eq!(v["next"], "Tal Rasha's Tombs & Canyon of the Magi");
        // the server's real answer from 2026-10-20: levels fold into one name
        let live = br#"{"current":[{"id":113,"name":"Crystalline Passage"},{"id":114,"name":"Frozen River"}],"current_names":"Crystalline Passage, Frozen River",
            "next":[{"id":42,"name":"Dry Hills"},{"id":56,"name":"Halls of the Dead Level 1"},{"id":57,"name":"Halls of the Dead Level 2"},{"id":60,"name":"Halls of the Dead Level 3"}],
            "next_names":"Dry Hills, Halls of the Dead Level 1, Halls of the Dead Level 2, Halls of the Dead Level 3","switch_at":1792564200,"minutes_left":2,"updated":null}"#;
        assert_eq!(v["switch_at"], 1792500000);
        let v = parse(live).unwrap();
        assert_eq!((v["now"].as_str(), v["now_act"].as_i64(), v["next"].as_str()), (Some("Crystalline Passage & Frozen River"), Some(5), Some("Dry Hills & Halls of the Dead")));
        assert_eq!(v["now_ids"], json!([113, 114]));
        let stars = vec!["chaos".to_string(), "dry-hills".to_string()];
        assert!(starred_in(&[113, 114], &stars).is_empty());
        assert_eq!(starred_in(&[42, 56, 57, 60], &stars), vec![&stars[1]]);
        // every group key is unique and every id belongs to one group only
        let mut seen = std::collections::HashSet::new();
        for g in GROUPS { assert!(seen.insert(g.0), "{}", g.0); }
        let mut ids = std::collections::HashSet::new();
        for g in GROUPS { for a in g.3 { assert!(ids.insert(*a), "area {} twice", a); } }
        let unknown = parse(br#"{"current":[],"current_names":"","next":[],"next_names":"","switch_at":1792500000}"#).unwrap();
        assert!(unknown["now"].is_null() && unknown["next"].is_null());
        assert!(parse(b"<html>").is_err());
        assert_eq!((act_of(1), act_of(74), act_of(75), act_of(103), act_of(131), act_of(0)), (Some(1), Some(2), Some(3), Some(4), Some(5), None));
    }
}
