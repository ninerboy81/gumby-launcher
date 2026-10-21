//! Saved accounts, groups and the one setting, in %APPDATA%\<app>\accounts.json. An account signs in with its email
//! and password, or - with the Battle.net Authenticator - with a sign-in token from Battle.net's own login page. Either
//! secret is locked with Windows DPAPI for the Windows user before it is written; the file never holds one in the
//! clear, and `view()` (what the window shows) never holds one at all.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const MAX_ACCOUNTS: usize = 8;
/// One game per screen quarter.
pub const MAX_GROUP: usize = 4;
pub const REGIONS: [(&str, &str, &str); 3] = [
    ("Americas", "US", "us.actual.battle.net"),
    ("Europe", "EU", "eu.actual.battle.net"),
    ("Asia", "KR", "kr.actual.battle.net"),
];

/// Battle.net's sign-in page for a token account of this region.
pub fn signin_url(region: &str) -> Option<String> {
    let host = match region { "Americas" => "us", "Europe" => "eu", "Asia" => "kr", _ => return None };
    Some(format!("https://{}.battle.net/login/en/?externalChallenge=login&app=OSI", host))
}
/// The REGION value D2R reads with a token.
fn osi_region(region: &str) -> &'static str { match region { "Europe" => "EU", "Asia" => "KR", _ => "NA" } }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub id: String, pub name: String, pub email: String, pub region: String, secret: String,
    /// "password" or "token" (Battle.net Authenticator)
    #[serde(default = "password_method")] pub method: String,
}
fn password_method() -> String { "password".into() }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Group { pub id: String, pub name: String, pub accounts: Vec<String> }

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)] format: u32,
    #[serde(default)] note: String,
    #[serde(default)] accounts: Vec<Account>,
    #[serde(default)] groups: Vec<Group>,
    #[serde(default)] allow_multi: bool,
    #[serde(default)] game_folder: Option<String>,
}

/// What the add/edit account dialog sends. An empty password on an edit keeps the saved one.
#[derive(Deserialize, Default)]
pub struct AccountInput {
    #[serde(default)] pub id: String, pub name: String, #[serde(default)] pub email: String, #[serde(default)] pub password: String,
    pub region: String, #[serde(default)] pub method: String,
    /// The sign-in token caught from Battle.net's page (filled in by the engine, never sent by the window).
    #[serde(skip)] pub token: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct GroupInput { #[serde(default)] pub id: String, pub name: String, pub accounts: Vec<String> }

pub struct Store { path: PathBuf, data: File }

pub fn region_host(region: &str) -> Option<&'static str> { REGIONS.iter().find(|r| r.0 == region).map(|r| r.2) }
fn region_tag(region: &str) -> &'static str { REGIONS.iter().find(|r| r.0 == region).map_or("?", |r| r.1) }

/// The D2R.exe arguments for an email + password login: signed in by itself, intro videos always skipped.
pub fn launch_args(email: &str, password: &str, region: &str) -> Result<Vec<String>, String> {
    let host = region_host(region).ok_or("Pick a region")?;
    Ok(vec!["-username".into(), email.into(), "-password".into(), password.into(), "-address".into(), host.into(), "-skiplogovideo".into()])
}

fn new_id(prefix: char, taken: &dyn Fn(&str) -> bool) -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    (0..).map(|n| if n == 0 { format!("{}{}", prefix, t) } else { format!("{}{}-{}", prefix, t, n) }).find(|c| !taken(c)).unwrap()
}

fn clean_name(name: &str, what: &str) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > 24 { return Err(format!("Give the {} a name (up to 24 letters)", what)); }
    Ok(name)
}

