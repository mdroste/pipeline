//! Automatic conversation titles.
//!
//! After a conversation's first completed exchange, Workspace asks the
//! isolated App Server for a short title in a separate ephemeral read-only
//! thread with no tools. The call is optional, user-configurable, and never
//! touches the conversation's own native thread, harness, or turn permit.
//! Everything here is pure; `commands.rs` owns the async orchestration.

use super::codex::WorkspaceModel;
use super::store::{ConversationSnapshot, Store, WorkbenchResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_TITLE: &str = "New conversation";
const PREFERENCE_KEY: &str = "conversation_titles";
const MAX_TITLE_CHARS: usize = 80;
const MAX_USER_EXCERPT_CHARS: usize = 2_000;
const MAX_ASSISTANT_EXCERPT_CHARS: usize = 1_200;
/// Reasoning efforts from cheapest to most expensive, for the automatic choice.
const EFFORT_ORDER: [&str; 6] = ["none", "minimal", "low", "medium", "high", "xhigh"];

pub const TITLE_INSTRUCTIONS: &str = "You write titles for saved chat conversations. Reply with only the title: three to six words, sentence case, no surrounding quotes, no trailing punctuation, no preamble. Do not run commands, read files, or search.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TitlePreferences {
    /// Generate a title after the first completed exchange.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Exact model id, or `None` for the cheapest available model.
    #[serde(default)]
    pub model: Option<String>,
    /// Reasoning effort, or `None` for the lowest the model advertises.
    #[serde(default)]
    pub effort: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl Default for TitlePreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            model: None,
            effort: None,
        }
    }
}

pub fn load_preferences(store: &Store) -> WorkbenchResult<TitlePreferences> {
    Ok(store
        .preference(PREFERENCE_KEY)?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

pub fn save_preferences(
    store: &Store,
    mut preferences: TitlePreferences,
) -> WorkbenchResult<TitlePreferences> {
    preferences.model = normalize_identifier(preferences.model.take(), "model")?;
    preferences.effort = normalize_identifier(preferences.effort.take(), "reasoning effort")?;
    let value = serde_json::to_value(&preferences)
        .map_err(|error| super::store::WorkbenchError::invalid(error.to_string()))?;
    store.set_preference(PREFERENCE_KEY, &value)?;
    Ok(preferences)
}

fn normalize_identifier(value: Option<String>, label: &str) -> WorkbenchResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(super::store::WorkbenchError::invalid(format!(
            "Invalid title {label}"
        )));
    }
    Ok(Some(value))
}

pub fn is_default_title(title: &str) -> bool {
    title.trim() == DEFAULT_TITLE
}

/// The first user message and the first final assistant reply, bounded.
pub fn first_exchange(snapshot: &ConversationSnapshot) -> Option<(String, String)> {
    let mut user = None;
    let mut assistant = None;
    for item in &snapshot.items {
        let kind = item.item_kind.to_ascii_lowercase();
        if !kind.contains("message") {
            continue;
        }
        let Some(text) = message_text(&item.payload) else {
            continue;
        };
        if kind.contains("user") {
            if user.is_none() {
                user = Some(text);
            }
        } else if user.is_some() && assistant.is_none() && item.is_final {
            assistant = Some(text);
        }
        if user.is_some() && assistant.is_some() {
            break;
        }
    }
    Some((
        truncate_chars(&user?, MAX_USER_EXCERPT_CHARS),
        truncate_chars(&assistant?, MAX_ASSISTANT_EXCERPT_CHARS),
    ))
}

