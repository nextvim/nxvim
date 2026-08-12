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
    pub const zed_revision = "90d024b88abc91264d9a0ad260eb4f365fa695c3";
};
