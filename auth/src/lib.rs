/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Copyright (c) 2026 Breno Xavier
 */

//! The Stein Client's Microsoft account. It started from Steinflayer's `stein-auth` (the same 5-step chain), plus
//! what a launcher needs on top of it:
//!
//! - the device code goes to the UI ([`begin`] returns the code and the URL; [`Pending::finish`] waits for the
//!   player to sign in, and can be cancelled);
//! - the Microsoft refresh token lives in the **system vault** (Credential Manager on Windows, Keychain on macOS),
//!   not in a plain-text JSON file;
//! - multiple accounts: the list (uuid and name, nothing secret) lives in a file; each account's secret, in the
//!   vault.
//!
//! **Accounts belong to Stein, not to the launcher:** they live in their own folder, shared by every Stein tool
//! ([`shared_dir`]: `%APPDATA%\.steinauth` on Windows, `~/.steinauth` elsewhere), with the secret in the system vault
//! under the `SteinAuth` service. The Stein Client, a bot or any other tool opens the same list with
//! [`Store::shared`] and signs in with the same accounts without asking for a code again. The folder is never inside
//! a portable install: the secrets belong to this machine's vault and do not travel with the folder.
//!
//! The flow:
//! 1. device code (Live ID) -> the player types it at microsoft.com/link;
//! 2. the Microsoft token (and the refresh token, which is stored);
//! 3. Xbox Live -> 4. XSTS -> 5. `login_with_xbox` and the Minecraft profile. An account without the game is refused
//!    ([`AuthError::NoMinecraft`]): the Stein Client is for the original Minecraft only.
//!
//! Offline accounts: only with an original account in the launcher. The offline nick is for single player, local
//! servers or servers without authentication (and for mod developers); whoever uses it has already proven they own
//! the game. Every time the game is launched with an offline account, the original account is confirmed again with
//! Microsoft ([`verify_owner`]); without internet, the last confirmation is honoured for [`OFFLINE_GRACE`].
//!
//! The `client_id` is the public Live ID one used by mineflayer, azalea and others (the Nintendo Switch edition's):
//! it accepts the device code flow without a registered app. It lives in a single constant: switching to our own app
//! registered in Azure means changing that one line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The Live ID client_id (the Nintendo Switch edition's, the same one mineflayer and azalea use).
pub const MS_CLIENT_ID: &str = "00000000441cc96b";
const MS_SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
const MS_DEVICE_CODE_URL: &str = "https://login.live.com/oauth20_connect.srf";
const MS_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const XBOX_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MOJANG_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MOJANG_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
/// The system vault service, shared by everything that uses the Stein accounts.
const VAULT_SERVICE: &str = "SteinAuth";
/// The service used by the first Stein Client test builds (accounts inside .steinclient); kept only for migration.
const LEGACY_VAULT_SERVICE: &str = "SteinClient";
/// Without internet, how long the last confirmation of the original account keeps offline accounts usable.
pub const OFFLINE_GRACE: Duration = Duration::from_secs(30 * 24 * 3600);

/// What the game needs to log in: the Minecraft token (short-lived, never stored), the undashed uuid and the name.
#[derive(Clone)]
pub struct Credentials {
    pub access_token: String,
    pub uuid: String,
    pub username: String,
}

/// Prints everything but the token, so a `{:?}` in a log or an error never leaks it.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("access_token", &"***")
            .field("uuid", &self.uuid)
            .field("username", &self.username)
            .finish()
    }
}

/// A known account (what shows up in the list; the secret stays in the vault).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub uuid: String,
    pub name: String,
    /// Offline account (just the nick; nothing in the vault).
    #[serde(default)]
    pub offline: bool,
    /// Original account: when Microsoft last confirmed it owns the game (Unix seconds).
    #[serde(default)]
    pub verified: u64,
}

