//! Add-only engine. Production construction permits planning/revalidation only.
mod capacity;
mod editor;
mod fs;
use super::{Client, SERVER_NAME};
use fs::{Dir, Snapshot, Stamp};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{
    collections::HashMap,
    fmt,
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Code {
    Disabled,
    Capacity,
    Missing,
    Exists,
    Unsafe,
    Permission,
    Unsupported,
    Malformed,
    Conflict,
    Changed,
    Expired,
    Altered,
    Consumed,
    Busy,
    BackupFailed,
    Uncertain,
    ReceiptStale,
    Verification,
    Io,
}
pub(super) type Result<T> = std::result::Result<T, Code>;
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Client transaction {:?}; inspect transaction status before retrying.",
            self
        )
    }
}
fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|_| Code::Io)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| Code::Io)
}
fn stamp(value: &Option<Snapshot>) -> Option<Stamp> {
    value.as_ref().map(|v| v.stamp.clone())
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
enum Operation {
    Add,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
enum ExpectedRegistration {
    Absent,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: u32,
    id: String,
    client: Client,
    operation: Operation,
    scope: String,
    target: PathBuf,
    server_name: String,
    executable: String,
    args: Vec<String>,
    expected: Option<Stamp>,
    parent_anchor: fs::Anchor,
    data_anchor: fs::Anchor,
    registration: ExpectedRegistration,
    payload: Vec<Stamp>,
    candidate_sha256: String,
}
/// Fields are private; no deserialization or frontend-supplied plan exists.
#[derive(Clone)]
pub(super) struct Plan {
    binding: Binding,
    expires: Instant,
}
impl Plan {
    pub(super) fn id(&self) -> &str {
        &self.binding.id
    }
    pub(super) fn expires(&self) -> Instant {
        self.expires
    }
    pub(super) fn server_name(&self) -> &str {
        &self.binding.server_name
    }
    pub(super) fn scope(&self) -> &str {
        &self.binding.scope
    }
    pub(super) fn operation(&self) -> &'static str {
        match self.binding.operation {
            Operation::Add => "add",
        }
    }
    pub(super) fn registration(&self) -> super::RegistrationStatus {
        match self.binding.registration {
            ExpectedRegistration::Absent => super::RegistrationStatus::NotRegistered,
        }
    }
    pub(super) fn client(&self) -> Client {
        self.binding.client
    }
    pub(super) fn executable(&self) -> &str {
        &self.binding.executable
    }
    pub(super) fn args(&self) -> &[String] {
        &self.binding.args
    }
    pub(super) fn retained_bytes(&self) -> usize {
        json(&self.binding).map_or(usize::MAX, |b| {
            b.len().saturating_mul(2).saturating_add(1024)
        })
    }
}
impl fmt::Debug for Plan {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("ApprovalPlan([private])")
    }
}
struct Reserved {
    digest: String,
    expires: Instant,
    consumed: bool,
}
pub(super) struct Engine {
    home: PathBuf,
    data: PathBuf,
    plans: Mutex<HashMap<String, Reserved>>,
    qualification_writes: bool,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Point {
    BeforeBackup,
    AfterBackup,
    BeforeFinalCheck,
    AfterFinalCheck,
    AfterPublish,
    BeforeReceipt,
    AfterReceipt,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    binding: Binding,
    phase: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    binding: Binding,
    post: Stamp,
    entry_sha256: String,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    NotApplied,
    PossiblyApplied,
    Verified,
    NeedsReview,
}
impl Engine {
    /// Only qualification tests can instantiate the engine. Production activation
    /// needs a separate policy/consent decision, trusted roots and runtime checks.
    #[cfg(test)]
    pub(super) fn isolated(home: &Path, data: &Path) -> Result<Self> {
        if !home.is_absolute() || !data.starts_with(home) || home == data {
            return Err(Code::Unsafe);
        }
        Dir::open(home)?.private()?;
        Dir::open(data)?.private()?;
        Ok(Self {
            home: home.into(),
            data: data.into(),
            plans: Mutex::new(HashMap::new()),
            qualification_writes: true,
        })
    }
    /// Production constructs only a read-only planner. No public mutation switch.
    pub(super) fn preview_only(home: &Path, data: &Path) -> Result<Self> {
        Dir::open(home)?.writable()?;
        Dir::open(data)?.writable()?;
        Ok(Self {
            home: home.into(),
            data: data.into(),
            plans: Mutex::new(HashMap::new()),
            qualification_writes: false,
        })
    }
    pub(super) fn recovery_usage(
        &self,
    ) -> crate::client_integration::consent_models::RecoveryUsage {
        capacity::inspect(&self.home, &self.data)
    }
    pub(super) fn revalidate(&self, plan: &Plan) -> Result<()> {
        let catalog = self.plans.lock().map_err(|_| Code::Io)?;
        let saved = catalog.get(&plan.binding.id).ok_or(Code::Altered)?;
        if saved.digest != hash(&json(&plan.binding)?) || saved.expires != plan.expires {
            return Err(Code::Altered);
        }
        if saved.consumed {
            return Err(Code::Consumed);
        }
        if Instant::now() >= plan.expires {
            return Err(Code::Expired);
        }
        drop(catalog);
        let b = &plan.binding;
        let parent = self.parent(b.client)?;
        if b.target != self.target(b.client)
            || parent.anchor() != b.parent_anchor
            || Dir::open(&self.data)?.anchor() != b.data_anchor
            || stamp(&parent.read(Self::filename(b.client))?) != b.expected
            || self.launch()?.2 != b.payload
        {
            return Err(Code::Changed);
        }
        Ok(())
    }
    fn target(&self, client: Client) -> PathBuf {
        self.home.join(match client {
            Client::ClaudeCode => ".claude.json",
            Client::CodexCli => ".codex/config.toml",
        })
    }
    fn launch(&self) -> Result<(String, Vec<String>, Vec<Stamp>)> {
        let root = self.data.join("figcon-installer");
        let paths = [
            root.join("runtime/bin/node"),
            root.join("mcp/node_modules/figma-console-mcp/dist/local.js"),
        ];
        let mut stamps = Vec::new();
        for path in &paths {
            let parent = Dir::open(path.parent().ok_or(Code::Unsafe)?)?;
            stamps.push(
                parent.fingerprint(
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .ok_or(Code::Unsafe)?,
                )?,
            );
        }
        Ok((
            paths[0].to_str().ok_or(Code::Unsupported)?.into(),
            vec![paths[1].to_str().ok_or(Code::Unsupported)?.into()],
            stamps,
        ))
    }
    fn parent(&self, client: Client) -> Result<Dir> {
        Dir::open(self.target(client).parent().ok_or(Code::Unsafe)?)
    }
    fn filename(client: Client) -> &'static str {
        match client {
            Client::ClaudeCode => ".claude.json",
            Client::CodexCli => "config.toml",
        }
    }
    fn storage(&self) -> Result<Dir> {
        let data = Dir::open(&self.data)?;
        data.writable()?;
        data.child("client-config-transactions")
    }
    pub fn prepare(&self, client: Client) -> Result<Plan> {
        self.prepare_for(client, Duration::from_secs(300))
    }
    fn prepare_for(&self, client: Client, ttl: Duration) -> Result<Plan> {
        let parent = self.parent(client)?;
        parent.writable()?;
        let before = parent.read(Self::filename(client))?;
        if let Some(before) = &before {
            fs::require_writable(before)?;
        }
        let (executable, args, payload) = self.launch()?;
        let entry = editor::entry(client, &executable, &args);
        let candidate = editor::add(client, before.as_ref().map(|s| s.text.as_str()), &entry)?;
        let binding = Binding {
            schema: 1,
            id: nonce()?,
            client,
            operation: Operation::Add,
            scope: "user".into(),
            target: self.target(client),
            server_name: SERVER_NAME.into(),
            executable,
            args,
            expected: stamp(&before),
            parent_anchor: parent.anchor(),
            data_anchor: Dir::open(&self.data)?.anchor(),
            registration: ExpectedRegistration::Absent,
            payload,
            candidate_sha256: hash(candidate.as_bytes()),
        };
        let expires = Instant::now() + ttl;
        let digest = hash(&json(&binding)?);
        self.plans.lock().map_err(|_| Code::Io)?.insert(
            binding.id.clone(),
            Reserved {
                digest,
                expires,
                consumed: false,
            },
        );
        Ok(Plan { binding, expires })
    }
    fn claim(&self, plan: &Plan) -> Result<()> {
        let mut plans = self.plans.lock().map_err(|_| Code::Io)?;
        let saved = plans.get_mut(&plan.binding.id).ok_or(Code::Altered)?;
        if saved.digest != hash(&json(&plan.binding)?)
            || saved.expires != plan.expires
            || plan.binding.target != self.target(plan.binding.client)
        {
            return Err(Code::Altered);
        }
        if Instant::now() >= plan.expires {
            return Err(Code::Expired);
        }
        if saved.consumed {
            return Err(Code::Consumed);
        }
        saved.consumed = true;
        Ok(())
    }
    pub fn apply(&self, plan: &Plan) -> Result<String> {
        self.apply_with(plan, &mut |_| Ok(()))
    }
    fn apply_with(&self, plan: &Plan, hook: &mut dyn FnMut(Point) -> Result<()>) -> Result<String> {
        if !self.qualification_writes {
            return Err(Code::Disabled);
        }
        self.claim(plan)?;
        let b = &plan.binding;
        let storage = self.storage()?;
        let _lock = storage.lock(match b.client {
            Client::ClaudeCode => "claude.lock",
            Client::CodexCli => "codex.lock",
        })?;
        let _capacity_lock = storage.lock("capacity.lock")?;
        capacity::admit(&self.home, &self.data)?;
        let parent = self.parent(b.client)?;
        if parent.anchor() != b.parent_anchor || Dir::open(&self.data)?.anchor() != b.data_anchor {
            return Err(Code::Changed);
        }
        parent.writable()?;
        let before = parent.read(Self::filename(b.client))?;
        if stamp(&before) != b.expected {
            return Err(Code::Changed);
        }
        if let Some(snapshot) = &before {
            fs::require_writable(snapshot)?;
        }
        if self.launch()?.2 != b.payload {
            return Err(Code::Changed);
        }
        let entry = editor::entry(b.client, &b.executable, &b.args);
        let candidate = editor::add(b.client, before.as_ref().map(|s| s.text.as_str()), &entry)?;
        if hash(candidate.as_bytes()) != b.candidate_sha256 {
            return Err(Code::Altered);
        }
        let artifacts = storage.child(&b.id)?;
        artifacts.create(
            "prepared.json",
            &json(&Journal {
                binding: b.clone(),
                phase: "prepared".into(),
            })?,
            0o600,
        )?;
        hook(Point::BeforeBackup)?;
        if let Some(before) = &before {
            artifacts
                .create("backup.bin", before.text.as_bytes(), 0o600)
                .map_err(|_| Code::BackupFailed)?;
            if artifacts
                .read("backup.bin")
                .map_err(|_| Code::BackupFailed)?
                .map(|s| s.stamp.sha256)
                != Some(before.stamp.sha256.clone())
            {
                return Err(Code::BackupFailed);
            }
        }
        hook(Point::AfterBackup)?;
        let temp = format!(".figcon-{}.candidate", b.id);
        parent.create(&temp, candidate.as_bytes(), 0o600)?;
        let staged = parent.read(&temp)?.ok_or(Code::Unsafe)?;
        if staged.stamp.sha256 != b.candidate_sha256 {
            return Err(Code::Changed);
        }
        artifacts.create(
            "applying.json",
            &json(&Journal {
                binding: b.clone(),
                phase: "applying".into(),
            })?,
            0o600,
        )?;
        hook(Point::BeforeFinalCheck)?;
        if Instant::now() >= plan.expires {
            return Err(Code::Expired);
        }
        storage.check()?;
        artifacts.check()?;
        parent.check()?;
        parent.writable()?;
        if self.launch()?.2 != b.payload
            || stamp(&parent.read(Self::filename(b.client))?) != b.expected
            || parent.read(&temp)?.map(|s| s.stamp) != Some(staged.stamp.clone())
        {
            return Err(Code::Changed);
        }
        // Every retained sibling candidate stays 0600. Original permissions are
        // restored through an owned handle only after successful publication.
        if let Some(before) = &before {
            fs::same_owners(before, &staged)?;
        }
        let publication = parent.publication_handle(&temp, &staged.stamp)?;
        hook(Point::AfterFinalCheck)?;
        if Instant::now() >= plan.expires {
            return Err(Code::Expired);
        }
        // This final check is not a CAS. Existing-file replacement is enabled
        // only by the isolated test constructor, never a production endpoint.
        parent.publish(&temp, Self::filename(b.client), before.is_none())?;
        let post = (|| {
            Dir::finish_mode(&publication, before.as_ref().map_or(0o600, |s| s.mode))?;
            parent.sync()?;
            hook(Point::AfterPublish)?;
            parent.check()?;
            let after = parent
                .read(Self::filename(b.client))?
                .ok_or(Code::Verification)?;
            if !fs::published_matches(&staged, &after) {
                return Err(Code::Verification);
            }
            if let Some(before) = &before {
                fs::same_permissions(before, &after)?;
            } else if after.mode != 0o600 {
                return Err(Code::Verification);
            }
            editor::verify(
                b.client,
                before.as_ref().map(|s| s.text.as_str()),
                &after.text,
                &entry,
            )?;
            Ok(after)
        })()
        .map_err(|_| Code::Uncertain)?;
        let receipt = Receipt {
            schema: 1,
            binding: b.clone(),
            post: post.stamp,
            entry_sha256: hash(&json(&entry)?),
        };
        (|| {
            hook(Point::BeforeReceipt)?;
            let bytes = json(&receipt)?;
            artifacts.create("receipt.json", &bytes, 0o600)?;
            hook(Point::AfterReceipt)?;
            artifacts.create("verified.json", &json(&hash(&bytes))?, 0o600)
        })()
        .map_err(|_| Code::Uncertain)?;
        Ok(b.id.clone())
    }
    fn transaction(&self, id: &str) -> Result<Dir> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Code::Unsafe);
        }
        // Opening evidence must never initialize or repair directories.
        let dir = Dir::open(&self.data.join("client-config-transactions").join(id))?;
        dir.private()?;
        Ok(dir)
    }
    pub fn verify_receipt(&self, id: &str) -> Result<()> {
        let artifacts = self.transaction(id)?;
        let raw = artifacts.read("receipt.json")?.ok_or(Code::ReceiptStale)?;
        let marker = artifacts.read("verified.json")?.ok_or(Code::ReceiptStale)?;
        if serde_json::from_str::<String>(&marker.text).map_err(|_| Code::ReceiptStale)?
            != hash(raw.text.as_bytes())
        {
            return Err(Code::ReceiptStale);
        }
        let receipt: Receipt = serde_json::from_str(&raw.text).map_err(|_| Code::ReceiptStale)?;
        let b = &receipt.binding;
        let prepared = artifacts.read("prepared.json")?.ok_or(Code::ReceiptStale)?;
        let prepared: Journal =
            serde_json::from_str(&prepared.text).map_err(|_| Code::ReceiptStale)?;
        if prepared.phase != "prepared" || json(&prepared.binding)? != json(b)? {
            return Err(Code::ReceiptStale);
        }
        let applying = artifacts.read("applying.json")?.ok_or(Code::ReceiptStale)?;
        let journal: Journal =
            serde_json::from_str(&applying.text).map_err(|_| Code::ReceiptStale)?;
        if receipt.schema != 1
            || b.id != id
            || b.schema != 1
            || b.scope != "user"
            || b.server_name != SERVER_NAME
            || b.target != self.target(b.client)
            || journal.phase != "applying"
            || json(&journal.binding)? != json(b)?
        {
            return Err(Code::ReceiptStale);
        }
        let after = self
            .parent(b.client)?
            .read(Self::filename(b.client))?
            .ok_or(Code::ReceiptStale)?;
        if self.parent(b.client)?.anchor() != b.parent_anchor
            || Dir::open(&self.data)?.anchor() != b.data_anchor
        {
            return Err(Code::ReceiptStale);
        }
        if after.stamp != receipt.post
            || after.stamp.sha256 != b.candidate_sha256
            || self.launch()?.2 != b.payload
        {
            return Err(Code::ReceiptStale);
        }
        let entry = editor::entry(b.client, &b.executable, &b.args);
        if hash(&json(&entry)?) != receipt.entry_sha256
            || editor::parse(b.client, &after.text)?
                .get(editor::key(b.client))
                .and_then(|s| s.get(SERVER_NAME))
                != Some(&entry)
        {
            return Err(Code::ReceiptStale);
        }
        Ok(())
    }
    pub fn recover(&self, id: &str) -> Recovery {
        if self.verify_receipt(id).is_ok() {
            return Recovery::Verified;
        }
        let inspect = || -> Result<Recovery> {
            let artifacts = self.transaction(id)?;
            let source = artifacts
                .read("applying.json")?
                .or(artifacts.read("prepared.json")?)
                .ok_or(Code::Unsafe)?;
            let journal: Journal = serde_json::from_str(&source.text).map_err(|_| Code::Unsafe)?;
            let b = journal.binding;
            if b.id != id
                || b.target != self.target(b.client)
                || b.scope != "user"
                || b.server_name != SERVER_NAME
            {
                return Err(Code::Unsafe);
            }
            let current = self.parent(b.client)?.read(Self::filename(b.client))?;
            if stamp(&current) == b.expected {
                return Ok(Recovery::NotApplied);
            }
            if current.is_some_and(|s| s.stamp.sha256 == b.candidate_sha256) {
                return Ok(Recovery::PossiblyApplied);
            }
            Ok(Recovery::NeedsReview)
        };
        inspect().unwrap_or(Recovery::NeedsReview)
    }
}
#[cfg(test)]
mod tests;
