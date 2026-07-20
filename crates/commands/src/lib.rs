//! In-game command parser and built-in commands.
//!
//! Parses Bedrock's structured command syntax and provides a registry
//! of built-in commands (gamemode, tp, give, kick, say, list, stop, etc.).

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Command-related errors
#[derive(Error, Debug)]
pub enum CommandError {
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("player not found: {0}")]
    PlayerNotFound(String),
}

/// Command sender type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSender {
    Player(i64),
    Console,
    CommandBlock,
}

impl CommandSender {
    pub fn is_player(&self) -> bool {
        matches!(self, CommandSender::Player(_))
    }

    pub fn is_console(&self) -> bool {
        matches!(self, CommandSender::Console)
    }

    pub fn player_id(&self) -> Option<i64> {
        match self {
            CommandSender::Player(id) => Some(*id),
            _ => None,
        }
    }
}

/// Command execution context
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub sender: CommandSender,
    pub args: Vec<String>,
    pub label: String,
}

impl CommandContext {
    pub fn new(sender: CommandSender, label: String, args: Vec<String>) -> Self {
        Self {
            sender,
            args,
            label,
        }
    }

    /// Get player name from sender
    pub fn player_name(&self, fallback: &str) -> String {
        match self.sender {
            CommandSender::Player(_) => fallback.to_string(),
            CommandSender::Console => "Console".to_string(),
            CommandSender::CommandBlock => "CommandBlock".to_string(),
        }
    }
}

/// Command output message type
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub message: String,
    pub sender_id: Option<i64>,
}

impl CommandOutput {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            sender_id: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            sender_id: None,
        }
    }

    pub fn with_sender(mut self, sender_id: i64) -> Self {
        self.sender_id = Some(sender_id);
        self
    }
}

/// Command execution result
pub type CommandResult = Result<CommandOutput, CommandError>;

/// Command signature for registration
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub permission: CommandPermission,
    pub aliases: Vec<String>,
}

impl CommandInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            usage: String::new(),
            permission: CommandPermission::Everyone,
            aliases: Vec::new(),
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn usage(mut self, usage: impl Into<String>) -> Self {
        self.usage = usage.into();
        self
    }

    pub fn permission(mut self, perm: CommandPermission) -> Self {
        self.permission = perm;
        self
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.push(alias.into());
        self
    }
}

/// Permission levels for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandPermission {
    Everyone = 0,
    GameMaster = 1,
    Admin = 2,
    Console = 3,
}

impl CommandPermission {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Everyone,
            1 => Self::GameMaster,
            2 => Self::Admin,
            _ => Self::Console,
        }
    }
}

/// Command executor trait
pub trait CommandExecutor: Send + Sync {
    fn execute(&self, ctx: &CommandContext) -> CommandResult;
}

/// Built-in command implementations
pub mod builtins {
    use super::*;

    /// Stop command - shuts down the server
    pub fn stop(_ctx: &CommandContext) -> CommandResult {
        Ok(CommandOutput::success("Stopping server..."))
    }

    /// List players command
    pub fn list_players(ctx: &CommandContext, players: &[String]) -> CommandResult {
        let player_list = if players.is_empty() {
            "No players online".to_string()
        } else {
            format!("{} player(s) online: {}", players.len(), players.join(", "))
        };
        
        Ok(CommandOutput::success(player_list)
            .with_sender(ctx.sender.player_id().unwrap_or(-1)))
    }

    /// Say command - broadcast message to all players
    pub fn say(_message: &str) -> CommandResult {
        Ok(CommandOutput::success("Message broadcast"))
    }

