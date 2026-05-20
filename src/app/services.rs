use crate::app::AppState;

#[derive(Clone)]
pub struct Services {
    pub state: AppState,
}

impl Services {
    pub const fn new(state: AppState) -> Self {
        Self { state }
    }
}