impl Store {
    pub fn load(dir: &Path) -> Store {
        let path = dir.join("accounts.json");
        let data = std::fs::read(&path).ok()
            .and_then(|raw| serde_json::from_slice::<File>(raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw)).ok())
            .unwrap_or_default();
        Store { path, data }
    }

    fn save(&mut self) -> Result<(), String> {
        self.data.format = 1;
        self.data.note = "Passwords are locked with Windows encryption (DPAPI) for this Windows user.".into();
        std::fs::create_dir_all(self.path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for _ in 0..10 {
            if std::fs::rename(&tmp, &self.path).is_ok() { return Ok(()); }
            std::thread::sleep(std::time::Duration::from_millis(50));   // a virus scanner may hold the file a moment
        }
        Err("Could not save your accounts".into())
    }

    pub fn accounts(&self) -> &[Account] { &self.data.accounts }
    pub fn groups(&self) -> &[Group] { &self.data.groups }
    pub fn account(&self, id: &str) -> Option<&Account> { self.data.accounts.iter().find(|a| a.id == id) }
    pub fn group(&self, id: &str) -> Option<&Group> { self.data.groups.iter().find(|g| g.id == id) }
    pub fn allow_multi(&self) -> bool { self.data.allow_multi }
    pub fn game_folder_setting(&self) -> Option<&str> { self.data.game_folder.as_deref() }

    pub fn save_account(&mut self, a: AccountInput) -> Result<String, String> {
        let name = clean_name(&a.name, "account")?;
        let token_method = a.method == "token";
        let email = a.email.trim().to_string();
        if !token_method && (!email.contains('@') || email.len() > 120 || email.contains(char::is_whitespace)) { return Err("Enter the Battle.net email address".into()); }
        if region_host(&a.region).is_none() { return Err("Pick a region".into()); }
        if a.password.chars().count() > 128 { return Err("That password is too long".into()); }
        let existing = self.data.accounts.iter().position(|x| !a.id.is_empty() && x.id == a.id);
        if !a.id.is_empty() && existing.is_none() { return Err("That account is not saved any more".into()); }
        if existing.is_none() && self.data.accounts.len() >= MAX_ACCOUNTS { return Err(format!("You can save up to {} accounts", MAX_ACCOUNTS)); }
        let method = if token_method { "token" } else { "password" };
        let same_method = existing.map_or(false, |i| self.data.accounts[i].method == method);
        let fresh = if token_method { a.token.clone().unwrap_or_default() } else { a.password.clone() };
        let secret = if fresh.is_empty() {
            match existing {
                Some(i) if same_method => self.data.accounts[i].secret.clone(),
                _ => return Err(if token_method { "Sign in with Battle.net first".into() } else { "Enter the password".into() }),
            }
        } else {
            let blob = crate::win::protect(fresh.as_bytes()).ok_or("Windows could not lock the sign-in")?;
            crate::b64::encode(&blob)
        };
        let id = match existing { Some(i) => self.data.accounts[i].id.clone(), None => new_id('a', &|c| self.data.accounts.iter().any(|x| x.id == c)) };
        let email = if token_method && !email.contains('@') { String::new() } else { email };
        let acct = Account { id: id.clone(), name, email, region: a.region, secret, method: method.into() };
        match existing { Some(i) => self.data.accounts[i] = acct, None => self.data.accounts.push(acct) }
        self.save()?;
        Ok(id)
    }

    /// Removes an account and takes it out of every group (a group left empty goes too).
    pub fn remove_account(&mut self, id: &str) -> Result<(), String> {
        let before = self.data.accounts.len();
        self.data.accounts.retain(|a| a.id != id);
        if self.data.accounts.len() == before { return Err("That account is not saved".into()); }
        for g in self.data.groups.iter_mut() { g.accounts.retain(|a| a != id); }
        self.data.groups.retain(|g| !g.accounts.is_empty());
        self.save()
    }

    pub fn save_group(&mut self, g: GroupInput) -> Result<String, String> {
        let name = clean_name(&g.name, "group")?;
        if g.accounts.is_empty() { return Err("Pick at least one account".into()); }
        if g.accounts.len() > MAX_GROUP { return Err(format!("A group opens up to {} games, one per quarter of the screen", MAX_GROUP)); }
        for (i, a) in g.accounts.iter().enumerate() {
            if self.account(a).is_none() { return Err("One of those accounts is not saved any more".into()); }
            if g.accounts[..i].contains(a) { return Err("An account can be in a group only once".into()); }
        }
        let existing = self.data.groups.iter().position(|x| !g.id.is_empty() && x.id == g.id);
        if !g.id.is_empty() && existing.is_none() { return Err("That group is not saved any more".into()); }
        let id = match existing { Some(i) => self.data.groups[i].id.clone(), None => new_id('g', &|c| self.data.groups.iter().any(|x| x.id == c)) };
        let group = Group { id: id.clone(), name, accounts: g.accounts };
        match existing { Some(i) => self.data.groups[i] = group, None => self.data.groups.push(group) }
        self.save()?;
        Ok(id)
    }

    pub fn remove_group(&mut self, id: &str) -> Result<(), String> {
        let before = self.data.groups.len();
        self.data.groups.retain(|g| g.id != id);
        if self.data.groups.len() == before { return Err("That group is not saved".into()); }
        self.save()
    }

    pub fn set_allow_multi(&mut self, on: bool) -> Result<(), String> { self.data.allow_multi = on; self.save() }

    pub fn set_game_folder(&mut self, folder: Option<String>) -> Result<(), String> {
        if let Some(f) = &folder {
            if !Path::new(f).join("D2R.exe").is_file() { return Err("D2R.exe is not in that folder".into()); }
        }
        self.data.game_folder = folder.filter(|f| !f.trim().is_empty());
        self.save()
    }

    /// The unlocked password, only to start the game.
    pub fn password(&self, id: &str) -> Result<String, String> {
        let a = self.account(id).ok_or("That account is not saved")?;
        let blob = crate::b64::decode(&a.secret).ok_or("The saved password is damaged: edit the account and enter it again")?;
        let plain = crate::win::unprotect(&blob).ok_or("The saved password can't be unlocked on this Windows user: edit the account and enter it again")?;
        String::from_utf8(plain).map_err(|_| "The saved password is damaged: edit the account and enter it again".to_string())
    }

    /// How to start an account's game (holds its password or token: use it at once and drop it).
    pub fn args_for(&self, id: &str) -> Result<crate::launch::Start, String> {
        let a = self.account(id).ok_or("That account is not saved")?;
        if a.method == "token" {
            return Ok(crate::launch::Start { args: vec!["-uid".into(), "osi".into(), "-skiplogovideo".into()],
                token: Some((self.password(id)?, osi_region(&a.region).into())) });
        }
        Ok(crate::launch::Start { args: launch_args(&a.email, &self.password(id)?, &a.region)?, token: None })
    }

    /// What the window shows: never a password, never the locked blob.
    pub fn view(&self) -> Value {
        let accounts: Vec<Value> = self.data.accounts.iter().map(|a| json!({"id": a.id, "name": a.name, "email": a.email,
            "region": a.region, "tag": region_tag(&a.region), "method": a.method})).collect();
        let groups: Vec<Value> = self.data.groups.iter().map(|g| json!({"id": g.id, "name": g.name, "accounts": g.accounts})).collect();
        json!({"accounts": accounts, "groups": groups, "allow_multi": self.data.allow_multi, "max_accounts": MAX_ACCOUNTS, "max_group": MAX_GROUP,
            "regions": REGIONS.iter().map(|r| r.0).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Store::load(&dir);
        (dir, s)
    }
    fn acct(name: &str, email: &str, pw: &str) -> AccountInput {
        AccountInput { id: String::new(), name: name.into(), email: email.into(), password: pw.into(), region: "Americas".into(), method: String::new(), token: None }
    }

    #[test]
    fn args_and_regions() {
        assert_eq!(launch_args("me@example.com", "pw word", "Americas").unwrap(),
            vec!["-username", "me@example.com", "-password", "pw word", "-address", "us.actual.battle.net", "-skiplogovideo"]);
        assert_eq!(launch_args("a@b", "x", "Europe").unwrap()[5], "eu.actual.battle.net");
        assert_eq!(launch_args("a@b", "x", "Asia").unwrap()[5], "kr.actual.battle.net");
        assert!(launch_args("a@b", "x", "Mars").is_err());
    }

    #[test]
    fn the_file_never_holds_the_password_and_the_view_never_shows_it() {
        let (dir, mut s) = temp_store("pw");
        let id = s.save_account(acct("Main", "me@example.com", "S3cret-Passw0rd!")).unwrap();
        let raw = std::fs::read_to_string(dir.join("accounts.json")).unwrap();
        assert!(!raw.contains("S3cret-Passw0rd!") && raw.contains("me@example.com"));
        let shown = s.view().to_string();
        assert!(!shown.contains("S3cret") && !shown.contains("secret"));
        assert_eq!(s.password(&id).unwrap(), "S3cret-Passw0rd!");
        assert_eq!(s.args_for(&id).unwrap().args[3], "S3cret-Passw0rd!");
        // reloads from disk the same
        let back = Store::load(&dir);
        assert_eq!(back.password(&id).unwrap(), "S3cret-Passw0rd!");
        assert_eq!(back.view()["accounts"][0]["tag"], "US");
        // an edit with an empty password keeps the saved one
        s.save_account(AccountInput { id: id.clone(), name: "Main".into(), email: "me2@example.com".into(), password: String::new(), region: "Europe".into(), method: String::new(), token: None }).unwrap();
        assert_eq!(s.password(&id).unwrap(), "S3cret-Passw0rd!");
        assert_eq!(s.account(&id).unwrap().email, "me2@example.com");
        assert_eq!(s.accounts().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_token_account_starts_with_its_token_and_never_shows_it() {
        let (dir, mut s) = temp_store("token");
        let tok = |t: &str| AccountInput { name: "Mule Sorc".into(), region: "Europe".into(), method: "token".into(), token: Some(t.into()), ..Default::default() };
        assert!(s.save_account(AccountInput { token: None, ..tok("") }).is_err());   // sign in first
        let id = s.save_account(tok("EU-abcdef0123456789-42")).unwrap();
        let raw = std::fs::read_to_string(dir.join("accounts.json")).unwrap();
        assert!(!raw.contains("EU-abcdef0123456789-42"));
        assert!(!s.view().to_string().contains("abcdef0123456789"));
        assert_eq!(s.view()["accounts"][0]["method"], "token");
        let st = s.args_for(&id).unwrap();
        assert_eq!(st.args, vec!["-uid", "osi", "-skiplogovideo"]);
        assert_eq!(st.token, Some(("EU-abcdef0123456789-42".to_string(), "EU".to_string())));
        // editing without signing in again keeps the token; switching to a password needs the password
        s.save_account(AccountInput { id: id.clone(), ..tok("") }).unwrap();
        assert_eq!(s.password(&id).unwrap(), "EU-abcdef0123456789-42");
        assert!(s.save_account(AccountInput { id: id.clone(), name: "Mule Sorc".into(), email: "m@example.com".into(), region: "Europe".into(), ..Default::default() }).is_err());
        assert_eq!(signin_url("Asia").unwrap(), "https://kr.battle.net/login/en/?externalChallenge=login&app=OSI");
        // accounts saved before tokens existed load as password accounts
        std::fs::write(dir.join("accounts.json"), r#"{"accounts":[{"id":"a1","name":"Old","email":"o@example.com","region":"Americas","secret":"AAAA"}]}"#).unwrap();
        assert_eq!(Store::load(&dir).accounts()[0].method, "password");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn account_checks_and_limit() {
        let (dir, mut s) = temp_store("limit");
        assert!(s.save_account(acct("", "me@example.com", "x")).is_err());
        assert!(s.save_account(acct("A", "not-an-email", "x")).is_err());
        assert!(s.save_account(acct("A", "a@example.com", "")).is_err());   // a new account needs its password
        assert!(s.save_account(AccountInput { region: "Mars".into(), ..acct("A", "a@example.com", "x") }).is_err());
        let ids: Vec<String> = (0..MAX_ACCOUNTS).map(|n| s.save_account(acct(&format!("Mule {}", n), &format!("m{}@example.com", n), "pw")).unwrap()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), MAX_ACCOUNTS);   // each its own id, even saved in the same millisecond
        assert!(s.save_account(acct("One more", "x@example.com", "pw")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn groups_keep_their_order_and_follow_removed_accounts() {
        let (dir, mut s) = temp_store("groups");
        let barb = s.save_account(acct("Main Barb", "b@example.com", "1")).unwrap();
        let sorc = s.save_account(acct("Mule Sorc", "n@example.com", "2")).unwrap();
        let pally = s.save_account(acct("Pally Alt", "r@example.com", "3")).unwrap();
        let mule = s.save_account(acct("Mule", "m@example.com", "4")).unwrap();
        let extra = s.save_account(acct("Extra", "e@example.com", "5")).unwrap();
        let g = s.save_group(GroupInput { id: String::new(), name: "Farm team".into(), accounts: vec![barb.clone(), sorc.clone(), pally.clone()] }).unwrap();
        assert_eq!(s.group(&g).unwrap().accounts, vec![barb.clone(), sorc.clone(), pally.clone()]);
        assert!(s.save_group(GroupInput { id: String::new(), name: "Too many".into(), accounts: vec![barb.clone(), sorc.clone(), pally.clone(), mule.clone(), extra] }).is_err());
        assert!(s.save_group(GroupInput { id: String::new(), name: "Twice".into(), accounts: vec![barb.clone(), barb.clone()] }).is_err());
        assert!(s.save_group(GroupInput { id: String::new(), name: "Ghost".into(), accounts: vec!["nope".into()] }).is_err());
        assert!(s.save_group(GroupInput { id: String::new(), name: "".into(), accounts: vec![barb.clone()] }).is_err());
        let solo = s.save_group(GroupInput { id: String::new(), name: "Solo run".into(), accounts: vec![sorc.clone()] }).unwrap();
        // editing reorders
        s.save_group(GroupInput { id: g.clone(), name: "Farm team".into(), accounts: vec![pally.clone(), barb.clone()] }).unwrap();
        assert_eq!(Store::load(&dir).group(&g).unwrap().accounts, vec![pally.clone(), barb.clone()]);
        // removing an account takes it out of groups; an emptied group goes
        s.remove_account(&sorc).unwrap();
        assert!(s.group(&solo).is_none());
        s.remove_account(&pally).unwrap();
        assert_eq!(s.group(&g).unwrap().accounts, vec![barb.clone()]);
        s.remove_group(&g).unwrap();
        assert!(s.groups().is_empty());
        // the several-games switch is off until turned on, and is saved
        assert!(!s.allow_multi());
        s.set_allow_multi(true).unwrap();
        assert!(Store::load(&dir).allow_multi());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
