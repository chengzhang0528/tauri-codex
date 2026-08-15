use semver::Version;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const OSS_ROOT: &str =
    "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
pub const BOOTSTRAP_KEY: &str = "bootstrap/windows-x64.json";
pub const SCHEMA_VERSION: u32 = 3;
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_COMPONENT_BYTES: u64 = 1024 * 1024 * 1024;
pub const SELF_USE_PROVENANCE: &str = "unsigned-self-use+sha256";
pub const UPSTREAM_PROVENANCE: &str = "upstream-authenticode+sha256";
pub const METADATA_PROVENANCE: &str = "self-use+sha256";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseMode {
    SelfUse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseEnvelope<T> {
    pub schema_version: u32,
    pub release_mode: ReleaseMode,
    pub payload: T,
}

pub type Bootstrap = ReleaseEnvelope<BootstrapPayload>;
pub type Manifest = ReleaseEnvelope<ManifestPayload>;

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
    #[serde(default)]
    pub installed_tree_sha256: Option<String>,
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
    SetupRequired,
    Downloading,
    Verifying,
    Staged,
    WaitingForDrain,
    Activating,
    HealthCheck,
    Ready,
    RebootRequired,
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
    verify_release_envelope(bootstrap)?;
    let payload = &bootstrap.payload;
    if bootstrap.schema_version != SCHEMA_VERSION
        || payload.product != "tauri-codex"
        || payload.platform != "windows"
        || payload.architecture != "x86_64"
    {
        return Err("self-use Bootstrap 与当前 Windows x64 Launcher 不兼容".to_string());
    }
    validate_version(&payload.minimum_launcher_version)?;
    if let Some(installer) = &payload.installer {
        validate_version(&installer.version)?;
        validate_artifact(&installer.artifact, MAX_COMPONENT_BYTES)?;
        if installer.artifact.provenance != SELF_USE_PROVENANCE {
            return Err("Installer artifact provenance 不满足 self-use 策略".to_string());
        }
        validate_prefix(
            &installer.artifact.object_key,
            &format!("installers/{}/windows-x64/", installer.version),
        )?;
    }
    validate_version(&payload.release.version)?;
    validate_artifact(&payload.release.manifest, MAX_MANIFEST_BYTES)?;
    if payload.release.manifest.provenance != METADATA_PROVENANCE {
        return Err("release manifest provenance 不满足 self-use 策略".to_string());
    }
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
    verify_release_envelope(manifest)?;
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
        let expected_provenance = match component.id {
            ComponentId::Manager => SELF_USE_PROVENANCE,
            ComponentId::Codex | ComponentId::Node => UPSTREAM_PROVENANCE,
        };
        if component.provenance != component.artifact.provenance
            || component.provenance != expected_provenance
        {
            return Err(format!(
                "{} provenance 不满足发布身份要求",
                component.id.as_str()
            ));
        }
        match component.id {
            ComponentId::Manager | ComponentId::Codex => {
                let digest = component
                    .installed_tree_sha256
                    .as_deref()
                    .ok_or_else(|| format!("{} 缺少安装树 SHA-256", component.id.as_str()))?;
                validate_sha256(digest, "安装树")?;
            }
            ComponentId::Node if component.installed_tree_sha256.is_some() => {
                return Err("Node 系统组件不得声明安装树 SHA-256".to_string())
            }
            ComponentId::Node => {}
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

pub fn verify_release_envelope<T>(envelope: &ReleaseEnvelope<T>) -> Result<(), String> {
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "release envelope schema {} 不受支持",
            envelope.schema_version
        ));
    }
    if envelope.release_mode != ReleaseMode::SelfUse {
        return Err("release envelope 不是 self-use 模式".to_string());
    }
    Ok(())
}