    /// Kick player command
    pub fn kick_player(target: &str, reason: &str) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Kicked player '{}' for: {}",
            target,
            if reason.is_empty() { "No reason given" } else { reason }
        )))
    }

    /// Teleport command
    pub fn teleport(target: &str, x: f64, y: f64, z: f64) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Teleported {} to ({}, {}, {})",
            target, x, y, z
        )))
    }

    /// Gamemode command
    pub fn gamemode(mode: &str, target: &str) -> CommandResult {
        let valid_modes = ["survival", "creative", "adventure", "spectator"];
        if !valid_modes.contains(&mode.to_lowercase().as_str()) {
            return Ok(CommandOutput::error(format!(
                "Invalid gamemode '{}'. Valid: {}",
                mode,
                valid_modes.join(", ")
            )));
        }
        
        Ok(CommandOutput::success(format!(
            "Set gamemode of {} to {}",
            if target.is_empty() { "yourself" } else { target },
            mode
        )))
    }

    /// Give item command
    pub fn give(target: &str, item: &str, amount: u32) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Gave {} {} {} to {}",
            amount,
            item,
            if amount == 1 { "" } else { "items" },
            if target.is_empty() { "yourself" } else { target }
        )))
    }

    /// Time set command
    pub fn time_set(time: u32) -> CommandResult {
        Ok(CommandOutput::success(format!("Set time to {}", time)))
    }

    /// Difficulty command
    pub fn difficulty(level: &str) -> CommandResult {
        let valid = ["peaceful", "easy", "normal", "hard"];
        if !valid.contains(&level.to_lowercase().as_str()) {
            return Ok(CommandOutput::error(format!(
                "Invalid difficulty '{}'. Valid: {}",
                level,
                valid.join(", ")
            )));
        }
        
        Ok(CommandOutput::success(format!("Set difficulty to {}", level)))
    }

    /// Weather command
    pub fn weather(weather_type: &str) -> CommandResult {
        let valid = ["clear", "rain", "thunder"];
        if !valid.contains(&weather_type.to_lowercase().as_str()) {
            return Ok(CommandOutput::error(format!(
                "Invalid weather '{}'. Valid: {}",
                weather_type,
                valid.join(", ")
            )));
        }
        
        Ok(CommandOutput::success(format!("Set weather to {}", weather_type)))
    }

    /// Spawn entity command
    pub fn spawn_entity(entity_type: &str, x: f64, y: f64, z: f64) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Spawned {} at ({}, {}, {})",
            entity_type, x, y, z
        )))
    }

    /// Kill command
    pub fn kill(target: &str) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Killed {}",
            if target.is_empty() { "yourself" } else { target }
        )))
    }

    /// Heal command
    pub fn heal(target: &str, amount: f32) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Healed {} for {} hearts",
            if target.is_empty() { "yourself" } else { target },
            amount
        )))
    }

    /// Feed command (hunger)
    pub fn feed(target: &str, amount: u32) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Fed {} with {} points",
            if target.is_empty() { "yourself" } else { target },
            amount
        )))
    }

    /// Set spawn point command
    pub fn set_spawn(x: i32, y: i32, z: i32) -> CommandResult {
        Ok(CommandOutput::success(format!("Set spawn point to ({}, {}, {})", x, y, z)))
    }

    /// Clear inventory command
    pub fn clear_inventory(target: &str) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Cleared inventory of {}",
            if target.is_empty() { "yourself" } else { target }
        )))
    }

    /// Effect command
    pub fn effect(target: &str, effect: &str, duration: u32) -> CommandResult {
        Ok(CommandOutput::success(format!(
            "Gave {} {} effect for {} seconds",
            if target.is_empty() { "yourself" } else { target },
            effect,
            duration
        )))
    }
}

/// Command parser
pub struct CommandParser;

