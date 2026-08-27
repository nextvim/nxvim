//! Typed application command envelope and request categories.

pub use super::typed_command::*;

#[cfg(test)]
mod tests {
    use crate::app::outcome::AppCommandOutcome;

    #[test]
    fn kernel_effects_survive_the_app_boundary() {
        let kernel = crate::kernel::CommandOutcome {
            effects: vec![
                crate::kernel::CommandEffect::Message("done".to_string()),
                crate::kernel::CommandEffect::QuitRequested,
            ],
            redraw: crate::kernel::RedrawRequest::View,
            invalidations: Vec::new(),
        };
        let outcome = AppCommandOutcome::from_kernel(kernel);
        assert_eq!(outcome.redraw, crate::kernel::RedrawRequest::View);
        assert!(outcome.quit);
        assert_eq!(outcome.kernel_effects.len(), 2);
    }
}
