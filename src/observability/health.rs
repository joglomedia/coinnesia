use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone)]
pub struct HealthRegistry {
    components: Arc<RwLock<BTreeMap<String, ComponentState>>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self {
            components: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn set_component(&self, name: impl Into<String>, healthy: bool) {
        let mut components = self.components.write().expect("health lock poisoned");
        let now = SystemTime::now();
        components.insert(
            name.into(),
            ComponentState {
                healthy,
                heartbeat_required: false,
                last_heartbeat: if healthy { Some(now) } else { None },
            },
        );
    }

    pub fn heartbeat(&self, name: impl Into<String>) {
        let mut components = self.components.write().expect("health lock poisoned");
        let now = SystemTime::now();
        let component = components.entry(name.into()).or_insert(ComponentState {
            healthy: true,
            heartbeat_required: true,
            last_heartbeat: None,
        });
        component.healthy = true;
        component.heartbeat_required = true;
        component.last_heartbeat = Some(now);
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        self.snapshot_with_staleness(Duration::from_secs(u64::MAX))
    }

    pub fn snapshot_with_staleness(&self, stale_after: Duration) -> HealthSnapshot {
        let components = self.components.read().expect("health lock poisoned");
        let now = SystemTime::now();
        let components = components
            .iter()
            .map(|(name, state)| {
                let stale = state
                    .heartbeat_required
                    .then(|| {
                        state
                            .last_heartbeat
                            .and_then(|heartbeat| now.duration_since(heartbeat).ok())
                            .map(|age| age > stale_after)
                            .unwrap_or(true)
                    })
                    .unwrap_or(false);
                ComponentSnapshot {
                    name: name.clone(),
                    healthy: state.healthy && !stale,
                    stale,
                    heartbeat_required: state.heartbeat_required,
                    last_heartbeat_age_secs: state
                        .last_heartbeat
                        .and_then(|heartbeat| now.duration_since(heartbeat).ok())
                        .map(|age| age.as_secs()),
                }
            })
            .collect::<Vec<_>>();
        let healthy = components.iter().all(|component| component.healthy);
        HealthSnapshot {
            healthy,
            components,
        }
    }

    pub fn is_healthy(&self, name: &str) -> bool {
        let components = self.components.read().expect("health lock poisoned");
        components
            .get(name)
            .map(|component| component.healthy)
            .unwrap_or(false)
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
struct ComponentState {
    healthy: bool,
    heartbeat_required: bool,
    last_heartbeat: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    pub healthy: bool,
    pub components: Vec<ComponentSnapshot>,
}

#[derive(Debug, Clone)]
pub struct ComponentSnapshot {
    pub name: String,
    pub healthy: bool,
    pub stale: bool,
    pub heartbeat_required: bool,
    pub last_heartbeat_age_secs: Option<u64>,
}
