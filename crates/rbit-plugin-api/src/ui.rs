//! Declarative host-rendered UI data. No markup, code, classes or external URLs.

use crate::PluginId;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};

pub const MAX_UI_CONTRIBUTIONS: usize = 32;
pub const MAX_UI_BYTES: usize = 64 * 1024;
pub const MAX_UI_LIST_ITEMS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UiValidationError {
    #[error("invalid UI contribution identifier")]
    Identifier,
    #[error("invalid internal UI target")]
    Target,
    #[error("missing or invalid UI route parameter")]
    Parameter,
    #[error("invalid or unauthorized UI message namespace/key/fallback")]
    Message,
    #[error("UI contribution resource limit exceeded")]
    Limit,
    #[error("unsupported UI schema version")]
    Schema,
    #[error("duplicate UI contribution identifier")]
    Duplicate,
    #[error("UI slot, component, visibility or route context do not match")]
    Slot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UiContributionId(String);
impl UiContributionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for UiContributionId {
    type Err = UiValidationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !contribution_id(value) {
            return Err(UiValidationError::Identifier);
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for UiContributionId {
    type Error = UiValidationError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<UiContributionId> for String {
    fn from(value: UiContributionId) -> Self {
        value.0
    }
}
impl fmt::Display for UiContributionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UiTarget(String);
impl FromStr for UiTarget {
    type Err = UiValidationError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > 256
            || !value.starts_with('/')
            || value.starts_with("//")
            || value.matches("{owner}").count() > 1
            || value.matches("{repo}").count() > 1
        {
            return Err(UiValidationError::Target);
        }
        if value != "/"
            && value[1..].split('/').any(|segment| {
                segment.is_empty()
                    || matches!(segment, "." | "..")
                    || (!matches!(segment, "{owner}" | "{repo}")
                        && !segment.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')))
            })
        {
            return Err(UiValidationError::Target);
        }
        Ok(Self(value.to_owned()))
    }
}
impl TryFrom<String> for UiTarget {
    type Error = UiValidationError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl From<UiTarget> for String {
    fn from(value: UiTarget) -> Self {
        value.0
    }
}
impl UiTarget {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn render(&self, owner: Option<&str>, repo: Option<&str>) -> Result<String, UiValidationError> {
        let mut result = String::new();
        if self.0 == "/" {
            return Ok("/".into());
        }
        for segment in self.0[1..].split('/') {
            result.push('/');
            match segment {
                "{owner}" => encode_segment(owner.ok_or(UiValidationError::Parameter)?, &mut result)?,
                "{repo}" => encode_segment(repo.ok_or(UiValidationError::Parameter)?, &mut result)?,
                _ => result.push_str(segment),
            }
        }
        Ok(result)
    }
}
fn encode_segment(value: &str, output: &mut String) -> Result<(), UiValidationError> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    if value.is_empty()
        || value.len() > 256
        || matches!(value, "." | "..")
        || value.chars().any(char::is_control)
        || value.bytes().any(|byte| matches!(byte, b'/' | b'\\' | b'%' | b'?' | b'#'))
    {
        return Err(UiValidationError::Parameter);
    }
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiSlot {
    GlobalNav,
    RepositoryTab,
    RepositorySettings,
    AccountSettings,
    OrganizationSettings,
    AdminNav,
    AdminPanel,
    RepositoryPanel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiVisibility {
    Public,
    SignedIn,
    InstanceAdmin,
    OrganizationAdmin,
    RepositoryReader,
    RepositoryAdmin,
    RepositoryActionsEnabled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiIcon {
    None,
    Activity,
    Box,
    Code,
    Globe,
    Info,
    Play,
    Settings,
    Shield,
    Terminal,
}
impl UiIcon {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Activity => "activity",
            Self::Box => "box",
            Self::Code => "code",
            Self::Globe => "globe",
            Self::Info => "info",
            Self::Play => "play",
            Self::Settings => "settings",
            Self::Shield => "shield",
            Self::Terminal => "terminal",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiNoticeTone {
    Info,
    Success,
    Warning,
    Danger,
}
impl UiNoticeTone {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}
pub type UiTone = UiNoticeTone;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiMessage {
    pub namespace: String,
    pub key: String,
    pub fallback: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiLink {
    pub label: UiMessage,
    pub target: UiTarget,
    pub icon: UiIcon,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiContent {
    Link(UiLink),
    Card { title: UiMessage, body: UiMessage, action: Option<UiLink> },
    Notice { message: UiMessage, tone: UiNoticeTone },
    List { title: UiMessage, items: Vec<UiLink> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiContribution {
    pub id: UiContributionId,
    pub slot: UiSlot,
    pub order: i16,
    pub visibility: UiVisibility,
    pub content: UiContent,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiContributionSet {
    pub schema_version: u32,
    pub contributions: Vec<UiContribution>,
}
impl UiContributionSet {
    pub const fn new(contributions: Vec<UiContribution>) -> Self {
        Self { schema_version: 1, contributions }
    }
    pub fn validate(&self, owner: &PluginId) -> Result<(), UiValidationError> {
        if self.schema_version != 1 {
            return Err(UiValidationError::Schema);
        }
        if self.contributions.is_empty() || self.contributions.len() > MAX_UI_CONTRIBUTIONS {
            return Err(UiValidationError::Limit);
        }
        let mut seen = BTreeSet::new();
        let mut bytes = 0;
        for contribution in &self.contributions {
            if !seen.insert(&contribution.id) {
                return Err(UiValidationError::Duplicate);
            }
            contribution.validate(owner, &mut bytes)?;
        }
        if bytes > MAX_UI_BYTES {
            return Err(UiValidationError::Limit);
        }
        Ok(())
    }
}
impl UiContribution {
    fn validate(&self, owner: &PluginId, bytes: &mut usize) -> Result<(), UiValidationError> {
        use UiSlot as S;
        use UiVisibility as V;
        let valid_visibility = match self.slot {
            S::GlobalNav => matches!(self.visibility, V::Public | V::SignedIn | V::InstanceAdmin),
            S::AccountSettings => matches!(self.visibility, V::SignedIn | V::InstanceAdmin),
            S::OrganizationSettings => matches!(self.visibility, V::OrganizationAdmin | V::InstanceAdmin),
            S::AdminNav | S::AdminPanel => self.visibility == V::InstanceAdmin,
            S::RepositorySettings => matches!(self.visibility, V::RepositoryAdmin | V::InstanceAdmin),
            S::RepositoryTab | S::RepositoryPanel => {
                matches!(self.visibility, V::RepositoryReader | V::RepositoryAdmin | V::RepositoryActionsEnabled)
            }
        };
        if !valid_visibility {
            return Err(UiValidationError::Slot);
        }
        if matches!(self.slot, S::GlobalNav | S::RepositoryTab | S::AccountSettings | S::AdminNav)
            && !matches!(self.content, UiContent::Link(_))
        {
            return Err(UiValidationError::Slot);
        }
        let check_message = |message: &UiMessage, bytes: &mut usize| {
            let own = message.namespace == owner.as_str();
            let first_party = (owner.as_str() == "rbit.actions" && matches!(message.namespace.as_str(), "core" | "actions"))
                || (owner.as_str() == "rbit.i18n" && message.namespace == "core");
            if (!own && !first_party)
                || !local_id(&message.key, 160)
                || message.fallback.is_empty()
                || message.fallback.len() > 1024
                || message.fallback.chars().any(|character| character.is_control() && character != '\n')
            {
                return Err(UiValidationError::Message);
            }
            *bytes += message.namespace.len() + message.key.len() + message.fallback.len();
            Ok(())
        };
        let check_link = |link: &UiLink, bytes: &mut usize| {
            check_message(&link.label, bytes)?;
            let target = link.target.as_str();
            let repo_slot = matches!(self.slot, S::RepositoryTab | S::RepositorySettings | S::RepositoryPanel);
            if (target.contains("{repo}") && !repo_slot)
                || (target.contains("{owner}") && !repo_slot && self.slot != S::OrganizationSettings)
                || (repo_slot && !target.starts_with("/{owner}/{repo}/"))
            {
                return Err(UiValidationError::Slot);
            }
            *bytes += target.len();
            Ok(())
        };
        *bytes += self.id.as_str().len();
        match &self.content {
            UiContent::Link(link) => check_link(link, bytes)?,
            UiContent::Card { title, body, action } => {
                check_message(title, bytes)?;
                check_message(body, bytes)?;
                if let Some(action) = action {
                    check_link(action, bytes)?;
                }
            }
            UiContent::Notice { message, .. } => check_message(message, bytes)?,
            UiContent::List { title, items } => {
                check_message(title, bytes)?;
                if items.is_empty() || items.len() > MAX_UI_LIST_ITEMS {
                    return Err(UiValidationError::Limit);
                }
                for item in items {
                    check_link(item, bytes)?;
                }
            }
        }
        Ok(())
    }
}
fn local_id(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn contribution_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.'))
}