impl Account {
    fn original(uuid: String, name: String) -> Account {
        Account { uuid, name, offline: false, verified: now() }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The uuid of an offline nick, the same one a server without authentication computes: UUID v3 of
/// `OfflinePlayer:<nick>`.
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// A nick the game accepts: 3 to 16 letters, digits or _.
pub fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The code the player types.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Sem conexão com a Microsoft. Verifique sua internet e tente novamente.")]
    Http(#[from] reqwest::Error),
    #[error("A Microsoft respondeu de forma inesperada. Tente novamente em instantes.")]
    Json(#[from] serde_json::Error),
    #[error("O código expirou. Clique em Entrar com a Microsoft para gerar outro.")]
    Expired,
    #[error("Entrada cancelada.")]
    Cancelled,
    #[error("A Microsoft recusou a entrada ({0}).")]
    Declined(String),
    #[error("Esta conta Microsoft ainda não tem um perfil Xbox. Acesse minecraft.net uma vez para criá-lo e tente novamente.")]
    NoXbox,
    #[error("Esta conta não possui o Minecraft: Java Edition.")]
    NoMinecraft,
    #[error("A sessão desta conta expirou. Entre novamente.")]
    NeedsLogin,
    #[error("Não foi possível acessar o cofre de credenciais do Windows ({0}).")]
    Vault(String),
    #[error("Erro inesperado ao entrar ({endpoint}: {detail}).")]
    Unexpected { endpoint: &'static str, detail: String },
    #[error("O nick deve ter de 3 a 16 caracteres, usando apenas letras, números e sublinhado (_).")]
    InvalidName,
    #[error("Já existe uma conta com esse nick.")]
    DuplicateName,
    #[error("Contas offline exigem uma conta Microsoft com o Minecraft: Java Edition no launcher. Adicione uma conta original para continuar.")]
    OwnerRequired,
    #[error("A sessão da sua conta original expirou. Entre novamente com ela para continuar usando contas offline.")]
    OwnerExpired,
    #[error("Não foi possível confirmar sua conta original. Conecte-se à internet e tente novamente.")]
    OwnerUnconfirmed,
}

fn http() -> Client {
    Client::builder()
        .user_agent(concat!("SteinClient/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("HTTP client")
}

/// A sign-in in progress: the code has been shown, the player still has to finish in the browser.
pub struct Pending {
    device_code: String,
    interval: u64,
    expires_in: u64,
    cancel: Arc<AtomicBool>,
}

/// Requests a new code. Show the `DeviceCode` and call [`Pending::finish`].
pub async fn begin() -> Result<(DeviceCode, Pending), AuthError> {
    let resp = http()
        .post(MS_DEVICE_CODE_URL)
        .form(&[("client_id", MS_CLIENT_ID), ("scope", MS_SCOPE), ("response_type", "device_code")])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(AuthError::Unexpected { endpoint: "device code", detail: resp.text().await.unwrap_or_default() });
    }
    #[derive(Deserialize)]
    struct Resp {
        device_code: String,
        user_code: String,
        verification_uri: String,
        interval: u64,
        expires_in: u64,
    }
    let r: Resp = resp.json().await?;
    Ok((
        DeviceCode { user_code: r.user_code, verification_uri: r.verification_uri, expires_in: r.expires_in },
        Pending {
            device_code: r.device_code,
            interval: r.interval.max(1),
            expires_in: r.expires_in.min(900),
            cancel: Arc::new(AtomicBool::new(false)),
        },
    ))
}

impl Pending {
    /// A cancel button: after `store(true)`, [`finish`](Pending::finish) stops at the next interval.
    pub fn canceller(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    /// Waits for the player to sign in, runs the rest of the chain and stores the account (list + vault).
    pub async fn finish(self, store: &Store) -> Result<Credentials, AuthError> {
        let client = http();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(self.expires_in);
        let mut interval = Duration::from_secs(self.interval);
        let (access, refresh) = loop {
            tokio::time::sleep(interval).await;
            if self.cancel.load(Ordering::Relaxed) {
                return Err(AuthError::Cancelled);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AuthError::Expired);
            }
            match token(
                &client,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", MS_CLIENT_ID),
                    ("device_code", &self.device_code),
                ],
            )
            .await?
            {
                Token::Ok { access_token, refresh_token } => break (access_token, refresh_token),
                Token::Err { error, error_description } => match error.as_str() {
                    "authorization_pending" => continue,
                    "slow_down" => interval += Duration::from_secs(1),
                    "expired_token" => return Err(AuthError::Expired),
                    _ => return Err(AuthError::Declined(format!("{error}: {error_description}"))),
                },
            }
        };
        let creds = minecraft(&client, &access).await?;
        store.save(&Account::original(creds.uuid.clone(), creds.username.clone()), &refresh)?;
        Ok(creds)
    }
}

/// Signs in again without a code, using the stored refresh token. Without one, or if it is rejected,
/// [`AuthError::NeedsLogin`].
pub async fn refresh(store: &Store, uuid: &str) -> Result<Credentials, AuthError> {
    let saved = store.secret(uuid)?.ok_or(AuthError::NeedsLogin)?;
    let client = http();
    let (access, new_refresh) = match token(
        &client,
        &[("grant_type", "refresh_token"), ("client_id", MS_CLIENT_ID), ("scope", MS_SCOPE), ("refresh_token", &saved)],
    )
    .await?
    {
        Token::Ok { access_token, refresh_token } => (access_token, refresh_token),
        Token::Err { .. } => return Err(AuthError::NeedsLogin),
    };
    let creds = minecraft(&client, &access).await?;
    store.save(&Account::original(creds.uuid.clone(), creds.username.clone()), &new_refresh)?;
    Ok(creds)
}

/// Confirms with Microsoft that there is an original account that owns the game, before launching with an offline
/// account. Tries each original account; without internet, a confirmation from within [`OFFLINE_GRACE`] is enough.
pub async fn verify_owner(store: &Store) -> Result<(), AuthError> {
    let owners: Vec<Account> = store.accounts().into_iter().filter(|a| !a.offline).collect();
    if owners.is_empty() {
        return Err(AuthError::OwnerRequired);
    }
    let mut unreachable = false;
    for a in &owners {
        match refresh(store, &a.uuid).await {
            Ok(_) => return Ok(()),
            Err(AuthError::Http(_)) => unreachable = true,
            Err(_) => {}
        }
    }
    if !unreachable {
        return Err(AuthError::OwnerExpired);
    }
    let recent = owners.iter().any(|a| {
        now().saturating_sub(a.verified) <= OFFLINE_GRACE.as_secs() && matches!(store.secret(&a.uuid), Ok(Some(_)))
    });
    if recent {
        Ok(())
    } else {
        Err(AuthError::OwnerUnconfirmed)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Token {
    Ok {
        access_token: String,
        refresh_token: String,
    },
    Err {
        error: String,
        #[serde(default)]
        error_description: String,
    },
}

async fn token(client: &Client, form: &[(&str, &str)]) -> Result<Token, AuthError> {
    Ok(client.post(MS_TOKEN_URL).form(form).send().await?.json().await?)
}

/// Steps 3 to 5: Xbox Live, XSTS, the Minecraft token and the profile.
async fn minecraft(client: &Client, ms_access: &str) -> Result<Credentials, AuthError> {
    #[derive(Deserialize)]
    struct Xbl {
        #[serde(rename = "Token")]
        token: String,
        #[serde(rename = "DisplayClaims")]
        claims: Claims,
    }
    #[derive(Deserialize)]
    struct Claims {
        xui: Vec<Xui>,
    }
    #[derive(Deserialize)]
    struct Xui {
        uhs: String,
    }
    let xbl: Xbl = client
        .post(XBOX_AUTH_URL)
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": ms_access },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let uhs = xbl.claims.xui.into_iter().next().map(|x| x.uhs).ok_or(AuthError::NoXbox)?;
    let resp = client
        .post(XSTS_URL)
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AuthError::NoXbox);
    }
    if !resp.status().is_success() {
        return Err(AuthError::Unexpected { endpoint: "xsts", detail: resp.text().await.unwrap_or_default() });
    }
    let xsts: Xbl = resp.json().await?;
    #[derive(Deserialize)]
    struct Login {
        access_token: String,
    }
    let login: Login = client
        .post(MOJANG_LOGIN_URL)
        .json(&serde_json::json!({ "identityToken": format!("XBL3.0 x={uhs};{}", xsts.token) }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let resp = client.get(MOJANG_PROFILE_URL).bearer_auth(&login.access_token).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AuthError::NoMinecraft);
    }
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
    }
    let p: Profile = resp.error_for_status()?.json().await?;
    Ok(Credentials { access_token: login.access_token, uuid: p.id, username: p.name })
}

/// The shared Stein accounts folder: `%APPDATA%\.steinauth` on Windows, `~/.steinauth` elsewhere.
/// `STEIN_AUTH_DIR` overrides it (tests, or a bot that wants its own accounts).
pub fn shared_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("STEIN_AUTH_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = if cfg!(windows) { std::env::var_os("APPDATA") } else { std::env::var_os("HOME") };
    base.map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".steinauth")
}

/// The accounts: the list in a file (`accounts.json`), the refresh tokens in the system vault.
pub struct Store {
    list: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Store {
        Store { list: dir.join("accounts.json") }
    }

    /// The Stein accounts, the same for the launcher and any other tool ([`shared_dir`]).
    pub fn shared() -> Store {
        Store::new(&shared_dir())
    }

    /// Pulls into this list the accounts from an old list (the first test builds kept it inside .steinclient),
    /// moving each secret to the new vault service. The old list is deleted afterwards.
    pub fn import_legacy(&self, old_dir: &Path) {
        let old = old_dir.join("accounts.json");
        let Some(accounts) = std::fs::read(&old).ok().and_then(|b| serde_json::from_slice::<Vec<Account>>(&b).ok())
        else {
            return;
        };
        let mut list = self.accounts();
        for a in accounts {
            if !a.offline {
                if let Ok(legacy) = keyring::Entry::new(LEGACY_VAULT_SERVICE, &a.uuid) {
                    if let Ok(secret) = legacy.get_password() {
                        let _ = Self::entry(&a.uuid)
                            .and_then(|e| e.set_password(&secret).map_err(|e| AuthError::Vault(e.to_string())));
                        let _ = legacy.delete_credential();
                    }
                }
            }
            if !list.iter().any(|b| b.uuid == a.uuid) {
                list.push(a);
            }
        }
        if self.write(&list).is_ok() {
            let _ = std::fs::remove_file(old);
        }
    }

    pub fn accounts(&self) -> Vec<Account> {
        std::fs::read(&self.list).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn write(&self, list: &[Account]) -> Result<(), AuthError> {
        if let Some(p) = self.list.parent() {
            std::fs::create_dir_all(p).map_err(|e| AuthError::Vault(e.to_string()))?;
        }
        // Write to a temp file and rename: the launcher and a bot may write the same list, and it must never be
        // left half-written.
        let tmp = self.list.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec_pretty(list)?).map_err(|e| AuthError::Vault(e.to_string()))?;
        std::fs::rename(&tmp, &self.list).map_err(|e| AuthError::Vault(e.to_string()))
    }

    fn entry(uuid: &str) -> Result<keyring::Entry, AuthError> {
        keyring::Entry::new(VAULT_SERVICE, uuid).map_err(|e| AuthError::Vault(e.to_string()))
    }

    fn secret(&self, uuid: &str) -> Result<Option<String>, AuthError> {
        match Self::entry(uuid)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Vault(e.to_string())),
        }
    }

