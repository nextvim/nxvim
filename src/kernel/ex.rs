use super::{CommandLineRequest, EditorContext};

/// Kernel-owned admission checks for parsed command-line and script-host work.
///
/// This boundary validates stable semantic identity only. Application
/// orchestration, UI projection, services, and script-host command execution
/// remain outside the kernel.
pub struct ExAdmission;

impl ExAdmission {
    pub fn command_line<'a>(
        current: Option<EditorContext>,
        request: &'a CommandLineRequest,
    ) -> Result<&'a CommandLineRequest, String> {
        if current != Some(request.current) {
            return Err("Command-line context changed before execution".to_string());
        }
        Ok(request)
    }

    pub fn host_command(
        current: Option<EditorContext>,
        origin: Option<EditorContext>,
    ) -> Result<(), String> {
        if origin.is_some() && current != origin {
            return Err("Script command context changed before execution".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{BufferId, TabPageId, WindowId};

    fn context(buffer: u64) -> EditorContext {
        EditorContext {
            tab: TabPageId::new(1),
            window: WindowId::new(1),
            buffer: BufferId::new(buffer).unwrap(),
        }
    }

    #[test]
    fn rejects_stale_command_line_context() {
        let request = CommandLineRequest::parse(context(1), ":quit").unwrap();
        assert!(ExAdmission::command_line(Some(context(2)), &request).is_err());
    }

    #[test]
    fn admits_matching_command_line_context() {
        let current = context(1);
        let request = CommandLineRequest::parse(current, ":quit").unwrap();
        let accepted = ExAdmission::command_line(Some(current), &request).unwrap();
        assert_eq!(accepted.current, current);
        assert_eq!(accepted.text, ":quit");
    }

    #[test]
    fn rejects_host_command_from_stale_context() {
        assert!(ExAdmission::host_command(Some(context(2)), Some(context(1))).is_err());
    }

    #[test]
    fn admits_host_command_without_origin_context() {
        assert!(ExAdmission::host_command(None, None).is_ok());
    }
}
