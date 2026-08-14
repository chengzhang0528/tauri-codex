use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const OSS_ROOT: &str =
    "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
pub const BOOTSTRAP_KEY: &str = "bootstrap/windows-x64.json";
pub const SCHEMA_VERSION: u32 = 2;
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_COMPONENT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SignedEnvelope<T> {
    pub schema_version: u32,
    pub key_id: String,
    pub payload: T,
    pub signature: String,
}

pub type Bootstrap = SignedEnvelope<BootstrapPayload>;
pub type Manifest = SignedEnvelope<ManifestPayload>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub product: String,
    pub platform: String,
    pub architecture: String,
    pub minimum_launcher_version: String,
    pub installer: Option<InstallerRef>,
    pub release: ReleaseRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct InstallerRef {
    pub version: String,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRef {
    pub version: String,
    pub manifest: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPayload {
    pub product: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub minimum_launcher_version: String,
    pub minimum_manager_version: String,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: ComponentId,
    pub version: String,
    pub kind: String,
    pub archive: String,
    pub required: bool,
    pub artifact: Artifact,
    pub install_path: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentId {
    Manager,
    Codex,
    Node,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manager => "manager",
            Self::Codex => "codex",
            Self::Node => "node",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub object_key: String,
    pub size: u64,
    pub sha256: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Verifying,
    Staged,
    WaitingForDrain,
    Activating,
    HealthCheck,
    Ready,
    Failed,
    RepairRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateTarget {
    Release { version: String },
    Installer { version: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckTrigger {
    Manual,
    Automatic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateIntent {
    Check { trigger: CheckTrigger },
    Prepare,
    Activate { active_sessions: usize },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeliverySnapshot {
    pub state: UpdateState,
    pub target: Option<UpdateTarget>,
    pub current_version: Option<String>,
    pub current_codex_version: Option<String>,
    pub current_node_version: Option<String>,
    pub active_sessions: usize,
    pub phase: String,
    pub component: String,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub checked_at: Option<u64>,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub state: UpdateState,
    pub target: Option<UpdateTarget>,
    pub message: String,
}

pub fn validate_bootstrap(bootstrap: &Bootstrap) -> Result<(), String> {
    verify_envelope(bootstrap)?;
    let payload = &bootstrap.payload;
    if bootstrap.schema_version != SCHEMA_VERSION
        || payload.product != "tauri-codex"
        || payload.platform != "windows"
        || payload.architecture != "x86_64"
    {
        return Err("signed Bootstrap 与当前 Windows x64 Launcher 不兼容".to_string());
    }
    validate_version(&payload.minimum_launcher_version)?;
    if let Some(installer) = &payload.installer {
        validate_version(&installer.version)?;
        validate_artifact(&installer.artifact, MAX_COMPONENT_BYTES)?;
        if installer.artifact.provenance != "authenticode+ed25519" {
            return Err("Installer artifact provenance 不满足身份要求".to_string());
        }
        validate_prefix(
            &installer.artifact.object_key,
            &format!("installers/{}/windows-x64/", installer.version),
        )?;
    }
    validate_version(&payload.release.version)?;
    validate_artifact(&payload.release.manifest, MAX_MANIFEST_BYTES)?;
    if payload.release.manifest.object_key
        != format!(
            "releases/{}/windows-x64/manifest.json",
            payload.release.version
        )
    {
        return Err("Bootstrap manifest object key 不匹配".to_string());
    }
    Ok(())
}

pub fn validate_manifest(manifest: &Manifest, bootstrap: &BootstrapPayload) -> Result<(), String> {
    verify_envelope(manifest)?;
    let payload = &manifest.payload;
    if manifest.schema_version != SCHEMA_VERSION
        || payload.product != "tauri-codex"
        || payload.version != bootstrap.release.version
        || payload.platform != "windows"
        || payload.architecture != "x86_64"
    {
        return Err("release manifest 与 Bootstrap 不一致".to_string());
    }
    validate_version(&payload.minimum_launcher_version)?;
    validate_version(&payload.minimum_manager_version)?;
    if Version::parse(&payload.minimum_launcher_version).map_err(|error| error.to_string())?
        > Version::parse(&bootstrap.minimum_launcher_version).map_err(|error| error.to_string())?
    {
        return Err("Bootstrap 声明的 Launcher 兼容版本低于 manifest 要求".to_string());
    }
    for id in [ComponentId::Manager, ComponentId::Codex, ComponentId::Node] {
        if payload
            .components
            .iter()
            .filter(|component| component.required && component.id == id)
            .count()
            != 1
        {
            return Err(format!(
                "manifest 必须且只能包含一个必需 {} 组件",
                id.as_str()
            ));
        }
    }
    for component in &payload.components {
        validate_version(&component.version)?;
        validate_artifact(&component.artifact, MAX_COMPONENT_BYTES)?;
        validate_prefix(
            &component.artifact.object_key,
            &format!("releases/{}/windows-x64/components/", payload.version),
        )?;
        if component.install_path.is_empty()
            || component.install_path.contains("..")
            || component.install_path.starts_with('/')
            || component.install_path.contains('\\')
            || component.install_path.contains(':')
        {
            return Err(format!("{} 安装路径不安全", component.id.as_str()));
        }
        if component.provenance != component.artifact.provenance
            || component.provenance != "authenticode+ed25519"
        {
            return Err(format!(
                "{} provenance 不满足发布身份要求",
                component.id.as_str()
            ));
        }
        match component.id {
            ComponentId::Manager | ComponentId::Codex
                if component.kind != "archive" || component.archive != "zip" =>
            {
                return Err(format!("{} 归档规则不合法", component.id.as_str()))
            }
            ComponentId::Node if component.kind != "system" || component.archive != "msi" => {
                return Err("Node 组件规则不合法".to_string())
            }
            _ => {}
        }
        let expected_path = match component.id {
            ComponentId::Manager => "manager",
            ComponentId::Codex => "codex",
            ComponentId::Node => "system",
        };
        if component.install_path != expected_path {
            return Err(format!(
                "{} 安装路径必须是 {expected_path}",
                component.id.as_str()
            ));
        }
    }
    let manager = payload
        .components
        .iter()
        .find(|component| component.id == ComponentId::Manager && component.required)
        .expect("required Manager was counted above");
    if manager.version != payload.version
        || Version::parse(&manager.version).map_err(|error| error.to_string())?
            < Version::parse(&payload.minimum_manager_version).map_err(|error| error.to_string())?
    {
        return Err("Manager 版本与 release compatibility 不一致".to_string());
    }
    Ok(())
}

pub fn verify_envelope<T>(envelope: &SignedEnvelope<T>) -> Result<(), String>
where
    T: Serialize,
{
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "signed envelope schema {} 不受支持",
            envelope.schema_version
        ));
    }
    let key_id = configured_key_id()?;
    if envelope.key_id != key_id {
        return Err(format!("签名 keyId {} 不受信任", envelope.key_id));
    }
    let public_key = configured_public_key()?;
    let signature = STANDARD
        .decode(envelope.signature.as_bytes())
        .map_err(|error| format!("Ed25519 signature 不是 base64：{error}"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|error| format!("Ed25519 signature 长度错误：{error}"))?;
    let key = VerifyingKey::from_bytes(&public_key)
        .map_err(|error| format!("Ed25519 public key 无效：{error}"))?;
    let payload = serde_json::to_value(&envelope.payload).map_err(|error| error.to_string())?;
    let bytes = canonical_json(&payload);
    key.verify(&bytes, &signature)
        .map_err(|_| "Ed25519 signature 校验失败".to_string())
}

pub fn parse_signed<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    label: &str,
) -> Result<SignedEnvelope<T>, String> {
    let envelope: SignedEnvelope<T> =
        serde_json::from_slice(bytes).map_err(|error| format!("{label} 无法解析：{error}"))?;
    verify_envelope(&envelope)?;
    Ok(envelope)
}

pub fn artifact_url(object_key: &str) -> Result<String, String> {
    validate_object_key(object_key)?;
    Ok(format!("{OSS_ROOT}/{object_key}"))
}

pub fn validate_artifact(artifact: &Artifact, max_size: u64) -> Result<(), String> {
    validate_object_key(&artifact.object_key)?;
    if artifact.size == 0 || artifact.size > max_size {
        return Err(format!("artifact size 不在允许范围内：{}", artifact.size));
    }
    let digest = artifact.sha256.trim().to_ascii_lowercase();
    if digest.len() != 64
        || !digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("artifact SHA-256 不合法".to_string());
    }
    if artifact.sha256 != digest {
        return Err("artifact SHA-256 必须使用小写十六进制".to_string());
    }
    if artifact.provenance.trim().is_empty() {
        return Err("artifact provenance 不能为空".to_string());
    }
    Ok(())
}

pub fn validate_object_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.starts_with('/')
        || key.contains('\\')
        || key.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.chars().any(|character| {
                    !(character.is_ascii_alphanumeric()
                        || matches!(character, '.' | '-' | '_' | '@'))
                })
        })
    {
        return Err("OSS object key 不安全".to_string());
    }
    Ok(())
}

fn validate_prefix(key: &str, prefix: &str) -> Result<(), String> {
    if !key.starts_with(prefix) {
        return Err(format!("object key 必须位于 {prefix}"));
    }
    Ok(())
}

pub fn validate_version(value: &str) -> Result<(), String> {
    let version =
        Version::parse(value).map_err(|error| format!("版本号不合法：{value} ({error})"))?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != value {
        return Err(format!("只接受规范三段稳定版本号：{value}"));
    }
    Ok(())
}

pub fn newer(candidate: &str, current: &str) -> Result<bool, String> {
    validate_version(candidate)?;
    validate_version(current)?;
    Ok(
        Version::parse(candidate).map_err(|error| error.to_string())?
            > Version::parse(current).map_err(|error| error.to_string())?,
    )
}

fn configured_key_id() -> Result<String, String> {
    if let Some(value) = option_env!("TAURI_CODEX_RELEASE_KEY_ID") {
        return Ok(value.to_string());
    }
    if cfg!(any(test, debug_assertions)) {
        return Ok("development-rfc8032".to_string());
    }
    Err("未配置 TAURI_CODEX_RELEASE_KEY_ID，拒绝验证发布签名".to_string())
}

fn configured_public_key() -> Result<[u8; 32], String> {
    let encoded = option_env!("TAURI_CODEX_RELEASE_PUBLIC_KEY").unwrap_or(
        if cfg!(any(test, debug_assertions)) {
            "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo="
        } else {
            ""
        },
    );
    let compact = encoded.to_string();
    if compact.is_empty() {
        return Err("未配置 TAURI_CODEX_RELEASE_PUBLIC_KEY，拒绝验证发布签名".to_string());
    }
    let bytes = STANDARD
        .decode(compact.as_bytes())
        .map_err(|error| format!("Ed25519 public key 不是 base64：{error}"))?;
    bytes
        .try_into()
        .map_err(|_| "Ed25519 public key 必须是 32 bytes".to_string())
}

pub fn canonical_json(value: &Value) -> Vec<u8> {
    fn write(value: &Value, output: &mut String) {
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => output.push_str(&value.to_string()),
            Value::String(value) => {
                output.push_str(&serde_json::to_string(value).expect("string serialization"))
            }
            Value::Array(values) => {
                output.push('[');
                for (index, item) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    write(item, output);
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(key).expect("key serialization"));
                    output.push(':');
                    write(&values[*key], output);
                }
                output.push('}');
            }
        }
    }
    let mut output = String::new();
    write(value, &mut output);
    output.into_bytes()
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_url, canonical_json, newer, validate_object_key, verify_envelope, Artifact,
        BootstrapPayload, ReleaseRef, SignedEnvelope,
    };
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_object_keys() {
        assert_eq!(
            canonical_json(&json!({"b": 2, "a": 1})),
            br#"{"a":1,"b":2}"#
        );
    }

    #[test]
    fn object_url_is_fixed_to_oss() {
        assert_eq!(artifact_url("releases/0.2.0/windows-x64/manifest.json").unwrap(), "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex/releases/0.2.0/windows-x64/manifest.json");
        assert!(artifact_url("../secret").is_err());
        assert!(validate_object_key("releases//bad").is_err());
    }

    #[test]
    fn versions_only_move_forward() {
        assert!(newer("0.2.1", "0.2.0").unwrap());
        assert!(!newer("0.2.0", "0.2.0").unwrap());
        assert!(!newer("0.1.9", "0.2.0").unwrap());
        assert!(newer("v0.2.1", "0.2.0").is_err());
        assert!(newer("0.2.1-beta.1", "0.2.0").is_err());
    }

    fn signed_bootstrap() -> SignedEnvelope<BootstrapPayload> {
        let payload = BootstrapPayload {
            product: "tauri-codex".to_string(),
            platform: "windows".to_string(),
            architecture: "x86_64".to_string(),
            minimum_launcher_version: "1.1.0".to_string(),
            installer: None,
            release: ReleaseRef {
                version: "0.2.0".to_string(),
                manifest: Artifact {
                    object_key: "releases/0.2.0/windows-x64/manifest.json".to_string(),
                    size: 128,
                    sha256: "a".repeat(64),
                    provenance: "ed25519".to_string(),
                },
            },
        };
        let key = SigningKey::from_bytes(&[
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ]);
        let bytes = canonical_json(&serde_json::to_value(&payload).unwrap());
        SignedEnvelope {
            schema_version: 2,
            key_id: "development-rfc8032".to_string(),
            payload,
            signature: STANDARD.encode(key.sign(&bytes).to_bytes()),
        }
    }

    #[test]
    fn ed25519_envelope_accepts_exact_payload_and_rejects_mutation() {
        let mut envelope = signed_bootstrap();
        verify_envelope(&envelope).expect("valid test envelope");
        envelope.payload.release.version = "0.2.1".to_string();
        assert!(verify_envelope(&envelope).is_err());
    }

    #[test]
    fn update_targets_have_typed_wire_shapes() {
        let release = serde_json::to_value(super::UpdateTarget::Release {
            version: "0.2.1".to_string(),
        })
        .unwrap();
        let installer = serde_json::to_value(super::UpdateTarget::Installer {
            version: "1.1.1".to_string(),
        })
        .unwrap();
        assert_eq!(release, json!({"release": {"version": "0.2.1"}}));
        assert_eq!(installer, json!({"installer": {"version": "1.1.1"}}));
    }

    #[test]
    fn signed_envelopes_reject_unknown_fields() {
        let mut value = serde_json::to_value(signed_bootstrap()).unwrap();
        value["unexpected"] = json!(true);
        assert!(serde_json::from_value::<SignedEnvelope<BootstrapPayload>>(value).is_err());
    }
}
