use super::indexer::AppIndexer;
use crate::storage::{Database, MemoryEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_MEMORY_CHARS: usize = 2500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub scope: String, // "preference", "agent", "project", "summary", "fact"
    pub owner_id: String,
    pub key: String,
    pub value: String,
    pub created_at: i64,
}

pub struct MemoryStore;

impl MemoryStore {
    /// Tokenizes text into lowercase alphanumeric keywords of length >= 3
    pub fn tokenize(text: &str) -> HashSet<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 3)
            .map(|w| w.to_lowercase())
            .collect()
    }

    /// Computes deterministic relevance score based on lexical keyword overlap and recency
    pub fn score_entry(keywords: &HashSet<String>, entry: &MemoryEntry, now: i64) -> f64 {
        let key_lower = entry.key.to_lowercase();
        let val_lower = entry.value.to_lowercase();
        let mut score = 0.0;

        for kw in keywords {
            if key_lower.contains(kw) {
                score += 5.0; // match in key is highly relevant
            }
            if val_lower.contains(kw) {
                score += 2.0; // match in value
            }
        }

        // Recency boost (within 24 hours gets +1.0, within 7 days gets +0.5)
        let age_ms = (now - entry.updated_at).max(0);
        let recency_boost = if age_ms < 86_400_000 {
            1.0
        } else if age_ms < 7 * 86_400_000 {
            0.5
        } else {
            0.0
        };

        score + recency_boost
    }

    /// Ranks a collection of memories by prompt relevance and recency
    pub fn rank_memories(
        keywords: &HashSet<String>,
        mut entries: Vec<MemoryEntry>,
        now: i64,
    ) -> Vec<MemoryEntry> {
        entries.sort_by(|a, b| {
            let score_b = Self::score_entry(keywords, b, now);
            let score_a = Self::score_entry(keywords, a, now);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        entries
    }

    /// Compiles a relevant, bounded context block to inject into system prompt,
    /// filtered strictly by the profile's declared memory scopes.
    pub fn build_context_block_scoped(
        db: &Database,
        active_agent_id: &str,
        task_prompt: &str,
        allowed_scopes: &[String],
    ) -> String {
        let now = chrono::Utc::now().timestamp_millis();
        let keywords = Self::tokenize(task_prompt);
        let prompt_lower = task_prompt.to_lowercase();

        let mut block = String::from("\n\n[Persistent Scoped Context & Memory]\n");

        // Helper closure to append lines within budget
        let append_entry = |line: &str, current_block: &mut String| -> bool {
            if current_block.len() + line.len() > MAX_MEMORY_CHARS {
                return false;
            }
            current_block.push_str(line);
            true
        };

        // 1. User preferences (scope: "preference", owner: "user")
        if allowed_scopes.iter().any(|s| s == "preference") {
            if let Ok(prefs) = db.get_memories_for_scope("preference", "user") {
                let ranked = Self::rank_memories(&keywords, prefs, now);
                if !ranked.is_empty() {
                    let mut pref_section = String::from("User Preferences:\n");
                    let mut added_any = false;
                    for p in ranked {
                        let line = format!("- {}: {}\n", p.key, p.value);
                        if pref_section.len() + line.len() > 1000 {
                            break;
                        }
                        pref_section.push_str(&line);
                        added_any = true;
                    }
                    if added_any {
                        let _ = append_entry(&pref_section, &mut block);
                    }
                }
            } else if let Ok(Some(prefs_str)) = db.get_setting("user_preferences") {
                let line = format!("User Preferences:\n{}\n", prefs_str);
                let _ = append_entry(&line, &mut block);
            }
        }

        // 2. Agent-specific memory (scope: "agent", owner: active_agent_id)
        // Strictly isolated: only memories owned by this specific bot ID are queried
        if allowed_scopes.iter().any(|s| s == "agent") {
            if let Ok(agent_mem) = db.get_memories_for_scope("agent", active_agent_id) {
                let ranked = Self::rank_memories(&keywords, agent_mem, now);
                if !ranked.is_empty() {
                    let mut agent_section = format!("\nAgent Facts (@{}):\n", active_agent_id);
                    let mut added_any = false;
                    for m in ranked {
                        let line = format!("- {}: {}\n", m.key, m.value);
                        if agent_section.len() + line.len() > 1000 {
                            break;
                        }
                        agent_section.push_str(&line);
                        added_any = true;
                    }
                    if added_any {
                        let _ = append_entry(&agent_section, &mut block);
                    }
                }
            }
        }

        // 3. Relevant Installed Desktop Applications (matched against prompt or top favorites)
        let apps = AppIndexer::scan_installed_apps();
        if !apps.is_empty() {
            let mut relevant_apps = Vec::new();
            for app in &apps {
                let name_lower = app.name.to_lowercase();
                if prompt_lower.contains(&name_lower)
                    || name_lower.contains("browser")
                    || name_lower.contains("term")
                {
                    relevant_apps.push(app);
                }
            }

            if relevant_apps.is_empty() {
                relevant_apps = apps.iter().take(8).collect();
            } else {
                relevant_apps.truncate(10);
            }

            let mut app_section = String::from("\nAvailable Desktop Applications:\n");
            for app in relevant_apps {
                let line = format!("- {}: {}\n", app.name, app.exec);
                app_section.push_str(&line);
            }
            let _ = append_entry(&app_section, &mut block);
        }

        // 4. Common User Directories
        let mut dir_section = String::from("\nStandard User Directories:\n");
        if let Some(home) = dirs::home_dir() {
            dir_section.push_str(&format!("- Home: {}\n", home.display()));
            dir_section.push_str(&format!("- Desktop: {}\n", home.join("Desktop").display()));
            dir_section.push_str(&format!(
                "- Downloads: {}\n",
                home.join("Downloads").display()
            ));
            dir_section.push_str(&format!(
                "- Documents: {}\n",
                home.join("Documents").display()
            ));
        }
        let _ = append_entry(&dir_section, &mut block);

        // Strict context budget enforcement
        if block.len() > MAX_MEMORY_CHARS {
            block.truncate(MAX_MEMORY_CHARS.saturating_sub(40));
            block.push_str("\n...[truncated for context budget]\n");
        }

        block
    }

    /// Compiles a relevant, bounded context block to inject into system prompt
    pub fn build_context_block(db: &Database, active_agent_id: &str, task_prompt: &str) -> String {
        Self::build_context_block_scoped(
            db,
            active_agent_id,
            task_prompt,
            &["preference".to_string(), "agent".to_string()],
        )
    }

    /// Explicitly save a memory fact
    pub fn save_fact(
        db: &Database,
        scope: &str,
        owner_id: &str,
        key: &str,
        value: &str,
        source: &str,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let id = format!("mem_{}", uuid::Uuid::new_v4());
        let entry = MemoryEntry {
            id,
            scope: scope.to_string(),
            owner_id: owner_id.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            source: source.to_string(),
            created_at: now,
            updated_at: now,
        };
        db.save_memory(&entry).map_err(|e| e.to_string())
    }
}
