//! Logical clocks and version vectors used by distributed Zed packages.

pub const logical = @import("clock.zig");
pub const system = @import("system_clock.zig");

pub const ReplicaId = logical.ReplicaId;
pub const Seq = logical.Seq;
pub const Lamport = logical.Lamport;
pub const Global = logical.Global;
pub const RealSystemClock = system.RealSystemClock;
pub const FakeSystemClock = system.FakeSystemClock;

pub const baseline = struct {
    pub const zig = "0.16.0";
    pub const zed_revision = "7a9ce83c781e725cb45940a8772527a991d4f9a4";
};
