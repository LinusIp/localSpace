//! Who a user is (deployment §4; Pilot 1, Phase A): local accounts an admin
//! makes, sessions, one-time links for a first password, and the lockouts
//! that make repeated failed logins expensive. Every record lives in the
//! database; nothing here is in memory across a restart.
//!
//! The rules, as answered on 2026-09-12: passwords are argon2id at 64 MiB,
//! 3 iterations, 1 lane, kept as PHC strings; a session is 32 random bytes,
//! stored as its blake3, hard-expiring at the configured lifetime; a
//! one-time link is single-use and dies at 24 hours; five consecutive
//! failures lock an account for 15 minutes, doubling each further round,
//! and thirty failures from one address in 15 minutes lock the address for
//! 15 minutes; the answer to a wrong email and a wrong password is the same.

use crate::store::{InviteRecord, LockRecord, SessionRecord, Store, UserRecord};
use anyhow::{Context, Result};
use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use localspace_proto as proto;
use std::path::{Path, PathBuf};

/// The shortest password a user may set; nothing else is required of it.
pub const MIN_PASSWORD_CHARS: usize = 12;
/// The longest, well past the 64 the spec asks for: spaces and any Unicode,
/// counted as characters, never truncated.
pub const MAX_PASSWORD_CHARS: usize = 256;

/// The most common passwords, refused whatever their length: the one check
/// that prevents compromise, and it works offline. See the file's header.
static COMMON_PASSWORDS: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        include_str!("common-passwords.txt")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    });

/// Whether a password is on the common list, case-insensitively.
pub fn is_common_password(password: &str) -> bool {
    COMMON_PASSWORDS.contains(password.to_lowercase().as_str())
}
/// How long a one-time link lives.
pub const INVITE_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// Where the first administrator's link is written, under the data
/// directory, for the service user alone (the fourth answer of 2026-09-13).
pub const FIRST_ADMIN_LINK_FILE: &str = "first-admin-link.txt";

pub fn first_admin_link_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FIRST_ADMIN_LINK_FILE)
}

/// Write the first administrator's link where only the service user reads
/// it: mode 0600 on Unix; the data directory's own permissions elsewhere.
pub fn write_first_admin_link(data_dir: &Path, link: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let path = first_admin_link_path(data_dir);
    let text = format!(
        "Open this link within 24 hours to become the first administrator of localSpace:\n\n\
         {link}\n\n\
         This file is deleted when the link is used. A link older than 24 hours is dead:\n\
         restart the service for a new one, or run `localspace admin bootstrap` with the\n\
         service stopped.\n"
    );
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .with_context(|| format!("writing {}", path.display()))?;
    std::io::Write::write_all(&mut file, text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protecting {}", path.display()))?;
    }
    Ok(path)
}

/// The link's file goes when the link is used, or when it is stale.
pub fn remove_first_admin_link(data_dir: &Path) {
    let _ = std::fs::remove_file(first_admin_link_path(data_dir));
}

/// What a one-time link is, without spending it.
#[derive(Debug, Clone)]
pub enum InviteStatus {
    /// Used, expired, replaced, or never issued.
    Dead,
    /// A first or reset password for this account.
    ForUser(UserRecord),
    /// The first administrator's: the account is made when it is used.
    FirstAdmin,
}
/// Consecutive failures that lock an account.
pub const ACCOUNT_FAILURES: u32 = 5;
/// An account's first lock; each further round doubles it.
pub const ACCOUNT_LOCK_MS: u64 = 15 * 60 * 1000;
/// Failures from one address inside the window that lock the address.
pub const IP_FAILURES: u32 = 30;
pub const IP_WINDOW_MS: u64 = 15 * 60 * 1000;
pub const IP_LOCK_MS: u64 = 15 * 60 * 1000;

/// What went wrong, for the audit log. The user is told less: the same
/// sentence for a wrong email, a wrong password and a locked account, so
/// nothing says whether an account exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginFailure {
    NoSuchUser,
    WrongPassword,
    Disabled,
    NoPasswordYet,
    AccountLocked { until_ms: u64 },
    AddressLocked { until_ms: u64 },
}