impl CommandParser {
    /// Parse a raw command string into label and arguments
    pub fn parse(raw: &str) -> (String, Vec<String>) {
        let raw = raw.trim();
        if raw.is_empty() {
            return (String::new(), Vec::new());
        }

        // Handle slash prefix
        let content = raw.strip_prefix('/').unwrap_or(raw);
        
        // Split by whitespace, preserving quoted strings
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut quote_char = '"';
        
        for ch in content.chars() {
            match ch {
                '"' | '\'' if !in_quotes => {
                    in_quotes = true;
                    quote_char = ch;
                }
                '"' | '\'' if ch == quote_char => {
                    in_quotes = false;
                }
                ' ' | '\t' if !in_quotes => {
                    if !current.is_empty() {
                        parts.push(current.clone());
                        current.clear();
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        
        if !current.is_empty() {
            parts.push(current);
        }

        if parts.is_empty() {
            return (String::new(), Vec::new());
        }

        let label = parts[0].clone();
        let args = parts[1..].to_vec();
        
        (label, args)
    }

    /// Get a coordinate from argument (handles relative ~ notation)
    pub fn parse_coord(arg: &str, current: f64) -> Option<f64> {
        if arg.is_empty() {
            return Some(current);
        }

        if arg == "~" {
            return Some(current);
        }

        if let Some(rest) = arg.strip_prefix('~') {
            let base = if rest.is_empty() { 0.0 } else { rest.parse().ok()? };
            return Some(current + base);
        }

        // Try absolute coordinate
        arg.parse().ok()
    }

    /// Parse a player name argument, returning None for "self" or empty
    pub fn parse_target(arg: &str) -> Option<String> {
        let trimmed = arg.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("self") || trimmed == "@s" {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

/// Command registry
pub struct CommandRegistry {
    commands: HashMap<String, Arc<dyn CommandExecutor>>,
    info: HashMap<String, CommandInfo>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
            info: HashMap::new(),
        };
        
        // Register built-in commands
        registry.register_builtin_commands();
        
        registry
    }

    fn register_builtin_commands(&mut self) {
        // Stop command
        self.register(
            CommandInfo::new("stop")
                .description("Stop the server")
                .usage("/stop")
                .permission(CommandPermission::Admin),
            Arc::new(|ctx: &CommandContext| builtins::stop(ctx)),
        );

        // List command
        self.register(
            CommandInfo::new("list")
                .description("List online players")
                .usage("/list")
                .permission(CommandPermission::Everyone),
            Arc::new(|_ctx: &CommandContext| builtins::list_players(_ctx, &[])),
        );

        // Say command
        self.register(
            CommandInfo::new("say")
                .description("Broadcast a message to all players")
                .usage("/say <message>")
                .permission(CommandPermission::Admin),
            Arc::new(|_ctx: &CommandContext| {
                let args = &_ctx.args.join(" ");
                builtins::say(args)
            }),
        );

        // Kick command
        self.register(
            CommandInfo::new("kick")
                .description("Kick a player from the server")
                .usage("/kick <player> [reason]")
                .permission(CommandPermission::Admin),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let reason = ctx.args.get(1..).map(|v| v.join(" ").as_str()).unwrap_or("");
                builtins::kick_player(target, reason)
            }),
        );

        // Gamemode command
        self.register(
            CommandInfo::new("gamemode")
                .alias("gm")
                .description("Change player gamemode")
                .usage("/gamemode <mode> [player]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let mode = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let target = ctx.args.get(1).map(|s| s.as_str()).unwrap_or("");
                builtins::gamemode(mode, target)
            }),
        );

        // Give command
        self.register(
            CommandInfo::new("give")
                .description("Give an item to a player")
                .usage("/give <player> <item> [amount]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let item = ctx.args.get(1).map(|s| s.as_str()).unwrap_or("stone");
                let amount: u32 = ctx.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
                builtins::give(target, item, amount)
            }),
        );

        // Time command
        self.register(
            CommandInfo::new("time")
                .description("Set or query the time")
                .usage("/time set <value>")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let time: u32 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6000);
                builtins::time_set(time)
            }),
        );

        // Difficulty command
        self.register(
            CommandInfo::new("difficulty")
                .alias("difficulty")
                .description("Set game difficulty")
                .usage("/difficulty <peaceful|easy|normal|hard>")
                .permission(CommandPermission::Admin),
            Arc::new(|ctx: &CommandContext| {
                let level = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("normal");
                builtins::difficulty(level)
            }),
        );

        // Weather command
        self.register(
            CommandInfo::new("weather")
                .description("Set weather")
                .usage("/weather <clear|rain|thunder> [duration]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let weather = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("clear");
                builtins::weather(weather)
            }),
        );

        // Kill command
        self.register(
            CommandInfo::new("kill")
                .description("Kill entities/players")
                .usage("/kill [player]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                builtins::kill(target)
            }),
        );

        // Heal command
        self.register(
            CommandInfo::new("heal")
                .description("Heal a player")
                .usage("/heal [player] [amount]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let amount: f32 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20.0);
                builtins::heal(target, amount)
            }),
        );

        // Feed command
        self.register(
            CommandInfo::new("feed")
                .description("Feed a player")
                .usage("/feed [player] [amount]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let amount: u32 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
                builtins::feed(target, amount)
            }),
        );

        // Spawn point command
        self.register(
            CommandInfo::new("setworldspawn")
                .description("Set the world spawn point")
                .usage("/setworldspawn [x] [y] [z]")
                .permission(CommandPermission::Admin),
            Arc::new(|ctx: &CommandContext| {
                let x: i32 = ctx.args.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
                let y: i32 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(64);
                let z: i32 = ctx.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                builtins::set_spawn(x, y, z)
            }),
        );

        // Clear command
        self.register(
            CommandInfo::new("clear")
                .description("Clear player inventory")
                .usage("/clear [player]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                builtins::clear_inventory(target)
            }),
        );

        // Effect command
        self.register(
            CommandInfo::new("effect")
                .description("Give/take effects")
                .usage("/effect <player> <effect> [duration] [amplifier]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let effect = ctx.args.get(1).map(|s| s.as_str()).unwrap_or("speed");
                let duration: u32 = ctx.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
                builtins::effect(target, effect, duration)
            }),
        );

        // Teleport command
        self.register(
            CommandInfo::new("tp")
                .alias("teleport")
                .description("Teleport players/entities")
                .usage("/tp [player] <x> <y> <z>")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let target = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("");
                let x: f64 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let y: f64 = ctx.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64.0);
                let z: f64 = ctx.args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                builtins::teleport(target, x, y, z)
            }),
        );

        // Spawn entity command
        self.register(
            CommandInfo::new("summon")
                .description("Summon an entity")
                .usage("/summon <entity> [x] [y] [z]")
                .permission(CommandPermission::GameMaster),
            Arc::new(|ctx: &CommandContext| {
                let entity = ctx.args.get(0).map(|s| s.as_str()).unwrap_or("pig");
                let x: f64 = ctx.args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let y: f64 = ctx.args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64.0);
                let z: f64 = ctx.args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                builtins::spawn_entity(entity, x, y, z)
            }),
        );
    }

    /// Register a command
    pub fn register(&mut self, info: CommandInfo, executor: Arc<dyn CommandExecutor>) {
        let name = info.name.clone();
        self.commands.insert(name.clone(), executor);
        self.info.insert(name, info);
    }

    /// Execute a command
    pub fn execute(&self, ctx: &CommandContext) -> CommandResult {
        let label = ctx.label.to_lowercase();
        
        let executor = self.commands.get(&label)
            .or_else(|| {
                // Try aliases
                for info in self.info.values() {
                    if info.aliases.iter().any(|a| a.to_lowercase() == label) {
                        return self.commands.get(&info.name);
                    }
                }
                None
            });

        let executor = executor.ok_or_else(|| CommandError::UnknownCommand(ctx.label.clone()))?;

        // Check permission
        let info = self.info.get(&label);
        if let Some(info) = info {
            if info.permission > CommandPermission::Everyone && !ctx.sender.is_console() {
                // Permission check would go here
            }
        }

        executor.execute(ctx)
    }

    /// Get command info
    pub fn get_info(&self, name: &str) -> Option<&CommandInfo> {
        self.info.get(name)
    }

    /// Get all registered commands
    pub fn commands(&self) -> Vec<&CommandInfo> {
        self.info.values().collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Command manager wrapping the registry with shared state
pub struct CommandManager {
    registry: Arc<RwLock<CommandRegistry>>,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(CommandRegistry::new())),
        }
    }

    /// Execute a raw command string
    pub async fn execute_raw(&self, raw: &str, sender: CommandSender) -> CommandResult {
        let (label, args) = CommandParser::parse(raw);
        
        if label.is_empty() {
            return Err(CommandError::InvalidArguments("Empty command".to_string()));
        }

        let ctx = CommandContext::new(sender, label.clone(), args);
        let registry = self.registry.read().await;
        registry.execute(&ctx)
    }

    /// Execute with a pre-built context
    pub async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let registry = self.registry.read().await;
        registry.execute(&ctx)
    }

    /// Register a new command
    pub async fn register(&self, info: CommandInfo, executor: Arc<dyn CommandExecutor>) {
        let mut registry = self.registry.write().await;
        registry.register(info, executor);
    }

    /// Get all commands
    pub async fn commands(&self) -> Vec<CommandInfo> {
        let registry = self.registry.read().await;
        registry.commands().iter().map(|i| (*i).clone()).collect()
    }
}

