//! Application-facing command results.
//!
//! Semantic effects and redraw invalidations are kernel-owned. This wrapper
//! contains only the temporary application/UI projection and quit signal
//! needed while legacy command handlers are being retired.

use crate::app::ui::ViewEffect;

#[derive(Debug, Default)]
pub struct AppCommandOutcome {
    pub redraw: crate::kernel::RedrawRequest,
    pub quit: bool,
    pub view_effects: Vec<ViewEffect>,
    pub kernel_effects: Vec<crate::kernel::CommandEffect>,
    pub invalidations: Vec<crate::kernel::RedrawInvalidation>,
}

impl AppCommandOutcome {
    pub fn from_kernel(outcome: crate::kernel::CommandOutcome) -> Self {
        let quit = outcome
            .effects
            .iter()
            .any(|effect| matches!(effect, crate::kernel::CommandEffect::QuitRequested));
        Self {
            redraw: outcome.redraw,
            quit,
            invalidations: outcome.invalidations,
            kernel_effects: outcome.effects,
            ..Self::default()
        }
    }

    pub fn redraw() -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::View,
            ..Self::default()
        }
    }

    pub fn layout() -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::Layout,
            invalidations: vec![crate::kernel::RedrawInvalidation::global(
                crate::kernel::RedrawInvalidationKind::CompleteLayout,
            )],
            ..Self::default()
        }
    }

    pub fn window_redraw(
        window: crate::kernel::WindowId,
        kind: crate::kernel::RedrawInvalidationKind,
    ) -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::View,
            invalidations: vec![crate::kernel::RedrawInvalidation::window(kind, window)],
            ..Self::default()
        }
    }

    pub fn global_redraw(kind: crate::kernel::RedrawInvalidationKind) -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::View,
            invalidations: vec![crate::kernel::RedrawInvalidation::global(kind)],
            ..Self::default()
        }
    }

    pub fn statusline() -> Self {
        Self::global_redraw(crate::kernel::RedrawInvalidationKind::Statusline)
    }

    pub fn quit() -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::View,
            quit: true,
            ..Self::default()
        }
    }

    pub fn with_effect(effect: ViewEffect) -> Self {
        Self {
            redraw: crate::kernel::RedrawRequest::Layout,
            view_effects: vec![effect],
            ..Self::default()
        }
    }

    pub fn merge(&mut self, mut other: Self) {
        self.redraw = self.redraw.max(other.redraw);
        self.quit |= other.quit;
        self.invalidations.append(&mut other.invalidations);
        self.view_effects.append(&mut other.view_effects);
        self.kernel_effects.append(&mut other.kernel_effects);
    }
}
