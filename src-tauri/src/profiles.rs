use crate::pipeline::PipelineMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mode: PipelineMode,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub dictionary_ids: Vec<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub group: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedContext {
    pub app_name: String,
    pub profile_id: String,
    pub profile_name: String,
    pub style: String,
    pub mode: PipelineMode,
    pub source: String,
}

pub fn default_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "personal".into(),
            name: "Personal".into(),
            mode: PipelineMode::Normal,
            style: "personal".into(),
            dictionary_ids: vec![],
            apps: vec![
                "Telegram".into(),
                "Messages".into(),
                "WhatsApp".into(),
                "Signal".into(),
            ],
            group: "chat".into(),
        },
        Profile {
            id: "work".into(),
            name: "Work".into(),
            mode: PipelineMode::Professional,
            style: "work".into(),
            dictionary_ids: vec![],
            apps: vec!["Slack".into(), "Microsoft Teams".into(), "Zoom".into()],
            group: "work".into(),
        },
        Profile {
            id: "email".into(),
            name: "Email".into(),
            mode: PipelineMode::Professional,
            style: "email".into(),
            dictionary_ids: vec![],
            apps: vec![
                "Mail".into(),
                "Spark".into(),
                "Outlook".into(),
                "Mimestream".into(),
            ],
            group: "email".into(),
        },
        Profile {
            id: "developer".into(),
            name: "Developer".into(),
            mode: PipelineMode::Code,
            style: "other".into(),
            dictionary_ids: vec![],
            apps: vec![
                "Code".into(),
                "Cursor".into(),
                "IntelliJ IDEA".into(),
                "Terminal".into(),
                "iTerm2".into(),
                "Warp".into(),
                "Xcode".into(),
            ],
            group: "ide".into(),
        },
        Profile {
            id: "global".into(),
            name: "Other".into(),
            mode: PipelineMode::Normal,
            style: "other".into(),
            dictionary_ids: vec![],
            apps: vec![],
            group: "global".into(),
        },
    ]
}

pub fn resolve_profile(
    profiles: &[Profile],
    app_name: Option<&str>,
    override_id: Option<&str>,
) -> ResolvedContext {
    if let Some(id) = override_id.filter(|s| !s.is_empty()) {
        if let Some(p) = profiles.iter().find(|p| p.id == id) {
            return context(p, app_name.unwrap_or(""), "override");
        }
    }
    let app = app_name.unwrap_or("").trim();
    if !app.is_empty() {
        if let Some(p) = profiles.iter().find(|p| {
            p.apps.iter().any(|a| {
                a.eq_ignore_ascii_case(app)
                    || (!a.is_empty() && app.to_lowercase().contains(&a.to_lowercase()))
            })
        }) {
            return context(p, app, "exact");
        }
        let group = group_for_app(app);
        if group != "global" {
            if let Some(p) = profiles.iter().find(|p| p.group == group) {
                return context(p, app, "group");
            }
        }
    }
    if let Some(p) = profiles.iter().find(|p| p.id == "global") {
        return context(p, app, "global");
    }
    if let Some(p) = profiles.first() {
        return context(p, app, "global");
    }
    ResolvedContext {
        app_name: app.into(),
        profile_id: "global".into(),
        profile_name: "Other".into(),
        style: "other".into(),
        mode: PipelineMode::Normal,
        source: "global".into(),
    }
}

fn context(profile: &Profile, app: &str, source: &str) -> ResolvedContext {
    ResolvedContext {
        app_name: app.into(),
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        style: profile.style.clone(),
        mode: profile.mode,
        source: source.into(),
    }
}

fn group_for_app(app: &str) -> String {
    let lower = app.to_lowercase();
    if ["telegram", "messages", "whatsapp", "signal"]
        .iter()
        .any(|k| lower.contains(k))
    {
        "chat".into()
    } else if ["slack", "teams", "zoom"].iter().any(|k| lower.contains(k)) {
        "work".into()
    } else if ["mail", "outlook", "spark"]
        .iter()
        .any(|k| lower.contains(k))
    {
        "email".into()
    } else if [
        "code", "cursor", "idea", "terminal", "iterm", "xcode", "warp",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        "ide".into()
    } else {
        "global".into()
    }
}

/// Spoken command overlay: Command > Snippet > Dictionary.
pub fn apply_voice_command(text: &str) -> (String, Option<PipelineMode>, Option<String>) {
    let pairs: [(&str, PipelineMode, &str); 10] = [
        ("use professional", PipelineMode::Professional, "work"),
        ("use code", PipelineMode::Code, "other"),
        ("use raw", PipelineMode::Raw, "other"),
        ("use normal", PipelineMode::Normal, "personal"),
        ("use email", PipelineMode::Professional, "email"),
        ("use work", PipelineMode::Professional, "work"),
        ("use personal", PipelineMode::Normal, "personal"),
        (
            "используй профессиональный",
            PipelineMode::Professional,
            "work",
        ),
        ("режим код", PipelineMode::Code, "other"),
        ("режим сырой", PipelineMode::Raw, "other"),
    ];
    let lower = text.to_lowercase();
    for (phrase, mode, style) in pairs {
        if let Some(idx) = lower.find(phrase) {
            let mut out = String::new();
            out.push_str(text[..idx].trim());
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(text[idx + phrase.len()..].trim());
            return (out.trim().to_string(), Some(mode), Some(style.into()));
        }
    }
    (text.to_string(), None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_app_wins() {
        let profiles = default_profiles();
        let ctx = resolve_profile(&profiles, Some("Telegram"), None);
        assert_eq!(ctx.profile_id, "personal");
        assert_eq!(ctx.source, "exact");
    }

    #[test]
    fn override_beats_app() {
        let profiles = default_profiles();
        let ctx = resolve_profile(&profiles, Some("Telegram"), Some("email"));
        assert_eq!(ctx.profile_id, "email");
        assert_eq!(ctx.source, "override");
    }

    #[test]
    fn unknown_app_falls_to_other_profile() {
        let profiles = default_profiles();
        let ctx = resolve_profile(&profiles, Some("Notes"), None);
        assert_eq!(ctx.profile_id, "global");
        assert_eq!(ctx.source, "global");
    }
}