pub fn parse_release<T: DeserializeOwned>(
    bytes: &[u8],
    label: &str,
) -> Result<ReleaseEnvelope<T>, String> {
    let envelope: ReleaseEnvelope<T> =
        serde_json::from_slice(bytes).map_err(|error| format!("{label} 无法解析：{error}"))?;
    verify_release_envelope(&envelope)?;
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
    validate_sha256(&artifact.sha256, "artifact")?;
    if artifact.provenance.trim().is_empty() {
        return Err("artifact provenance 不能为空".to_string());
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), String> {
    let digest = value.trim().to_ascii_lowercase();
    if digest.len() != 64
        || !digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!("{label} SHA-256 不合法"));
    }
    if value != digest {
        return Err(format!("{label} SHA-256 必须使用小写十六进制"));
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

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_url, newer, validate_manifest, validate_object_key, verify_release_envelope,
        Artifact, BootstrapPayload, Component, ComponentId, ManifestPayload, ReleaseEnvelope,
        ReleaseMode, ReleaseRef, METADATA_PROVENANCE, SELF_USE_PROVENANCE, UPSTREAM_PROVENANCE,
    };
    use serde_json::json;

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

    fn self_use_bootstrap() -> ReleaseEnvelope<BootstrapPayload> {
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
                    provenance: METADATA_PROVENANCE.to_string(),
                },
            },
        };
        ReleaseEnvelope {
            schema_version: 3,
            release_mode: ReleaseMode::SelfUse,
            payload,
        }
    }

    fn self_use_manifest(payload: ManifestPayload) -> ReleaseEnvelope<ManifestPayload> {
        ReleaseEnvelope {
            schema_version: 3,
            release_mode: ReleaseMode::SelfUse,
            payload,
        }
    }

    fn component(id: ComponentId, tree: Option<String>) -> Component {
        let (version, kind, archive, install_path, name) = match &id {
            ComponentId::Manager => ("0.2.0", "archive", "zip", "manager", "manager.zip"),
            ComponentId::Codex => ("0.147.0", "archive", "zip", "codex", "codex.zip"),
            ComponentId::Node => ("24.19.0", "system", "msi", "system", "node.msi"),
        };
        let provenance = match &id {
            ComponentId::Manager => SELF_USE_PROVENANCE,
            ComponentId::Codex | ComponentId::Node => UPSTREAM_PROVENANCE,
        };
        Component {
            id,
            version: version.to_string(),
            kind: kind.to_string(),
            archive: archive.to_string(),
            required: true,
            artifact: Artifact {
                object_key: format!("releases/0.2.0/windows-x64/components/{name}"),
                size: 1,
                sha256: "a".repeat(64),
                provenance: provenance.to_string(),
            },
            install_path: install_path.to_string(),
            provenance: provenance.to_string(),
            installed_tree_sha256: tree,
        }
    }

    #[test]
    fn self_use_envelope_is_explicit_and_schema_bound() {
        let mut envelope = self_use_bootstrap();
        verify_release_envelope(&envelope).expect("valid self-use envelope");
        assert_eq!(
            serde_json::to_value(&envelope).unwrap()["releaseMode"],
            json!("self-use")
        );
        envelope.schema_version = 2;
        assert!(verify_release_envelope(&envelope).is_err());
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
    fn setup_required_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_value(super::UpdateState::SetupRequired).unwrap(),
            json!("setup_required")
        );
    }

    #[test]
    fn release_envelopes_reject_unknown_fields() {
        let mut value = serde_json::to_value(self_use_bootstrap()).unwrap();
        value["unexpected"] = json!(true);
        assert!(serde_json::from_value::<ReleaseEnvelope<BootstrapPayload>>(value).is_err());
    }

    #[test]
    fn manifest_requires_tree_digests_and_role_scoped_provenance() {
        let payload = ManifestPayload {
            product: "tauri-codex".to_string(),
            version: "0.2.0".to_string(),
            platform: "windows".to_string(),
            architecture: "x86_64".to_string(),
            minimum_launcher_version: "1.1.0".to_string(),
            minimum_manager_version: "0.2.0".to_string(),
            components: vec![
                component(ComponentId::Manager, Some("b".repeat(64))),
                component(ComponentId::Codex, Some("c".repeat(64))),
                component(ComponentId::Node, None),
            ],
        };
        validate_manifest(
            &self_use_manifest(payload.clone()),
            &self_use_bootstrap().payload,
        )
        .unwrap();

        let mut missing = payload.clone();
        missing.components[0].installed_tree_sha256 = None;
        assert!(
            validate_manifest(&self_use_manifest(missing), &self_use_bootstrap().payload).is_err()
        );

        let mut node_tree = payload;
        node_tree.components[2].installed_tree_sha256 = Some("d".repeat(64));
        assert!(
            validate_manifest(&self_use_manifest(node_tree), &self_use_bootstrap().payload)
                .is_err()
        );

        let mut weakened = self_use_manifest(ManifestPayload {
            product: "tauri-codex".to_string(),
            version: "0.2.0".to_string(),
            platform: "windows".to_string(),
            architecture: "x86_64".to_string(),
            minimum_launcher_version: "1.1.0".to_string(),
            minimum_manager_version: "0.2.0".to_string(),
            components: vec![
                component(ComponentId::Manager, Some("b".repeat(64))),
                component(ComponentId::Codex, Some("c".repeat(64))),
                component(ComponentId::Node, None),
            ],
        });
        weakened.payload.components[1].provenance = SELF_USE_PROVENANCE.to_string();
        weakened.payload.components[1].artifact.provenance = SELF_USE_PROVENANCE.to_string();
        assert!(validate_manifest(&weakened, &self_use_bootstrap().payload).is_err());
    }
}