impl Default for CommandManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let (label, args) = CommandParser::parse("/test arg1 arg2");
        assert_eq!(label, "test");
        assert_eq!(args, vec!["arg1", "arg2"]);
    }

    #[test]
    fn test_parse_no_slash() {
        let (label, args) = CommandParser::parse("help me");
        assert_eq!(label, "help");
        assert_eq!(args, vec!["me"]);
    }

    #[test]
    fn test_parse_quoted() {
        let (label, args) = CommandParser::parse("/say \"Hello World\"");
        assert_eq!(label, "say");
        assert_eq!(args, vec!["Hello World"]);
    }

    #[test]
    fn test_parse_relative_coord() {
        assert_eq!(CommandParser::parse_coord("~", 5.0), Some(5.0));
        assert_eq!(CommandParser::parse_coord("~5", 10.0), Some(15.0));
        assert_eq!(CommandParser::parse_coord("~-2", 10.0), Some(8.0));
        assert_eq!(CommandParser::parse_coord("100", 0.0), Some(100.0));
    }

    #[test]
    fn test_parse_target() {
        assert_eq!(CommandParser::parse_target(""), None);
        assert_eq!(CommandParser::parse_target("@s"), None);
        assert_eq!(CommandParser::parse_target("self"), None);
        assert_eq!(CommandParser::parse_target("Steve"), Some("Steve".to_string()));
    }

    #[tokio::test]
    async fn test_execute_stop() {
        let manager = CommandManager::new();
        let result = manager.execute_raw("/stop", CommandSender::Console).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[tokio::test]
    async fn test_execute_unknown() {
        let manager = CommandManager::new();
        let result = manager.execute_raw("/unknown_command", CommandSender::Console).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gamemode_command() {
        let manager = CommandManager::new();
        
        // Valid gamemode
        let result = manager.execute_raw("/gamemode creative", CommandSender::Player(1)).await;
        assert!(result.is_ok());
        
        // Invalid gamemode
        let result = manager.execute_raw("/gamemode invalid", CommandSender::Player(1)).await;
        assert!(result.is_ok()); // Should return error output, not fail
        assert!(!result.unwrap().success);
    }

    #[test]
    fn test_command_context() {
        let ctx = CommandContext::new(
            CommandSender::Player(123),
            "test".to_string(),
            vec!["arg1".to_string()],
        );
        
        assert!(ctx.sender.is_player());
        assert_eq!(ctx.sender.player_id(), Some(123));
    }
}