impl LoginFailure {
    /// The word the audit record carries.
    pub fn label(&self) -> &'static str {
        match self {
            LoginFailure::NoSuchUser => "no_such_user",
            LoginFailure::WrongPassword => "wrong_password",
            LoginFailure::Disabled => "disabled",
            LoginFailure::NoPasswordYet => "no_password",
            LoginFailure::AccountLocked { .. } => "account_locked",
            LoginFailure::AddressLocked { .. } => "address_locked",
        }
    }

    /// What every failure says to the user.
    pub const MESSAGE: &'static str = "That email or password isn't right.";
}

#[derive(Debug, Clone)]
pub struct SignedIn {
    pub user: UserRecord,
    /// The session id, to go in the cookie. Only its hash is stored.
    pub session: String,
    pub expires_ms: u64,
}

/// The identity store: the database, and the hashing parameters.
#[derive(Clone)]
pub struct Directory {
    store: Store,
    session_ttl_ms: u64,
}

fn hasher() -> Argon2<'static> {
    // 64 MiB, 3 iterations, 1 lane (answer 2 of Phase A).
    let params = Params::new(64 * 1024, 3, 1, None).expect("argon2 parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password as the store keeps it.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(hasher()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing the password: {e}"))?
        .to_string())
}

/// A hash nothing matches, verified against when there is no account or no
/// password to check, so a failed sign-in costs the same whatever the
/// reason and the time it takes says nothing about who exists.
fn decoy_hash() -> &'static str {
    static DECOY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DECOY.get_or_init(|| hash_password("not anyone's password").unwrap_or_default())
}

