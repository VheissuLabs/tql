use super::Environment;
use crate::settings::Ai;

pub const LOCAL: &str = "tql-local";

pub const MODELS: [(&str, &str); 11] = [
    ("anthropic", "claude-sonnet-5"),
    ("openai", "gpt-5"),
    ("gemini", "gemini-2.5-pro"),
    ("groq", "llama-3.3-70b-versatile"),
    ("mistral", "mistral-large-latest"),
    ("deepseek", "deepseek-chat"),
    ("xai", "grok-4"),
    ("openrouter", "anthropic/claude-sonnet-5"),
    ("ollama", "llama3.2"),
    ("azure", "gpt-4o"),
    ("openai-compatible", ""),
];

const CHAT: [&str; 11] = [
    "anthropic",
    "azure",
    "deepseek",
    "gemini",
    "groq",
    "mistral",
    "ollama",
    "openai",
    "openai-compatible",
    "openrouter",
    "xai",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    Anthropic,
    Responses,
    Gemini,
    Chat,
    JsonObject,
    Ollama,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub name: String,
    pub wire: Wire,
    pub url: String,
    pub key: String,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self.name.as_str() {
            "anthropic" => "Anthropic",
            "openai" | "azure" => "OpenAI",
            "gemini" => "Gemini",
            "groq" => "Groq",
            "mistral" => "Mistral",
            "deepseek" => "DeepSeek",
            "xai" => "xAI",
            "openrouter" => "OpenRouter",
            "ollama" => "Ollama",
            _ => "OpenAI-compatible",
        }
    }
}

pub fn key_variable(provider: &str) -> String {
    match provider {
        "azure" => "AZURE_OPENAI_API_KEY".to_string(),
        "eleven" => "ELEVENLABS_API_KEY".to_string(),
        "openai-compatible" => "OPENAI_COMPATIBLE_API_KEY".to_string(),
        other => format!("{}_API_KEY", other.replace('-', "_").to_uppercase()),
    }
}

pub fn endpoint(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let path = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split_once('/')
        .map_or("", |(_, path)| path);
    let path = path.split(['?', '#']).next().unwrap_or_default();
    if path.trim_matches('/').is_empty() {
        format!("{url}/v1")
    } else {
        url.to_string()
    }
}

fn filled(environment: &Environment, name: &str) -> Option<String> {
    environment(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn local(settings: &Ai) -> Option<Provider> {
    let url = settings.url.trim();
    (!url.is_empty()).then(|| Provider {
        name: LOCAL.to_string(),
        wire: Wire::Chat,
        url: endpoint(url),
        key: settings.key.trim().to_string(),
    })
}

fn named(name: &str, environment: &Environment) -> Option<Provider> {
    let key = filled(environment, &key_variable(name)).unwrap_or_default();
    let address = |variable: &str, default: &str| {
        filled(environment, variable).unwrap_or_else(|| default.to_string())
    };
    let (wire, url) = match name {
        "anthropic" => (
            Wire::Anthropic,
            address("ANTHROPIC_URL", "https://api.anthropic.com/v1"),
        ),
        "openai" => (
            Wire::Responses,
            address("OPENAI_URL", "https://api.openai.com/v1"),
        ),
        "azure" => {
            let base = filled(environment, "AZURE_OPENAI_URL")?;
            (
                Wire::Responses,
                format!("{}/openai/v1", base.trim_end_matches('/')),
            )
        }
        "gemini" => (
            Wire::Gemini,
            address(
                "GEMINI_URL",
                "https://generativelanguage.googleapis.com/v1beta/",
            ),
        ),
        "groq" => (
            Wire::Chat,
            address("GROQ_URL", "https://api.groq.com/openai/v1"),
        ),
        "mistral" => (
            Wire::Chat,
            address("MISTRAL_URL", "https://api.mistral.ai/v1"),
        ),
        "deepseek" => (
            Wire::JsonObject,
            address("DEEPSEEK_URL", "https://api.deepseek.com/v1"),
        ),
        "xai" => (Wire::Responses, address("XAI_URL", "https://api.x.ai/v1")),
        "openrouter" => (
            Wire::Chat,
            address("OPENROUTER_URL", "https://openrouter.ai/api/v1"),
        ),
        "ollama" => {
            let url = filled(environment, "OLLAMA_URL")?;
            return Some(Provider {
                name: name.to_string(),
                wire: Wire::Ollama,
                url,
                key,
            });
        }
        "openai-compatible" => {
            let url = filled(environment, "OPENAI_COMPATIBLE_URL")?;
            return Some(Provider {
                name: name.to_string(),
                wire: Wire::Chat,
                url,
                key,
            });
        }
        _ => return None,
    };
    (!key.is_empty()).then(|| Provider {
        name: name.to_string(),
        wire,
        url,
        key,
    })
}

pub fn chosen(settings: &Ai, environment: &Environment) -> Option<Provider> {
    let configured = settings.provider.trim();
    if !configured.is_empty() && configured != "auto" {
        return named(configured, environment).or_else(|| local(settings));
    }
    local(settings).or_else(|| {
        CHAT.iter()
            .find_map(|provider| named(provider, environment))
    })
}

pub fn model(settings: &Ai, provider: &str) -> String {
    let configured = settings.model.trim();
    if !configured.is_empty() {
        return configured.to_string();
    }
    MODELS
        .iter()
        .find(|(name, _)| *name == provider)
        .map(|(_, model)| model.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_in_the_v1_an_endpoint_is_served_under() {
        assert_eq!(
            endpoint("http://localhost:1234"),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            endpoint("http://localhost:1234/"),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            endpoint("http://localhost:1234/v1"),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            endpoint("https://gateway.test/api/openai/v1"),
            "https://gateway.test/api/openai/v1"
        );
    }

    #[test]
    fn names_the_variable_each_provider_reads_its_key_from() {
        assert_eq!(key_variable("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(key_variable("azure"), "AZURE_OPENAI_API_KEY");
        assert_eq!(
            key_variable("openai-compatible"),
            "OPENAI_COMPATIBLE_API_KEY"
        );
        assert_eq!(key_variable("groq"), "GROQ_API_KEY");
    }
}
