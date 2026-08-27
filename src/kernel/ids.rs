//! Stable identities used across command, event, and asynchronous boundaries.
//!
//! Buffer identity is owned by `vim-buffer`; window and tab identities are
//! owned by `vim-ui`. Re-exporting them here avoids introducing duplicate ID
//! types during the migration.

pub use vim_buffer::BufferId;
pub use vim_ui::{TabPageId, WindowId};

macro_rules! external_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(std::num::NonZeroU64);

        impl $name {
            pub fn new(value: u64) -> Option<Self> {
                std::num::NonZeroU64::new(value).map(Self)
            }

            pub const fn get(self) -> u64 {
                self.0.get()
            }
        }
    };
}

external_id!(TimerId);
external_id!(JobId);
external_id!(ChannelId);
external_id!(TerminalId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_runtime_ids_are_non_zero_and_round_trip() {
        assert!(TimerId::new(0).is_none());
        assert_eq!(TimerId::new(1).unwrap().get(), 1);
        assert_eq!(JobId::new(2).unwrap().get(), 2);
        assert_eq!(ChannelId::new(3).unwrap().get(), 3);
        assert_eq!(TerminalId::new(4).unwrap().get(), 4);
    }
}
