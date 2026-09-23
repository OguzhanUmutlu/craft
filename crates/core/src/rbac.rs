use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    SuperAdmin,
    ServerOperator,
    BackupAuditor,
    Viewer,
}

impl Role {
    pub fn name(&self) -> &'static str {
        match self {
            Role::SuperAdmin => "SuperAdmin",
            Role::ServerOperator => "ServerOperator",
            Role::BackupAuditor => "BackupAuditor",
            Role::Viewer => "Viewer",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "superadmin" | "admin" | "root" => Some(Role::SuperAdmin),
            "serveroperator" | "operator" | "op" => Some(Role::ServerOperator),
            "backupauditor" | "auditor" | "backup" => Some(Role::BackupAuditor),
            "viewer" | "view" | "read" | "readonly" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn has_permission(&self, perm: Permission) -> bool {
        match self {
            Role::SuperAdmin => true,
            Role::ServerOperator => matches!(
                perm,
                Permission::ServerStart
                    | Permission::ServerStop
                    | Permission::ServerRestart
                    | Permission::ServerFixForce
                    | Permission::ServerConsoleView
                    | Permission::ServerConsoleInput
                    | Permission::BackupCreate
                    | Permission::FileBrowse
                    | Permission::FileEdit
            ),
            Role::BackupAuditor => matches!(
                perm,
                Permission::BackupCreate
                    | Permission::BackupRestore
                    | Permission::BackupDelete
                    | Permission::AuditLogView
                    | Permission::FileBrowse
            ),
            Role::Viewer => matches!(
                perm,
                Permission::ServerConsoleView | Permission::FileBrowse
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ServerStart,
    ServerStop,
    ServerRestart,
    ServerDelete,
    ServerFixForce,
    ServerConsoleView,
    ServerConsoleInput,
    BackupCreate,
    BackupRestore,
    BackupDelete,
    FileBrowse,
    FileEdit,
    FileDelete,
    AuditLogView,
    UserManage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAccount {
    pub username: String,
    pub salt_hex: String,
    pub password_hash_hex: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_servers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
}

impl UserAccount {
    pub fn new(
        username: impl Into<String>,
        password: &str,
        role: Role,
        assigned_servers: Option<Vec<String>>,
    ) -> Self {
        let salt = generate_salt();
        let salt_hex = hex::encode(&salt);
        let password_hash_hex = hash_password(password, &salt);

        Self {
            username: username.into(),
            salt_hex,
            password_hash_hex,
            role,
            assigned_servers,
            totp_secret: None,
            disabled: false,
            created_at: Utc::now(),
        }
    }

    pub fn verify_password(&self, password: &str) -> bool {
        if self.disabled {
            return false;
        }
        let salt = match hex::decode(&self.salt_hex) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let computed = hash_password(password, &salt);
        computed == self.password_hash_hex
    }

    pub fn can_access_server(&self, server_name: &str) -> bool {
        if self.role == Role::SuperAdmin {
            return true;
        }
        match &self.assigned_servers {
            None => true,
            Some(servers) => servers
                .iter()
                .any(|s| s == "*" || s.eq_ignore_ascii_case(server_name)),
        }
    }

    pub fn has_permission(&self, perm: Permission, server_name: Option<&str>) -> bool {
        if self.disabled {
            return false;
        }
        if !self.role.has_permission(perm) {
            return false;
        }
        if let Some(srv) = server_name {
            if !self.can_access_server(srv) {
                return false;
            }
        }
        true
    }
}

pub fn generate_salt() -> [u8; 16] {
    let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let pid = std::process::id();
    let mut hasher = Sha256::new();
    hasher.update(now.to_le_bytes());
    hasher.update(pid.to_le_bytes());
    let res = hasher.finalize();
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&res[..16]);
    salt
}

pub fn hash_password(password: &str, salt: &[u8]) -> String {
    let mut current = {
        let mut h = Sha256::new();
        h.update(salt);
        h.update(password.as_bytes());
        h.finalize().to_vec()
    };

    // Key stretching: 1,000 rounds of Sha256(current + salt)
    for _ in 0..1000 {
        let mut h = Sha256::new();
        h.update(&current);
        h.update(salt);
        current = h.finalize().to_vec();
    }

    hex::encode(current)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RbacRegistry {
    #[serde(default)]
    pub users: Vec<UserAccount>,
}

impl RbacRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.rbac_file.exists() {
            let content = fs::read_to_string(&paths.rbac_file)?;
            let reg: RbacRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse rbac.toml: {}", e)))?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    pub fn load_or_init(paths: &CraftPaths) -> Result<Self> {
        let mut reg = Self::load(paths)?;
        if reg.users.is_empty() {
            // Create default admin user
            let default_admin = UserAccount::new("admin", "admin", Role::SuperAdmin, None);
            reg.users.push(default_admin);
            reg.save(paths)?;
        }
        Ok(reg)
    }

    fn save_internal(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize rbac.toml: {}", e)))?;
        let temp_path = paths.rbac_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(temp_path, &paths.rbac_file)?;
        Ok(())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_file_path = paths.locks_dir.join("rbac.lock");
        if let Some(parent) = lock_file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;
        let res = self.save_internal(paths);
        let _ = lock_file.unlock();
        res
    }

    pub fn get_user(&self, username: &str) -> Option<&UserAccount> {
        self.users
            .iter()
            .find(|u| u.username.eq_ignore_ascii_case(username))
    }

    pub fn get_user_mut(&mut self, username: &str) -> Option<&mut UserAccount> {
        self.users
            .iter_mut()
            .find(|u| u.username.eq_ignore_ascii_case(username))
    }

    pub fn authenticate(&self, username: &str, password: &str) -> Option<&UserAccount> {
        let user = self.get_user(username)?;
        if user.verify_password(password) {
            Some(user)
        } else {
            None
        }
    }

    pub fn add_user(&mut self, user: UserAccount) -> Result<()> {
        if self.get_user(&user.username).is_some() {
            return Err(CraftError::Other(format!(
                "User '{}' already exists",
                user.username
            )));
        }
        self.users.push(user);
        Ok(())
    }

    pub fn remove_user(&mut self, username: &str) -> Result<()> {
        let initial_len = self.users.len();
        self.users
            .retain(|u| !u.username.eq_ignore_ascii_case(username));
        if self.users.len() == initial_len {
            return Err(CraftError::Other(format!(
                "User '{}' does not exist",
                username
            )));
        }
        Ok(())
    }

    pub fn update_password(&mut self, username: &str, new_password: &str) -> Result<()> {
        let user = self.get_user_mut(username).ok_or_else(|| {
            CraftError::Other(format!("User '{}' does not exist", username))
        })?;
        let salt = generate_salt();
        user.salt_hex = hex::encode(&salt);
        user.password_hash_hex = hash_password(new_password, &salt);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_password_verification() {
        let user = UserAccount::new("operator1", "SecretPass123!", Role::ServerOperator, None);
        assert!(user.verify_password("SecretPass123!"));
        assert!(!user.verify_password("WrongPassword"));
        assert_eq!(user.role, Role::ServerOperator);
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::SuperAdmin.has_permission(Permission::UserManage));
        assert!(Role::SuperAdmin.has_permission(Permission::ServerDelete));

        assert!(!Role::ServerOperator.has_permission(Permission::UserManage));
        assert!(Role::ServerOperator.has_permission(Permission::ServerStart));
        assert!(Role::ServerOperator.has_permission(Permission::ServerConsoleInput));

        assert!(Role::BackupAuditor.has_permission(Permission::AuditLogView));
        assert!(!Role::BackupAuditor.has_permission(Permission::ServerStart));

        assert!(Role::Viewer.has_permission(Permission::ServerConsoleView));
        assert!(!Role::Viewer.has_permission(Permission::ServerConsoleInput));
    }

    #[test]
    fn test_server_scope_restrictions() {
        let scoped_user = UserAccount::new(
            "mod1",
            "pass",
            Role::ServerOperator,
            Some(vec!["lobby".to_string(), "survival".to_string()]),
        );

        assert!(scoped_user.has_permission(Permission::ServerStart, Some("lobby")));
        assert!(scoped_user.has_permission(Permission::ServerStart, Some("survival")));
        assert!(!scoped_user.has_permission(Permission::ServerStart, Some("creative")));
    }

    #[test]
    fn test_rbac_registry_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());

        let mut reg = RbacRegistry::load_or_init(&paths).unwrap();
        assert_eq!(reg.users.len(), 1);
        assert_eq!(reg.users[0].username, "admin");

        let op = UserAccount::new("op", "op123", Role::ServerOperator, None);
        reg.add_user(op).unwrap();
        reg.save(&paths).unwrap();

        let loaded = RbacRegistry::load(&paths).unwrap();
        assert_eq!(loaded.users.len(), 2);
        assert!(loaded.authenticate("op", "op123").is_some());
        assert!(loaded.authenticate("op", "bad").is_none());
    }
}
