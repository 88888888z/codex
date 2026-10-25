//! Workbench actions that bridge the fullscreen shell to existing chat views.

use super::*;

impl ChatWidget {
    pub(crate) fn dispatch_workbench_command(&mut self, command: SlashCommand) {
        self.dispatch_command(command);
    }

    pub(crate) fn dismiss_workbench_view(&mut self) -> bool {
        self.bottom_pane.dismiss_active_view()
    }
}
