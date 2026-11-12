//! Consent is a separate command invocation; detection/startup never approves.
use super::consent_models::*;
use std::sync::Arc;
pub struct Service {
    #[cfg(target_os = "macos")]
    inner: std::sync::Mutex<macos::Registry>,
}
impl Default for Service {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: Default::default(),
        }
    }
}
impl Service {
    pub fn start_expiry_worker(service: &Arc<Self>) {
        let weak = Arc::downgrade(service);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let Some(service) = weak.upgrade() else { break };
                service.prune();
            }
        });
    }
    pub fn prune(&self) {
        #[cfg(target_os = "macos")]
        if let Ok(mut registry) = self.inner.lock() {
            registry.prune();
        }
    }
}
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::client_integration::{
        Client, ConfigurationStatus, Context, DetectionStatus, RegistrationStatus, inspect,
        write_engine::{Code, Engine, Plan},
    };
    use crate::installer::{models::InstallationState, status::inspect_installation};
    use std::{
        collections::HashMap,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    const MAX_PLANS: usize = 16;
    const MAX_PLAN_BYTES: usize = 64 * 1024;
    const MAX_REGISTRY_BYTES: usize = 1024 * 1024;
    #[derive(Clone, Copy)]
    enum State {
        Pending,
        Consumed,
        Rejected,
    }
    struct Qualified {
        engine: Engine,
        plan: Plan,
        context: Context,
    }
    struct Entry {
        owner: String,
        expires: Instant,
        state: State,
        data: Option<Qualified>,
        bytes: usize,
    }
    #[derive(Default)]
    pub(super) struct Registry {
        entries: HashMap<String, Entry>,
        working: usize,
    }
    impl Registry {
        pub(super) fn prune(&mut self) {
            let now = Instant::now();
            self.entries.retain(|_, e| e.expires > now);
        }
        fn reserve(&mut self) -> Result<(), ConsentError> {
            self.prune();
            let bytes: usize = self.entries.values().map(|e| e.bytes).sum();
            if self.entries.len() + self.working >= MAX_PLANS
                || bytes + (self.working + 1) * MAX_PLAN_BYTES > MAX_REGISTRY_BYTES
            {
                return Err(err(ConsentCode::RegistryCapacity));
            }
            self.working += 1;
            Ok(())
        }
    }
    struct Working<'a>(&'a Service);
    impl Drop for Working<'_> {
        fn drop(&mut self) {
            if let Ok(mut registry) = self.0.inner.lock() {
                registry.working = registry.working.saturating_sub(1);
            }
        }
    }
    fn err(code: ConsentCode) -> ConsentError {
        ConsentError::new(code)
    }
    fn map(code: Code) -> ConsentError {
        err(match code {
            Code::Expired => ConsentCode::ExpiredPlan,
            Code::Consumed => ConsentCode::ConsumedPlan,
            Code::Conflict => ConsentCode::Conflict,
            Code::Changed | Code::Altered => ConsentCode::StalePlan,
            Code::Capacity => ConsentCode::RecoveryCapacity,
            Code::Uncertain => ConsentCode::Uncertain,
            Code::Disabled => ConsentCode::ActivationDisabled,
            _ => ConsentCode::UnsupportedConfiguration,
        })
    }
    fn roots(context: &Context) -> Result<std::path::PathBuf, ConsentError> {
        let data = context
            .managed
            .root
            .parent()
            .ok_or_else(|| err(ConsentCode::UnsupportedConfiguration))?;
        if context.managed.root != data.join("figcon-installer") {
            return Err(err(ConsentCode::UnsupportedConfiguration));
        }
        Ok(data.into())
    }
    fn prerequisites(context: &Context, client: Client) -> Result<(), ConsentError> {
        if !context.supported {
            return Err(err(ConsentCode::UnsupportedPlatform));
        }
        let status = inspect(context, client);
        if status.detection != DetectionStatus::Detected {
            return Err(err(ConsentCode::ClientUnavailable));
        }
        if !matches!(
            status.configuration,
            ConfigurationStatus::Missing | ConfigurationStatus::Readable
        ) {
            return Err(err(ConsentCode::UnsupportedConfiguration));
        }
        if status.registration != RegistrationStatus::NotRegistered {
            return Err(err(ConsentCode::Conflict));
        }
        if inspect_installation(&context.managed).state != InstallationState::Installed {
            return Err(err(ConsentCode::PrerequisitesMissing));
        }
        Ok(())
    }
    impl Service {
        pub(in crate::client_integration) fn prepare(
            &self,
            context: Context,
            client: Client,
            owner: &str,
        ) -> Result<ConsentPreview, ConsentError> {
            self.prepare_inner(context, client, owner, false)
        }
        fn prepare_inner(
            &self,
            context: Context,
            client: Client,
            owner: &str,
            synthetic: bool,
        ) -> Result<ConsentPreview, ConsentError> {
            if owner.len() > 256 {
                return Err(err(ConsentCode::NotOwner));
            }
            self.inner
                .lock()
                .map_err(|_| err(ConsentCode::OperationFailed))?
                .reserve()?;
            let _working = Working(self);
            prerequisites(&context, client)?;
            let data = roots(&context)?;
            let engine = if synthetic {
                #[cfg(test)]
                {
                    Engine::isolated(&context.home, &data).map_err(map)?
                }
                #[cfg(not(test))]
                {
                    return Err(err(ConsentCode::ActivationDisabled));
                }
            } else {
                Engine::preview_only(&context.home, &data).map_err(map)?
            };
            let recovery = engine.recovery_usage();
            if recovery.state != RecoveryState::Available {
                return Err(err(ConsentCode::RecoveryCapacity));
            }
            let plan = engine.prepare(client).map_err(map)?;
            let size = plan
                .retained_bytes()
                .saturating_add(context.retained_bytes())
                .saturating_add(owner.len());
            if size > MAX_PLAN_BYTES {
                return Err(err(ConsentCode::RegistryCapacity));
            }
            let seconds = plan
                .expires()
                .saturating_duration_since(Instant::now())
                .as_secs();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| err(ConsentCode::OperationFailed))?
                .as_millis() as u64;
            let preview = ConsentPreview {
                plan_id: plan.id().into(),
                client: plan.client(),
                client_name: plan.client().name().into(),
                proposed_server_name: plan.server_name().into(),
                proposed_executable: plan.executable().into(),
                proposed_arguments: plan.args().to_vec(),
                scope: plan.scope().into(),
                operation: plan.operation().into(),
                registration: plan.registration(),
                summary: "Add Figcon's private Figma Console registration for this user without replacing registrations. An exact private backup of any existing configuration will be retained. Production changes are currently disabled.".into(),
                warnings: vec!["production-disabled".into(), "external-writer-race".into(), "backup-required-for-existing-file".into(), "recovery-files-retained".into(), "effective-configuration-unknown".into()],
                expires_at_unix_ms: now + seconds * 1000,
                expires_in_seconds: seconds,
                production_mutation_enabled: false,
                recovery,
            };
            self.inner
                .lock()
                .map_err(|_| err(ConsentCode::OperationFailed))?
                .entries
                .insert(
                    plan.id().into(),
                    Entry {
                        owner: owner.into(),
                        expires: plan.expires(),
                        state: State::Pending,
                        data: Some(Qualified {
                            engine,
                            plan,
                            context,
                        }),
                        bytes: size,
                    },
                );
            Ok(preview)
        }
        pub(in crate::client_integration) fn approve(
            &self,
            context: Context,
            id: &str,
            owner: &str,
        ) -> Result<ApprovalReceipt, ConsentError> {
            self.approve_inner(context, id, owner, false)
        }
        fn approve_inner(
            &self,
            context: Context,
            id: &str,
            owner: &str,
            synthetic: bool,
        ) -> Result<ApprovalReceipt, ConsentError> {
            let q = {
                let mut registry = self
                    .inner
                    .lock()
                    .map_err(|_| err(ConsentCode::OperationFailed))?;
                let e = find(&mut registry, id, owner)?;
                match e.state {
                    State::Pending => {}
                    State::Consumed => return Err(err(ConsentCode::ConsumedPlan)),
                    State::Rejected => return Err(err(ConsentCode::Rejected)),
                }
                e.state = State::Consumed;
                let q = e
                    .data
                    .take()
                    .ok_or_else(|| err(ConsentCode::ConsumedPlan))?;
                e.bytes = 256 + e.owner.len();
                registry.working += 1;
                q
            };
            let _working = Working(self);
            if context.home != q.context.home || context.managed.root != q.context.managed.root {
                return Err(err(ConsentCode::StalePlan));
            }
            prerequisites(&context, q.plan.client()).map_err(|_| err(ConsentCode::StalePlan))?;
            q.engine.revalidate(&q.plan).map_err(map)?;
            if q.engine.recovery_usage().state != RecoveryState::Available {
                return Err(err(ConsentCode::RecoveryCapacity));
            }
            // There is no production branch that invokes Engine::apply. No env,
            // frontend setting, or runtime boolean can activate configuration writes.
            if synthetic {
                #[cfg(test)]
                {
                    q.engine.apply(&q.plan).map_err(map)?;
                    return Ok(ApprovalReceipt {
                        plan_id: id.into(),
                        state: "verified".into(),
                    });
                }
            }
            Err(err(ConsentCode::ActivationDisabled))
        }
        pub(in crate::client_integration) fn reject(
            &self,
            id: &str,
            owner: &str,
        ) -> Result<ApprovalReceipt, ConsentError> {
            let mut registry = self
                .inner
                .lock()
                .map_err(|_| err(ConsentCode::OperationFailed))?;
            let e = find(&mut registry, id, owner)?;
            if !matches!(e.state, State::Pending) {
                return Err(err(ConsentCode::ConsumedPlan));
            }
            e.state = State::Rejected;
            e.data = None;
            e.bytes = 256 + e.owner.len();
            Ok(ApprovalReceipt {
                plan_id: id.into(),
                state: "rejected".into(),
            })
        }
    }
    fn find<'a>(
        registry: &'a mut Registry,
        id: &str,
        owner: &str,
    ) -> Result<&'a mut Entry, ConsentError> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err(ConsentCode::UnknownPlan));
        }
        let e = registry
            .entries
            .get_mut(id)
            .ok_or_else(|| err(ConsentCode::UnknownPlan))?;
        if e.owner != owner {
            return Err(err(ConsentCode::NotOwner));
        }
        if e.expires <= Instant::now() {
            return Err(err(ConsentCode::ExpiredPlan));
        }
        Ok(e)
    }
    #[cfg(test)]
    mod tests;
}
