use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

#[derive(Debug, Clone)]
pub struct HealthRegistry {
    components: Arc<RwLock<BTreeMap<String, bool>>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self {
            components: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn set_component(&self, name: impl Into<String>, healthy: bool) {
        let mut components = self.components.write().expect("health lock poisoned");
        components.insert(name.into(), healthy);
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        let components = self.components.read().expect("health lock poisoned");
        let components = components
            .iter()
            .map(|(name, healthy)| ComponentSnapshot {
                name: name.clone(),
                healthy: *healthy,
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
        components.get(name).copied().unwrap_or(false)
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
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
}