    fn save(&self, account: &Account, refresh: &str) -> Result<(), AuthError> {
        Self::entry(&account.uuid)?.set_password(refresh).map_err(|e| AuthError::Vault(e.to_string()))?;
        let mut list = self.accounts();
        list.retain(|a| a.uuid != account.uuid);
        list.insert(0, account.clone());
        self.write(&list)
    }

    /// Is there an original account in the launcher (which is what allows creating offline accounts)?
    pub fn has_owner(&self) -> bool {
        self.accounts().iter().any(|a| !a.offline)
    }

    /// Creates an offline account. Requires an original account in the launcher.
    pub fn add_offline(&self, name: &str) -> Result<Account, AuthError> {
        let name = name.trim();
        if !valid_name(name) {
            return Err(AuthError::InvalidName);
        }
        if !self.has_owner() {
            return Err(AuthError::OwnerRequired);
        }
        let mut list = self.accounts();
        if list.iter().any(|a| a.name.eq_ignore_ascii_case(name)) {
            return Err(AuthError::DuplicateName);
        }
        let account = Account { uuid: offline_uuid(name), name: name.to_string(), offline: true, verified: 0 };
        list.push(account.clone());
        self.write(&list)?;
        Ok(account)
    }

    /// Removes the account from the list and its secret from the vault.
    pub fn remove(&self, uuid: &str) -> Result<(), AuthError> {
        if let Ok(e) = Self::entry(uuid) {
            let _ = e.delete_credential();
        }
        let mut list = self.accounts();
        list.retain(|a| a.uuid != uuid);
        self.write(&list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_server() {
        // Same as UUID.nameUUIDFromBytes("OfflinePlayer:Notch") in Java.
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn debug_hides_the_token() {
        let c = Credentials { access_token: "secret-token".into(), uuid: "u".into(), username: "Steve".into() };
        let shown = format!("{c:?}");
        assert!(!shown.contains("secret-token") && shown.contains("Steve"), "{shown}");
    }

    #[test]
    fn nick_validation() {
        assert!(valid_name("Steve_01"));
        assert!(!valid_name("ab"));
        assert!(!valid_name("name with space"));
        assert!(!valid_name("abcdefghijklmnopq"));
        assert!(!valid_name("João"));
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stein-auth-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_legacy_list_once() {
        let old = temp("old");
        let new = temp("new");
        let account = Account { uuid: offline_uuid("Test_01"), name: "Test_01".into(), offline: true, verified: 0 };
        std::fs::write(old.join("accounts.json"), serde_json::to_vec(&vec![account.clone()]).unwrap()).unwrap();
        let store = Store::new(&new);
        store.import_legacy(&old);
        assert_eq!(store.accounts(), vec![account.clone()]);
        assert!(!old.join("accounts.json").exists());
        store.import_legacy(&old); // no old list left, nothing changes
        assert_eq!(store.accounts(), vec![account]);
        assert!(std::fs::read_dir(&new).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    }

    #[test]
    fn shared_folder() {
        assert!(shared_dir().ends_with(".steinauth"));
    }
}