/// Whether a password matches a stored hash. A hash that does not parse is
/// no match.
pub fn verify_password(password: &str, phc: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    hasher()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// 32 random bytes as hex: a session id, a one-time token.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// How a token is stored: never itself.
pub fn key_of(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

pub fn normalise_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

/// A local account with no password yet.
fn fresh_user(email: String, name: &str, roles: Vec<proto::UserRole>, now_ms: u64) -> UserRecord {
    UserRecord {
        id: format!("u_{}", uuid::Uuid::new_v4().simple()),
        email,
        name: name.trim().to_string(),
        roles,
        provider: "local".into(),
        password_hash: None,
        disabled: false,
        created_ms: now_ms,
        last_login_ms: None,
        password_set_ms: None,
    }
}

pub fn password_acceptable(password: &str) -> std::result::Result<(), String> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_CHARS {
        return Err(format!(
            "Use at least {MIN_PASSWORD_CHARS} characters for your password."
        ));
    }
    if length > MAX_PASSWORD_CHARS {
        return Err(format!(
            "Use at most {MAX_PASSWORD_CHARS} characters for your password."
        ));
    }
    if is_common_password(password) {
        return Err("That password is too common. Choose another one.".into());
    }
    Ok(())
}

impl Directory {
    pub fn new(store: Store, session_ttl_ms: u64) -> Directory {
        Directory {
            store,
            session_ttl_ms,
        }
    }

    pub fn session_ttl_ms(&self) -> u64 {
        self.session_ttl_ms
    }

    // -- users --------------------------------------------------------------

    /// A new local account with no password yet, and the one-time link's
    /// token to set one. The email must be new.
    pub fn create_user(
        &self,
        email: &str,
        name: &str,
        roles: Vec<proto::UserRole>,
        now_ms: u64,
    ) -> Result<(UserRecord, String)> {
        let email = normalise_email(email);
        if email.is_empty() || !email.contains('@') {
            anyhow::bail!("`{email}` is not an email address");
        }
        if self.store.user_by_email(&email)?.is_some() {
            anyhow::bail!("there is already an account for {email}");
        }
        let user = fresh_user(email, name, roles, now_ms);
        self.store.put_user(&user)?;
        let token = self.invite(&user.id, now_ms)?;
        Ok((user, token))
    }

    /// The first administrator's one-time link, while there are no accounts
    /// at all (the fourth answer of 2026-09-13). Minting it again kills the
    /// earlier one, so only the newest link — the one in the file and the
    /// log — opens.
    pub fn bootstrap_invite(&self, now_ms: u64) -> Result<String> {
        if !self.is_empty()? {
            anyhow::bail!("this server has accounts already; an administrator makes the next one");
        }
        for (key, mut invite) in self.store.invites()? {
            if invite.first_admin && invite.used_ms.is_none() {
                invite.used_ms = Some(now_ms);
                self.store.put_invite(&key, &invite)?;
            }
        }
        let token = random_token();
        self.store.put_invite(
            &key_of(&token),
            &InviteRecord {
                user: String::new(),
                created_ms: now_ms,
                expires_ms: now_ms + INVITE_TTL_MS,
                used_ms: None,
                first_admin: true,
            },
        )?;
        Ok(token)
    }

    /// Spend the first administrator's link: the account is made from the
    /// name and email given, with the password, and signed in.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_first_admin(
        &self,
        token: &str,
        email: &str,
        name: &str,
        password: &str,
        ip: &str,
        user_agent: &str,
        now_ms: u64,
    ) -> std::result::Result<SignedIn, String> {
        let key = key_of(token);
        let invite = self
            .store
            .get_invite(&key)
            .map_err(|e| format!("{e:#}"))?
            .filter(|i| i.first_admin && i.used_ms.is_none() && i.expires_ms > now_ms)
            .ok_or_else(|| {
                "This link has been used or has expired. Restart the service for a new one."
                    .to_string()
            })?;
        if !self.is_empty().map_err(|e| format!("{e:#}"))? {
            return Err("This server has an administrator already.".into());
        }
        let email = normalise_email(email);
        if email.is_empty() || !email.contains('@') {
            return Err("Give your work email address.".into());
        }
        let name = name.trim();
        if name.is_empty() {
            return Err("Give your name.".into());
        }
        password_acceptable(password)?;
        let hash = hash_password(password).map_err(|e| format!("{e:#}"))?;
        let mut user = fresh_user(email, name, vec![proto::UserRole::Admin], now_ms);
        user.password_hash = Some(hash);
        user.password_set_ms = Some(now_ms);
        user.last_login_ms = Some(now_ms);
        self.store.put_user(&user).map_err(|e| format!("{e:#}"))?;
        let mut invite = invite;
        invite.used_ms = Some(now_ms);
        invite.user = user.id.clone();
        self.store
            .put_invite(&key, &invite)
            .map_err(|e| format!("{e:#}"))?;
        self.start_session(user, ip, user_agent, now_ms)
            .map_err(|e| format!("{e:#}"))
    }

    /// What a token is, without spending it.
    pub fn invite_status(&self, token: &str, now_ms: u64) -> Result<InviteStatus> {
        let Some(invite) = self.store.get_invite(&key_of(token))? else {
            return Ok(InviteStatus::Dead);
        };
        if invite.used_ms.is_some() || invite.expires_ms <= now_ms {
            return Ok(InviteStatus::Dead);
        }
        if invite.first_admin {
            return Ok(if self.is_empty()? {
                InviteStatus::FirstAdmin
            } else {
                InviteStatus::Dead
            });
        }
        Ok(
            match self.store.get_user(&invite.user)?.filter(|u| !u.disabled) {
                Some(user) => InviteStatus::ForUser(user),
                None => InviteStatus::Dead,
            },
        )
    }

    pub fn users(&self) -> Result<Vec<UserRecord>> {
        let mut users = self.store.users()?;
        users.sort_by(|a, b| a.email.cmp(&b.email));
        Ok(users)
    }

    pub fn user(&self, id: &str) -> Result<Option<UserRecord>> {
        self.store.get_user(id)
    }

    /// Whether any account exists: the bootstrap question.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.store.users()?.is_empty())
    }

    /// A role change ends every live session: a revoked employee does not
    /// keep working because a tab stayed open.
    pub fn set_roles(&self, id: &str, roles: Vec<proto::UserRole>) -> Result<UserRecord> {
        let mut user = self.store.get_user(id)?.context("no such user")?;
        user.roles = roles;
        self.store.put_user(&user)?;
        self.revoke_sessions(id)?;
        Ok(user)
    }

    pub fn set_disabled(&self, id: &str, disabled: bool) -> Result<UserRecord> {
        let mut user = self.store.get_user(id)?.context("no such user")?;
        user.disabled = disabled;
        self.store.put_user(&user)?;
        if disabled {
            self.revoke_sessions(id)?;
        }
        Ok(user)
    }

    /// A reset forgets the password, ends every session, and gives a new
    /// one-time link.
    pub fn reset_password(&self, id: &str, now_ms: u64) -> Result<String> {
        let mut user = self.store.get_user(id)?.context("no such user")?;
        user.password_hash = None;
        user.password_set_ms = None;
        self.store.put_user(&user)?;
        self.revoke_sessions(id)?;
        self.invite(id, now_ms)
    }

    /// An admin clears an account's lock.
    pub fn unlock(&self, id: &str) -> Result<()> {
        self.store.clear_lock(&format!("account:{id}"))
    }

    pub fn account_locked_until(&self, id: &str, now_ms: u64) -> Result<Option<u64>> {
        Ok(self
            .store
            .get_lock(&format!("account:{id}"))?
            .filter(|l| l.locked_until_ms > now_ms)
            .map(|l| l.locked_until_ms))
    }

    // -- one-time links -----------------------------------------------------

    fn invite(&self, user: &str, now_ms: u64) -> Result<String> {
        let token = random_token();
        self.store.put_invite(
            &key_of(&token),
            &InviteRecord {
                user: user.to_string(),
                created_ms: now_ms,
                expires_ms: now_ms + INVITE_TTL_MS,
                used_ms: None,
                first_admin: false,
            },
        )?;
        Ok(token)
    }

    /// The account a live token is for, without spending it.
    pub fn invite_user(&self, token: &str, now_ms: u64) -> Result<Option<UserRecord>> {
        let Some(invite) = self.store.get_invite(&key_of(token))? else {
            return Ok(None);
        };
        if invite.used_ms.is_some() || invite.expires_ms <= now_ms {
            return Ok(None);
        }
        Ok(self.store.get_user(&invite.user)?.filter(|u| !u.disabled))
    }

    /// Spend a token on a first (or reset) password and sign the user in.
    pub fn set_password(
        &self,
        token: &str,
        password: &str,
        ip: &str,
        user_agent: &str,
        now_ms: u64,
    ) -> std::result::Result<SignedIn, String> {
        let user = self
            .invite_user(token, now_ms)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| {
                "This link has been used or has expired. Ask your administrator for a new one."
                    .to_string()
            })?;
        password_acceptable(password)?;
        let hash = hash_password(password).map_err(|e| format!("{e:#}"))?;
        let mut invite = self
            .store
            .get_invite(&key_of(token))
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| "This link has been used or has expired.".to_string())?;
        invite.used_ms = Some(now_ms);
        self.store
            .put_invite(&key_of(token), &invite)
            .map_err(|e| format!("{e:#}"))?;
        let mut user = user;
        user.password_hash = Some(hash);
        user.password_set_ms = Some(now_ms);
        user.last_login_ms = Some(now_ms);
        self.store.put_user(&user).map_err(|e| format!("{e:#}"))?;
        self.unlock(&user.id).map_err(|e| format!("{e:#}"))?;
        self.start_session(user, ip, user_agent, now_ms)
            .map_err(|e| format!("{e:#}"))
    }

    // -- signing in and out -------------------------------------------------

    /// A login attempt. `Err` is for the audit log; the user is told
    /// `LoginFailure::MESSAGE` whatever the reason.
    pub fn login(
        &self,
        email: &str,
        password: &str,
        ip: &str,
        user_agent: &str,
        now_ms: u64,
    ) -> Result<std::result::Result<SignedIn, LoginFailure>> {
        if let Some(until) = self.address_locked_until(ip, now_ms)? {
            return Ok(Err(LoginFailure::AddressLocked { until_ms: until }));
        }
        let email = normalise_email(email);
        let Some(user) = self.store.user_by_email(&email)? else {
            let _ = verify_password(password, decoy_hash());
            self.count_address_failure(ip, now_ms)?;
            return Ok(Err(LoginFailure::NoSuchUser));
        };
        if let Some(until) = self.account_locked_until(&user.id, now_ms)? {
            self.count_address_failure(ip, now_ms)?;
            return Ok(Err(LoginFailure::AccountLocked { until_ms: until }));
        }
        let outcome = match (&user.password_hash, user.disabled) {
            (_, true) => {
                let _ = verify_password(password, decoy_hash());
                Err(LoginFailure::Disabled)
            }
            (None, _) => {
                let _ = verify_password(password, decoy_hash());
                Err(LoginFailure::NoPasswordYet)
            }
            (Some(hash), false) if verify_password(password, hash) => Ok(()),
            (Some(_), false) => Err(LoginFailure::WrongPassword),
        };
        match outcome {
            Ok(()) => {
                self.unlock(&user.id)?;
                let mut user = user;
                user.last_login_ms = Some(now_ms);
                self.store.put_user(&user)?;
                Ok(Ok(self.start_session(user, ip, user_agent, now_ms)?))
            }
            Err(failure) => {
                self.count_address_failure(ip, now_ms)?;
                if matches!(failure, LoginFailure::WrongPassword) {
                    self.count_account_failure(&user.id, now_ms)?;
                }
                Ok(Err(failure))
            }
        }
    }

    fn start_session(
        &self,
        user: UserRecord,
        ip: &str,
        user_agent: &str,
        now_ms: u64,
    ) -> Result<SignedIn> {
        let session = random_token();
        let expires_ms = now_ms + self.session_ttl_ms;
        self.store.put_session(
            &key_of(&session),
            &SessionRecord {
                user: user.id.clone(),
                created_ms: now_ms,
                expires_ms,
                ip: ip.to_string(),
                user_agent: user_agent.chars().take(200).collect(),
                revoked: false,
            },
        )?;
        Ok(SignedIn {
            user,
            session,
            expires_ms,
        })
    }

    /// The user behind a live session id, if it is live: not revoked, not
    /// expired, the account not disabled.
    pub fn session(&self, id: &str, now_ms: u64) -> Result<Option<(UserRecord, SessionRecord)>> {
        let Some(session) = self.store.get_session(&key_of(id))? else {
            return Ok(None);
        };
        if session.revoked || session.expires_ms <= now_ms {
            return Ok(None);
        }
        let Some(user) = self.store.get_user(&session.user)? else {
            return Ok(None);
        };
        if user.disabled {
            return Ok(None);
        }
        Ok(Some((user, session)))
    }

    pub fn logout(&self, id: &str) -> Result<bool> {
        let key = key_of(id);
        let Some(mut session) = self.store.get_session(&key)? else {
            return Ok(false);
        };
        if session.revoked {
            return Ok(false);
        }
        session.revoked = true;
        self.store.put_session(&key, &session)?;
        Ok(true)
    }

    /// End every live session of a user. Returns how many were live.
    pub fn revoke_sessions(&self, user: &str) -> Result<usize> {
        let mut ended = 0;
        for (key, mut session) in self.store.sessions_of(user)? {
            if !session.revoked {
                session.revoked = true;
                self.store.put_session(&key, &session)?;
                ended += 1;
            }
        }
        Ok(ended)
    }

    // -- lockouts -----------------------------------------------------------

    fn address_locked_until(&self, ip: &str, now_ms: u64) -> Result<Option<u64>> {
        if ip.is_empty() {
            return Ok(None);
        }
        Ok(self
            .store
            .get_lock(&format!("ip:{ip}"))?
            .filter(|l| l.locked_until_ms > now_ms)
            .map(|l| l.locked_until_ms))
    }

    /// One more failure from an address; the thirtieth inside the window
    /// locks it.
    fn count_address_failure(&self, ip: &str, now_ms: u64) -> Result<()> {
        if ip.is_empty() {
            return Ok(());
        }
        let key = format!("ip:{ip}");
        let mut lock = self.store.get_lock(&key)?.unwrap_or_default();
        if now_ms.saturating_sub(lock.window_start_ms) > IP_WINDOW_MS {
            lock.window_start_ms = now_ms;
            lock.failures = 0;
        }
        lock.failures += 1;
        if lock.failures >= IP_FAILURES {
            lock.locked_until_ms = now_ms + IP_LOCK_MS;
            lock.failures = 0;
            lock.window_start_ms = now_ms;
            lock.rounds += 1;
        }
        self.store.put_lock(&key, &lock)
    }

    /// One more wrong password for an account; the fifth in a row locks it,
    /// for twice as long each round.
    fn count_account_failure(&self, id: &str, now_ms: u64) -> Result<()> {
        let key = format!("account:{id}");
        let mut lock = self.store.get_lock(&key)?.unwrap_or_default();
        lock.failures += 1;
        if lock.failures >= ACCOUNT_FAILURES {
            let factor = 1u64 << lock.rounds.min(10);
            lock.locked_until_ms = now_ms + ACCOUNT_LOCK_MS * factor;
            lock.failures = 0;
            lock.rounds += 1;
        }
        self.store.put_lock(&key, &lock)
    }

    /// Whether an account is locked right now, for the admin page.
    pub fn lock_of(&self, id: &str) -> Result<Option<LockRecord>> {
        self.store.get_lock(&format!("account:{id}"))
    }
}

