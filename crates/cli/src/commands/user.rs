use crate::cli::UserCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    AuditLedger, CraftError, CraftPaths, RbacRegistry, Result, Role, UserAccount,
    DEFAULT_AUDIT_SECRET,
};

pub fn handle_user(action: UserCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        UserCommands::Add {
            username,
            role,
            password,
            servers,
        } => handle_add(&username, &role, password, servers, paths),
        UserCommands::Ls => handle_list(paths),
        UserCommands::Rm { username } => handle_remove(&username, paths),
        UserCommands::Passwd { username, password } => {
            handle_passwd(&username, password, paths)
        }
    }
}

fn parse_role(role_str: &str) -> Result<Role> {
    match role_str.trim().to_lowercase().as_str() {
        "superadmin" | "admin" => Ok(Role::SuperAdmin),
        "serveroperator" | "operator" | "op" => Ok(Role::ServerOperator),
        "backupauditor" | "auditor" | "backup" => Ok(Role::BackupAuditor),
        "viewer" | "view" | "read" => Ok(Role::Viewer),
        other => Err(CraftError::Other(format!(
            "Invalid role '{}'. Available roles: SuperAdmin, ServerOperator, BackupAuditor, Viewer",
            other
        ))),
    }
}

fn handle_add(
    username: &str,
    role_str: &str,
    password_opt: Option<String>,
    servers: Vec<String>,
    paths: &CraftPaths,
) -> Result<()> {
    let username = username.trim();
    if username.is_empty() {
        return Err(CraftError::Other("Username cannot be empty".to_string()));
    }

    let role = parse_role(role_str)?;

    let pwd = match password_opt {
        Some(p) if !p.trim().is_empty() => p,
        _ => {
            let prompt = format!("Enter password for user '{}'", username);
            dialoguer::Password::new()
                .with_prompt(prompt)
                .interact()
                .map_err(|e| CraftError::Other(format!("Failed to read password: {}", e)))?
        }
    };

    if pwd.trim().is_empty() {
        return Err(CraftError::Other("Password cannot be empty".to_string()));
    }

    let assigned_servers = if servers.is_empty() {
        None
    } else {
        Some(servers)
    };

    let user = UserAccount::new(username, &pwd, role, assigned_servers.clone());
    let mut registry = RbacRegistry::load_or_init(paths)?;
    registry.add_user(user)?;
    registry.save(paths)?;

    let _ = AuditLedger::append(
        paths,
        "cli",
        "system",
        "user_create",
        Some(username.to_string()),
        None,
        "success",
        Some(format!(
            "Role: {:?}, Assigned servers: {:?}",
            role, assigned_servers
        )),
        DEFAULT_AUDIT_SECRET,
    );

    println!(
        "{} User '{}' created successfully with role {:?}.",
        "[OK]".green().bold(),
        username,
        role
    );
    Ok(())
}

fn handle_list(paths: &CraftPaths) -> Result<()> {
    let registry = RbacRegistry::load_or_init(paths)?;

    if registry.users.is_empty() {
        println!("{}", "No registered users found in rbac.toml.".dimmed());
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("Username").fg(Color::Cyan),
            Cell::new("Role").fg(Color::Yellow),
            Cell::new("Assigned Servers").fg(Color::Blue),
            Cell::new("Disabled").fg(Color::Magenta),
            Cell::new("Created At").fg(Color::White),
        ]));

    for user in &registry.users {
        let role_cell = match user.role {
            Role::SuperAdmin => Cell::new("SuperAdmin").fg(Color::Magenta),
            Role::ServerOperator => Cell::new("ServerOperator").fg(Color::Cyan),
            Role::BackupAuditor => Cell::new("BackupAuditor").fg(Color::Yellow),
            Role::Viewer => Cell::new("Viewer").fg(Color::Blue),
        };

        let servers_cell = match &user.assigned_servers {
            None => Cell::new("All (*)").fg(Color::Green),
            Some(srvs) if srvs.is_empty() => Cell::new("All (*)").fg(Color::Green),
            Some(srvs) => Cell::new(srvs.join(", ")),
        };

        let disabled_cell = if user.disabled {
            Cell::new("Yes").fg(Color::Red)
        } else {
            Cell::new("No").fg(Color::Green)
        };

        let created_str = user.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string();

        table.add_row(Row::from(vec![
            Cell::new(&user.username).fg(Color::White),
            role_cell,
            servers_cell,
            disabled_cell,
            Cell::new(created_str),
        ]));
    }

    println!("\n{}", "Registered User Accounts:".bold());
    println!("{}", table);
    Ok(())
}

fn handle_remove(username: &str, paths: &CraftPaths) -> Result<()> {
    let username = username.trim();
    if username.is_empty() {
        return Err(CraftError::Other("Username cannot be empty".to_string()));
    }

    let mut registry = RbacRegistry::load_or_init(paths)?;
    registry.remove_user(username)?;
    registry.save(paths)?;

    let _ = AuditLedger::append(
        paths,
        "cli",
        "system",
        "user_delete",
        Some(username.to_string()),
        None,
        "success",
        Some(format!("Removed user '{}'", username)),
        DEFAULT_AUDIT_SECRET,
    );

    println!(
        "{} User '{}' removed successfully.",
        "[OK]".green().bold(),
        username
    );
    Ok(())
}

fn handle_passwd(username: &str, password_opt: Option<String>, paths: &CraftPaths) -> Result<()> {
    let username = username.trim();
    if username.is_empty() {
        return Err(CraftError::Other("Username cannot be empty".to_string()));
    }

    let mut registry = RbacRegistry::load_or_init(paths)?;
    if registry.get_user(username).is_none() {
        return Err(CraftError::Other(format!("User '{}' not found", username)));
    }

    let pwd = match password_opt {
        Some(p) if !p.trim().is_empty() => p,
        _ => {
            let prompt = format!("Enter new password for user '{}'", username);
            dialoguer::Password::new()
                .with_prompt(prompt)
                .interact()
                .map_err(|e| CraftError::Other(format!("Failed to read password: {}", e)))?
        }
    };

    if pwd.trim().is_empty() {
        return Err(CraftError::Other("Password cannot be empty".to_string()));
    }

    registry.update_password(username, &pwd)?;
    registry.save(paths)?;

    let _ = AuditLedger::append(
        paths,
        "cli",
        "system",
        "user_password_update",
        Some(username.to_string()),
        None,
        "success",
        Some(format!("Updated password for user '{}'", username)),
        DEFAULT_AUDIT_SECRET,
    );

    println!(
        "{} Password for user '{}' updated successfully.",
        "[OK]".green().bold(),
        username
    );
    Ok(())
}
