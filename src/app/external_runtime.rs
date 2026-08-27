use std::collections::VecDeque;

use crate::kernel::{BufferId, ChannelId, JobId, TabPageId, TerminalId, TimerId, WindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeOwner {
    pub script_task: Option<u64>,
    pub buffer: Option<BufferId>,
    pub window: Option<WindowId>,
    pub tab: Option<TabPageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    Active,
    Firing,
    Stopped,
}

impl TimerState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Active, Self::Firing | Self::Stopped)
                | (Self::Firing, Self::Active | Self::Stopped)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Starting,
    Running,
    Exited,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Starting,
                Self::Running | Self::Failed | Self::Cancelled
            ) | (Self::Running, Self::Exited | Self::Failed | Self::Cancelled)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    Opening,
    Open,
    Closing,
    Closed,
    Failed,
}

impl ChannelState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Opening, Self::Open | Self::Failed)
                | (Self::Open, Self::Closing | Self::Closed | Self::Failed)
                | (Self::Closing, Self::Closed | Self::Failed)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalState {
    Starting,
    Running,
    Exited,
    Closed,
}

impl TerminalState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Starting, Self::Running | Self::Exited | Self::Closed)
                | (Self::Running, Self::Exited | Self::Closed)
                | (Self::Exited, Self::Closed)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStream {
    Stdout,
    Stderr,
}

/// Owned event produced by an external runtime and admitted on the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalRuntimeEvent {
    TimerReady {
        timer: TimerId,
        generation: u64,
        owner: RuntimeOwner,
    },
    JobStarted {
        job: JobId,
        sequence: u64,
        owner: RuntimeOwner,
    },
    JobOutput {
        job: JobId,
        stream: JobStream,
        sequence: u64,
        bytes: Vec<u8>,
        owner: RuntimeOwner,
    },
    JobExited {
        job: JobId,
        sequence: u64,
        status: Option<i32>,
        owner: RuntimeOwner,
    },
    ChannelMessage {
        channel: ChannelId,
        sequence: u64,
        bytes: Vec<u8>,
        owner: RuntimeOwner,
    },
    ChannelClosed {
        channel: ChannelId,
        sequence: u64,
        error: Option<String>,
        owner: RuntimeOwner,
    },
    Failed {
        resource: ExternalResourceId,
        sequence: u64,
        message: String,
        owner: RuntimeOwner,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalResourceId {
    Timer(TimerId),
    Job(JobId),
    Channel(ChannelId),
    Terminal(TerminalId),
}

#[derive(Debug, Default)]
struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    fn allocate(&mut self) -> u64 {
        if self.next == 0 {
            self.next = 1;
        }
        let allocated = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("external runtime ID space exhausted");
        allocated
    }
}

/// Main-thread integration seam for timers, jobs, channels, and terminals.
/// Transport-specific managers will enqueue owned events here; editor and VM
/// state are never exposed to transport threads.
#[derive(Debug, Default)]
pub struct ExternalRuntimeService {
    ids: IdAllocator,
    ready: VecDeque<ExternalRuntimeEvent>,
    accepting_requests: bool,
}

impl ExternalRuntimeService {
    pub fn new() -> Self {
        Self {
            accepting_requests: true,
            ..Self::default()
        }
    }

    pub fn allocate_timer_id(&mut self) -> TimerId {
        TimerId::new(self.ids.allocate()).expect("allocator only returns non-zero IDs")
    }

    pub fn allocate_job_id(&mut self) -> JobId {
        JobId::new(self.ids.allocate()).expect("allocator only returns non-zero IDs")
    }

    pub fn allocate_channel_id(&mut self) -> ChannelId {
        ChannelId::new(self.ids.allocate()).expect("allocator only returns non-zero IDs")
    }

    pub fn allocate_terminal_id(&mut self) -> TerminalId {
        TerminalId::new(self.ids.allocate()).expect("allocator only returns non-zero IDs")
    }

    pub fn is_accepting_requests(&self) -> bool {
        self.accepting_requests
    }

    pub fn has_ready_events(&self) -> bool {
        !self.ready.is_empty()
    }

    pub fn enqueue(&mut self, event: ExternalRuntimeEvent) -> Result<(), ExternalRuntimeEvent> {
        if !self.accepting_requests {
            return Err(event);
        }
        self.ready.push_back(event);
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<ExternalRuntimeEvent> {
        self.ready.drain(..).collect()
    }

    pub fn begin_shutdown(&mut self) {
        self.accepting_requests = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> RuntimeOwner {
        RuntimeOwner {
            script_task: Some(9),
            buffer: None,
            window: None,
            tab: None,
        }
    }

    #[test]
    fn lifecycle_transitions_reject_terminal_state_reentry() {
        assert!(TimerState::Active.can_transition_to(TimerState::Firing));
        assert!(!TimerState::Stopped.can_transition_to(TimerState::Active));
        assert!(JobState::Starting.can_transition_to(JobState::Running));
        assert!(!JobState::Exited.can_transition_to(JobState::Running));
        assert!(ChannelState::Open.can_transition_to(ChannelState::Closing));
        assert!(!ChannelState::Closed.can_transition_to(ChannelState::Open));
        assert!(TerminalState::Exited.can_transition_to(TerminalState::Closed));
        assert!(!TerminalState::Closed.can_transition_to(TerminalState::Running));
    }

    #[test]
    fn semantic_ids_share_a_monotonic_non_zero_namespace() {
        let mut service = ExternalRuntimeService::new();
        assert_eq!(service.allocate_timer_id().get(), 1);
        assert_eq!(service.allocate_job_id().get(), 2);
        assert_eq!(service.allocate_channel_id().get(), 3);
        assert_eq!(service.allocate_terminal_id().get(), 4);
    }

    #[test]
    fn events_are_drained_in_admission_order() {
        let mut service = ExternalRuntimeService::new();
        let timer = service.allocate_timer_id();
        service
            .enqueue(ExternalRuntimeEvent::TimerReady {
                timer,
                generation: 1,
                owner: owner(),
            })
            .unwrap();
        service
            .enqueue(ExternalRuntimeEvent::TimerReady {
                timer,
                generation: 2,
                owner: owner(),
            })
            .unwrap();

        let events = service.drain_events();
        assert!(matches!(
            events[0],
            ExternalRuntimeEvent::TimerReady { generation: 1, .. }
        ));
        assert!(matches!(
            events[1],
            ExternalRuntimeEvent::TimerReady { generation: 2, .. }
        ));
        assert!(!service.has_ready_events());
    }

    #[test]
    fn shutdown_rejects_new_events_without_losing_queued_events() {
        let mut service = ExternalRuntimeService::new();
        let timer = service.allocate_timer_id();
        let event = ExternalRuntimeEvent::TimerReady {
            timer,
            generation: 1,
            owner: owner(),
        };
        service.enqueue(event.clone()).unwrap();
        service.begin_shutdown();

        assert!(!service.is_accepting_requests());
        assert_eq!(
            service.enqueue(event),
            Err(ExternalRuntimeEvent::TimerReady {
                timer,
                generation: 1,
                owner: owner(),
            })
        );
        assert_eq!(service.drain_events().len(), 1);
    }
}