/// The record as the API shows it: never the hash.
pub fn info(user: &UserRecord, locked_until_ms: Option<u64>) -> proto::UserInfo {
    proto::UserInfo {
        id: user.id.clone(),
        email: user.email.clone(),
        name: user.name.clone(),
        roles: user.roles.clone(),
        provider: user.provider.clone(),
        disabled: user.disabled,
        has_password: user.password_hash.is_some(),
        created_ms: user.created_ms,
        last_login_ms: user.last_login_ms,
        locked_until_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 60 * 60 * 1000;

    fn directory() -> Directory {
        Directory::new(Store::in_memory().unwrap(), 12 * HOUR)
    }

    #[test]
    fn a_new_account_sets_its_password_through_a_single_use_link_and_signs_in() {
        let d = directory();
        let (anna, token) = d
            .create_user(
                "Anna@Example.com",
                "Anna",
                vec![proto::UserRole::Member],
                1_000,
            )
            .unwrap();
        assert_eq!(anna.email, "anna@example.com", "lower-cased");
        assert!(anna.password_hash.is_none());
        assert!(
            d.create_user("anna@example.com", "Again", vec![], 1_000)
                .is_err(),
            "one account per email"
        );
        assert_eq!(d.invite_user(&token, 2_000).unwrap().unwrap().id, anna.id);

        assert!(
            d.set_password(&token, "short", "10.0.0.1", "test", 2_000)
                .is_err(),
            "twelve characters at least"
        );
        let signed = d
            .set_password(&token, "correct horse battery", "10.0.0.1", "test", 2_000)
            .unwrap();
        assert_eq!(signed.user.id, anna.id);
        assert_eq!(signed.expires_ms, 2_000 + 12 * HOUR);
        assert!(d.session(&signed.session, 3_000).unwrap().is_some());
        assert!(
            d.invite_user(&token, 3_000).unwrap().is_none(),
            "the link is spent"
        );
        assert!(
            d.set_password(&token, "correct horse battery", "", "", 3_000)
                .is_err()
        );

        // Signing in with the password, and the wrong one.
        let again = d
            .login(
                "anna@example.com",
                "correct horse battery",
                "10.0.0.1",
                "test",
                4_000,
            )
            .unwrap()
            .unwrap();
        assert_ne!(again.session, signed.session, "a session of its own");
        assert_eq!(
            d.login("anna@example.com", "wrong", "10.0.0.1", "test", 4_000)
                .unwrap()
                .unwrap_err(),
            LoginFailure::WrongPassword
        );
        assert_eq!(
            d.login("nobody@example.com", "whatever", "10.0.0.1", "test", 4_000)
                .unwrap()
                .unwrap_err(),
            LoginFailure::NoSuchUser
        );
    }

    #[test]
    fn a_link_dies_at_twenty_four_hours() {
        let d = directory();
        let (_, token) = d.create_user("a@b.c", "A", vec![], 0).unwrap();
        assert!(d.invite_user(&token, INVITE_TTL_MS - 1).unwrap().is_some());
        assert!(d.invite_user(&token, INVITE_TTL_MS).unwrap().is_none());
    }

    #[test]
    fn sessions_expire_hard_and_end_on_logout_reset_role_change_and_disable() {
        let d = directory();
        let (anna, token) = d
            .create_user("a@b.c", "A", vec![proto::UserRole::Member], 0)
            .unwrap();
        let s = d
            .set_password(&token, "correct horse battery", "", "", 0)
            .unwrap();
        assert!(d.session(&s.session, 12 * HOUR - 1).unwrap().is_some());
        assert!(
            d.session(&s.session, 12 * HOUR).unwrap().is_none(),
            "hard expiry"
        );

        let s = d
            .login("a@b.c", "correct horse battery", "", "", 0)
            .unwrap()
            .unwrap();
        assert!(d.logout(&s.session).unwrap());
        assert!(d.session(&s.session, 1).unwrap().is_none());
        assert!(!d.logout(&s.session).unwrap(), "already ended");

        let s = d
            .login("a@b.c", "correct horse battery", "", "", 0)
            .unwrap()
            .unwrap();
        d.set_roles(&anna.id, vec![proto::UserRole::Viewer])
            .unwrap();
        assert!(
            d.session(&s.session, 1).unwrap().is_none(),
            "a role change ends the session"
        );

        let s = d
            .login("a@b.c", "correct horse battery", "", "", 0)
            .unwrap()
            .unwrap();
        let new_token = d.reset_password(&anna.id, 0).unwrap();
        assert!(
            d.session(&s.session, 1).unwrap().is_none(),
            "a reset ends the session"
        );
        assert_eq!(
            d.login("a@b.c", "correct horse battery", "", "", 0)
                .unwrap()
                .unwrap_err(),
            LoginFailure::NoPasswordYet,
            "the old password is gone"
        );
        let s = d
            .set_password(&new_token, "another good password", "", "", 0)
            .unwrap();
        d.set_disabled(&anna.id, true).unwrap();
        assert!(d.session(&s.session, 1).unwrap().is_none(), "disabled");
        assert_eq!(
            d.login("a@b.c", "another good password", "", "", 0)
                .unwrap()
                .unwrap_err(),
            LoginFailure::Disabled
        );
    }

    #[test]
    fn five_wrong_passwords_lock_the_account_for_fifteen_minutes_doubling() {
        let d = directory();
        let (anna, token) = d.create_user("a@b.c", "A", vec![], 0).unwrap();
        d.set_password(&token, "correct horse battery", "", "", 0)
            .unwrap();
        for _ in 0..4 {
            assert_eq!(
                d.login("a@b.c", "wrong", "", "", 1_000)
                    .unwrap()
                    .unwrap_err(),
                LoginFailure::WrongPassword
            );
        }
        // The right password before the fifth failure clears the count.
        assert!(
            d.login("a@b.c", "correct horse battery", "", "", 1_000)
                .unwrap()
                .is_ok()
        );
        for _ in 0..5 {
            let _ = d.login("a@b.c", "wrong", "", "", 2_000).unwrap();
        }
        assert_eq!(
            d.login("a@b.c", "correct horse battery", "", "", 2_001)
                .unwrap()
                .unwrap_err(),
            LoginFailure::AccountLocked {
                until_ms: 2_000 + ACCOUNT_LOCK_MS
            },
            "locked, even with the right password"
        );
        assert_eq!(
            d.account_locked_until(&anna.id, 2_001).unwrap(),
            Some(2_000 + ACCOUNT_LOCK_MS)
        );
        // After the lock, the next round locks for twice as long.
        let later = 2_000 + ACCOUNT_LOCK_MS + 1;
        for _ in 0..5 {
            let _ = d.login("a@b.c", "wrong", "", "", later).unwrap();
        }
        assert_eq!(
            d.account_locked_until(&anna.id, later).unwrap(),
            Some(later + 2 * ACCOUNT_LOCK_MS)
        );
        // An admin clears it.
        d.unlock(&anna.id).unwrap();
        assert!(
            d.login("a@b.c", "correct horse battery", "", "", later)
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn thirty_failures_from_one_address_lock_the_address_whatever_the_account() {
        let d = directory();
        let (_, token) = d.create_user("a@b.c", "A", vec![], 0).unwrap();
        d.set_password(&token, "correct horse battery", "", "", 0)
            .unwrap();
        for i in 0..IP_FAILURES {
            let _ = d
                .login(&format!("nobody{i}@b.c"), "x", "10.0.0.9", "", 1_000)
                .unwrap();
        }
        assert_eq!(
            d.login("a@b.c", "correct horse battery", "10.0.0.9", "", 1_001)
                .unwrap()
                .unwrap_err(),
            LoginFailure::AddressLocked {
                until_ms: 1_000 + IP_LOCK_MS
            }
        );
        assert!(
            d.login("a@b.c", "correct horse battery", "10.0.0.10", "", 1_001)
                .unwrap()
                .is_ok(),
            "another address is not locked"
        );
        assert!(
            d.login(
                "a@b.c",
                "correct horse battery",
                "10.0.0.9",
                "",
                1_000 + IP_LOCK_MS + 1
            )
            .unwrap()
            .is_ok(),
            "the address is free again after the lock"
        );
    }

    #[test]
    fn the_wrong_email_and_the_wrong_password_say_the_same_thing() {
        assert_eq!(LoginFailure::NoSuchUser.label(), "no_such_user");
        assert_eq!(LoginFailure::MESSAGE, "That email or password isn't right.");
        assert!(!verify_password("pw", "not a hash"));
        let hash = hash_password("a password of length").unwrap();
        assert!(
            hash.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"),
            "{hash}"
        );
        assert!(verify_password("a password of length", &hash));
        assert!(!verify_password("a password of lengtH", &hash));
    }
}

#[cfg(test)]
mod password_rules {
    use super::*;

    #[test]
    fn twelve_characters_at_least_two_hundred_and_fifty_six_at_most_and_nothing_common() {
        assert!(password_acceptable("elevenchars").is_err());
        assert!(password_acceptable("twelve chars").is_ok(), "spaces count");
        assert!(
            password_acceptable("пароль из кириллицы").is_ok(),
            "any Unicode counts by character"
        );
        assert!(password_acceptable("🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑").is_ok());
        let sixty_four =
            "a phrase with spaces, punctuation and ünïcödé that runs long!".to_string() + "!!!";
        assert_eq!(sixty_four.chars().count(), 64);
        assert!(password_acceptable(&sixty_four).is_ok());
        assert!(password_acceptable(&"x".repeat(256)).is_ok());
        assert!(password_acceptable(&"x".repeat(257)).is_err());
        for common in [
            "password1234",
            "Password1234",
            "correcthorsebatterystaple",
            "qwertyuiop123",
            "letmeinletmein",
        ] {
            assert_eq!(
                password_acceptable(common).unwrap_err(),
                "That password is too common. Choose another one.",
                "{common}"
            );
        }
        assert!(
            COMMON_PASSWORDS.len() >= 3000,
            "{} entries",
            COMMON_PASSWORDS.len()
        );
    }

    #[test]
    fn a_long_password_is_never_truncated() {
        let long = "a".repeat(70);
        let hash = hash_password(&long).unwrap();
        assert!(verify_password(&long, &hash));
        assert!(
            !verify_password(&long[..64], &hash),
            "the first 64 characters are not the password"
        );
        assert!(!verify_password(&long[..69], &hash));
    }
}
