#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KillSwitchState {
    pub triggered: bool,
    pub manual_restart_required: bool,
}