pub fn message_text(payload: &Value) -> Option<String> {
    for key in ["text", "message", "content"] {
        match payload.get(key) {
            Some(Value::String(text)) if !text.trim().is_empty() => return Some(text.clone()),
            Some(Value::Array(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        part.as_str()
                            .or_else(|| part.get("text").and_then(Value::as_str))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn title_prompt(user: &str, assistant: &str) -> String {
    format!(
        "Write a title for the conversation below.\n\n<user_message>\n{user}\n</user_message>\n\n<assistant_reply>\n{assistant}\n</assistant_reply>\n\nReply with only the title."
    )
}

/// Turns a model reply into a usable title, or `None` when nothing survives.
pub fn sanitize_title(raw: &str) -> Option<String> {
    let first_line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut title = first_line
        .trim_start_matches(['#', '*', '-', '>'])
        .trim()
        .to_string();
    for prefix in ["Title:", "title:", "TITLE:"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }
    title = title
        .trim_matches(|character: char| {
            matches!(
                character,
                '"' | '\u{201c}' | '\u{201d}' | '\'' | '`' | '*' | '_'
            )
        })
        .trim_end_matches(['.', '!', ':', ';', ','])
        .trim()
        .to_string();
    let collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed.chars().any(char::is_control) {
        return None;
    }
    let bounded = truncate_chars(&collapsed, MAX_TITLE_CHARS);
    if is_default_title(&bounded) {
        return None;
    }
    Some(bounded)
}

/// The model and effort for the title call. `None` lets the App Server pick
/// its default model. A preferred model that the account cannot use falls
/// back to the automatic choice instead of failing a background call.
pub fn choose_model(
    catalog: &[WorkspaceModel],
    preferences: &TitlePreferences,
) -> Option<(String, Option<String>)> {
    let preferred = preferences
        .model
        .as_deref()
        .and_then(|wanted| catalog.iter().find(|model| model.model == wanted));
    let model = preferred.or_else(|| cheapest_model(catalog))?;
    let effort = preferences
        .effort
        .as_deref()
        .filter(|wanted| {
            model
                .supported_reasoning_efforts
                .iter()
                .any(|option| option.reasoning_effort == *wanted)
        })
        .map(str::to_string)
        .or_else(|| lowest_effort(model));
    Some((model.model.clone(), effort))
}

fn cheapest_model(catalog: &[WorkspaceModel]) -> Option<&WorkspaceModel> {
    for marker in ["nano", "mini"] {
        if let Some(model) = catalog
            .iter()
            .find(|model| model.model.to_ascii_lowercase().contains(marker))
        {
            return Some(model);
        }
    }
    catalog
        .iter()
        .find(|model| model.is_default)
        .or_else(|| catalog.first())
}

fn lowest_effort(model: &WorkspaceModel) -> Option<String> {
    EFFORT_ORDER
        .iter()
        .find(|candidate| {
            model
                .supported_reasoning_efforts
                .iter()
                .any(|option| option.reasoning_effort == **candidate)
        })
        .map(|effort| (*effort).to_string())
        .or_else(|| {
            model
                .supported_reasoning_efforts
                .first()
                .map(|option| option.reasoning_effort.clone())
        })
}

fn truncate_chars(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (count, character) in text.chars().enumerate() {
        if count >= limit {
            out.push('…');
            break;
        }
        out.push(character);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::super::codex::ReasoningEffortOption;
    use super::super::store::{TranscriptItem, WorkbenchSession};
    use super::*;
    use serde_json::json;

    fn model(id: &str, is_default: bool, efforts: &[&str]) -> WorkspaceModel {
        WorkspaceModel {
            id: id.to_string(),
            model: id.to_string(),
            display_name: id.to_string(),
            description: String::new(),
            is_default,
            default_reasoning_effort: efforts.first().map(|s| s.to_string()).unwrap_or_default(),
            supported_reasoning_efforts: efforts
                .iter()
                .map(|effort| ReasoningEffortOption {
                    reasoning_effort: effort.to_string(),
                    description: String::new(),
                })
                .collect(),
        }
    }

    fn item(kind: &str, text: &str, is_final: bool) -> TranscriptItem {
        TranscriptItem {
            id: format!("{kind}-{}", text.len()),
            turn_id: None,
            provider_item_id: format!("p-{kind}-{}", text.len()),
            item_kind: kind.to_string(),
            payload: json!({ "text": text }),
            is_final,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn snapshot(items: Vec<TranscriptItem>) -> ConversationSnapshot {
        ConversationSnapshot {
            workspace: None,
            session: WorkbenchSession {
                id: "session".to_string(),
                workspace_id: None,
                paper_id: None,
                title: DEFAULT_TITLE.to_string(),
                preset_id: None,
                overrides: json!({}),
                draft: String::new(),
                revision: 1,
                archived_at: None,
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
            },
            active_binding: None,
            turns: Vec::new(),
            items,
            sequence: 1,
        }
    }

    #[test]
    fn sanitizes_model_replies_into_bounded_titles() {
        assert_eq!(
            sanitize_title("\"Identification in panel data.\"\n\nExtra line").as_deref(),
            Some("Identification in panel data")
        );
        assert_eq!(
            sanitize_title("Title: **Welfare effects of tariffs**").as_deref(),
            Some("Welfare effects of tariffs")
        );
        assert_eq!(sanitize_title("   \n  ").as_deref(), None);
        assert_eq!(sanitize_title("New conversation").as_deref(), None);
        let long = sanitize_title(&"word ".repeat(60)).unwrap();
        assert!(long.chars().count() <= MAX_TITLE_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn automatic_model_choice_prefers_small_models_and_lowest_effort() {
        let catalog = vec![
            model("gpt-large", true, &["medium", "high"]),
            model("gpt-large-mini", false, &["high", "low", "medium"]),
        ];
        let automatic = TitlePreferences::default();
        assert_eq!(
            choose_model(&catalog, &automatic),
            Some(("gpt-large-mini".to_string(), Some("low".to_string())))
        );
        let pinned = TitlePreferences {
            enabled: true,
            model: Some("gpt-large".to_string()),
            effort: Some("high".to_string()),
        };
        assert_eq!(
            choose_model(&catalog, &pinned),
            Some(("gpt-large".to_string(), Some("high".to_string())))
        );
        let unavailable = TitlePreferences {
            enabled: true,
            model: Some("gone".to_string()),
            effort: Some("xhigh".to_string()),
        };
        assert_eq!(
            choose_model(&catalog, &unavailable),
            Some(("gpt-large-mini".to_string(), Some("low".to_string())))
        );
        assert_eq!(choose_model(&[], &automatic), None);
        let default_only = vec![model("gpt-a", false, &[]), model("gpt-b", true, &[])];
        assert_eq!(
            choose_model(&default_only, &automatic),
            Some(("gpt-b".to_string(), None))
        );
    }

    #[test]
    fn first_exchange_requires_a_final_reply_after_the_first_prompt() {
        assert_eq!(first_exchange(&snapshot(Vec::new())), None);
        let only_user = snapshot(vec![item("userMessage", "Why does the IV fail?", true)]);
        assert_eq!(first_exchange(&only_user), None);
        let streaming = snapshot(vec![
            item("userMessage", "Why does the IV fail?", true),
            item("agentMessage", "Because…", false),
        ]);
        assert_eq!(first_exchange(&streaming), None);
        let complete = snapshot(vec![
            item("agentMessage", "Stray greeting", true),
            item("userMessage", "Why does the IV fail?", true),
            item("commandExecution", "ls", true),
            item(
                "agentMessage",
                "The exclusion restriction is violated.",
                true,
            ),
            item("userMessage", "Later question", true),
        ]);
        assert_eq!(
            first_exchange(&complete),
            Some((
                "Why does the IV fail?".to_string(),
                "The exclusion restriction is violated.".to_string()
            ))
        );
        let prompt = title_prompt(
            "Why does the IV fail?",
            "The exclusion restriction is violated.",
        );
        assert!(prompt.contains("<user_message>\nWhy does the IV fail?"));
        assert!(prompt.ends_with("Reply with only the title."));
    }

    #[test]
    fn preferences_round_trip_through_the_store_with_defaults() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open_at(&temporary.path().join("store")).unwrap();
        assert_eq!(
            load_preferences(&store).unwrap(),
            TitlePreferences::default()
        );
        let saved = save_preferences(
            &store,
            TitlePreferences {
                enabled: false,
                model: Some("  gpt-mini ".to_string()),
                effort: Some(String::new()),
            },
        )
        .unwrap();
        assert_eq!(saved.model.as_deref(), Some("gpt-mini"));
        assert_eq!(saved.effort, None);
        assert_eq!(load_preferences(&store).unwrap(), saved);
        assert!(save_preferences(
            &store,
            TitlePreferences {
                enabled: true,
                model: Some("bad\nmodel".to_string()),
                effort: None,
            },
        )
        .is_err());
        assert!(is_default_title(" New conversation "));
    }
}
