use super::db::{BotRecord, Database, McpServerRecord, RoutineRecord, SkillRecord};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatePack {
    pub schema_version: u32,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub bots: Vec<BotRecord>,
    pub skills: Vec<SkillRecord>,
    pub routines: Vec<RoutineRecord>,
    pub mcp_servers: Vec<McpServerRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub bots_imported: usize,
    pub skills_imported: usize,
    pub routines_imported: usize,
    pub mcp_servers_imported: usize,
}

pub struct TemplateEngine;

impl TemplateEngine {
    /// Export workspace configuration, bots, skills, routines, and safe MCP metadata
    /// Strictly excludes any secrets, tokens, or private conversational history
    pub fn export_pack(db: &Database, name: &str, description: &str) -> Result<String, String> {
        let bots = db.list_bots().map_err(|e| e.to_string())?;
        let skills = db.list_skills().map_err(|e| e.to_string())?;
        let routines = db.list_routines().map_err(|e| e.to_string())?;
        let raw_mcp = db.list_mcp_servers().map_err(|e| e.to_string())?;

        // Redact any potentially sensitive environment variables from MCP configs
        let mcp_servers = raw_mcp
            .into_iter()
            .map(|mut srv| {
                if let Ok(mut map) = serde_json::from_str::<
                    serde_json::Map<String, serde_json::Value>,
                >(&srv.env_json)
                {
                    for (k, v) in map.iter_mut() {
                        let upper = k.to_uppercase();
                        if upper.contains("KEY")
                            || upper.contains("TOKEN")
                            || upper.contains("SECRET")
                            || upper.contains("PASS")
                            || upper.contains("AUTH")
                            || upper.contains("CREDENTIAL")
                            || upper.contains("PRIVATE")
                            || v.as_str() == Some("[VAULT_SECRET]")
                        {
                            *v = serde_json::Value::String("<REDACTED_ON_EXPORT>".to_string());
                        }
                    }
                    if let Ok(sanitized) = serde_json::to_string(&map) {
                        srv.env_json = sanitized;
                    }
                }
                srv
            })
            .collect();

        let pack = TemplatePack {
            schema_version: 1,
            name: name.to_string(),
            description: description.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            bots,
            skills,
            routines,
            mcp_servers,
        };

        serde_json::to_string_pretty(&pack).map_err(|e| e.to_string())
    }

    /// Import and validate a template pack into SQLite
    pub fn import_pack(db: &Database, json_str: &str) -> Result<ImportSummary, String> {
        let pack: TemplatePack = serde_json::from_str(json_str)
            .map_err(|e| format!("Invalid template JSON schema: {}", e))?;

        if pack.schema_version != 1 {
            return Err(format!(
                "Unsupported template schema version: {}. Expected version 1.",
                pack.schema_version
            ));
        }

        let mut summary = ImportSummary {
            bots_imported: 0,
            skills_imported: 0,
            routines_imported: 0,
            mcp_servers_imported: 0,
        };

        // 1. Import Bots
        for bot in pack.bots {
            if Self::is_safe_identifier(&bot.id)
                && !bot.name.trim().is_empty()
                && db.create_bot(&bot).is_ok()
            {
                summary.bots_imported += 1;
            }
        }

        // 2. Import Skills
        for skill in pack.skills {
            if Self::is_safe_identifier(&skill.id)
                && !skill.name.trim().is_empty()
                && serde_json::from_str::<serde_json::Value>(&skill.steps_json).is_ok()
                && db.create_skill(&skill).is_ok()
            {
                summary.skills_imported += 1;
            }
        }

        // 3. Import Routines
        for routine in pack.routines {
            if Self::is_safe_identifier(&routine.id)
                && !routine.cron_expression.trim().is_empty()
                && db.create_routine(&routine).is_ok()
            {
                summary.routines_imported += 1;
            }
        }

        // 4. Import MCP Servers
        for srv in pack.mcp_servers {
            if Self::is_safe_identifier(&srv.id)
                && !srv.command.trim().is_empty()
                && db.create_mcp_server(&srv).is_ok()
            {
                summary.mcp_servers_imported += 1;
            }
        }

        Ok(summary)
    }

    /// Built-in pre-packaged templates for instant 1-click enhancement
    pub fn get_starter_templates() -> Vec<TemplatePack> {
        vec![
            TemplatePack {
                schema_version: 1,
                name: "DevOps & Systems Engineering Pack".to_string(),
                description: "Forge specialized profile with automated git and system inspect skills".to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                bots: vec![
                    BotRecord {
                        id: "forge-devops".into(),
                        name: "Forge".into(),
                        avatar: "wrench".into(),
                        role: "DevOps, Code & Terminal Specialist".into(),
                        system_prompt: "You are Forge, the code and systems engineer of Kyro. You specialize in shell commands, git workflows, Docker containerization, code edits, and debugging.".into(),
                        is_default: false,
                    }
                ],
                skills: vec![
                    SkillRecord {
                        id: "skill_git_inspect".into(),
                        name: "Git Status & Log Inspector".into(),
                        description: "Runs non-destructive git status and recent branch logs".into(),
                        steps_json: "[]".into(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                    }
                ],
                routines: vec![
                    RoutineRecord {
                        id: "routine_daily_standup".into(),
                        name: "Daily Workspace Status Summary".into(),
                        cron_expression: "0 9 * * 1-5".into(),
                        action_type: "prompt".into(),
                        target_id: "Inspect repository git status and open tasks".into(),
                        enabled: false,
                        last_run: None,
                        next_run: None,
                    }
                ],
                mcp_servers: vec![],
            },
            TemplatePack {
                schema_version: 1,
                name: "Web Intelligence & QA Pack".to_string(),
                description: "Scout specialized agent with browser navigation routines".to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                bots: vec![
                    BotRecord {
                        id: "scout-web".into(),
                        name: "Scout".into(),
                        avatar: "globe".into(),
                        role: "Browser Automation & Web Researcher".into(),
                        system_prompt: "You are Scout, the web intelligence and browser automation agent of Kyro. You visually navigate websites, fill forms, extract data, and handle web workflows.".into(),
                        is_default: false,
                    }
                ],
                skills: vec![],
                routines: vec![
                    RoutineRecord {
                        id: "routine_hn_digest".into(),
                        name: "Hourly Tech News Digest".into(),
                        cron_expression: "0 * * * *".into(),
                        action_type: "prompt".into(),
                        target_id: "Open browser and check latest tech headlines".into(),
                        enabled: false,
                        last_run: None,
                        next_run: None,
                    }
                ],
                mcp_servers: vec![],
            }
        ]
    }

    fn is_safe_identifier(id: &str) -> bool {
        let trimmed = id.trim();
        !trimmed.is_empty()
            && !trimmed.contains("..")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() <= 128
    }
}
