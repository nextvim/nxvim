const std = @import("std");
const text = @import("text");

fn contents(buffer: *const text.Buffer, allocator: std.mem.Allocator) ![]u8 {
    return buffer.snapshot().text(allocator);
}

fn expectText(buffer: *const text.Buffer, expected: []const u8) !void {
    const actual = try contents(buffer, std.testing.allocator);
    defer std.testing.allocator.free(actual);
    try std.testing.expectEqualStrings(expected, actual);
    try buffer.validate();
}

test "operations clone their version ranges and text ownership" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(60);
    var source = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "abc");
    defer source.deinit();
    var target = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "abc");
    defer target.deinit();

    var updates = try target.subscribe();
    defer updates.deinit();
    var operation = try source.edit(&.{.{ .start = 1, .end = 2, .new_text = "owned" }});
    var clone = try operation.clone(allocator);
    operation.deinit();
    try target.applyOps(&.{clone});
    clone.deinit();
    try expectText(&target, "aownedc");
    var patch = try updates.consume();
    defer patch.deinit();
    try std.testing.expectEqual(@as(usize, 1), patch.edits().len);
    try std.testing.expectEqual(@as(usize, 1), patch.edits()[0].old.start);
    try std.testing.expectEqual(@as(usize, 2), patch.edits()[0].old.end);
    try std.testing.expectEqual(@as(usize, 6), patch.edits()[0].new.end);
}

test "causally blocked operations are sorted deduplicated and flushed" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(61);
    var source = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "x");
    defer source.deinit();
    var target = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "x");
    defer target.deinit();

    var first = try source.edit(&.{.{ .start = 1, .end = 1, .new_text = "1" }});
    defer first.deinit();
    var second = try source.edit(&.{.{ .start = 2, .end = 2, .new_text = "2" }});
    defer second.deinit();

    try target.applyOps(&.{ second, second });
    try std.testing.expectEqual(@as(usize, 1), target.deferredOperationCount());
    try expectText(&target, "x");
    try target.applyOps(&.{first});
    try std.testing.expectEqual(@as(usize, 0), target.deferredOperationCount());
    try expectText(&target, "x12");
    try target.applyOps(&.{ first, second, first });
    try expectText(&target, "x12");
}

test "concurrent insertion order converges across delivery permutations" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(62);
    var replicas: [3]text.Buffer = undefined;
    replicas[0] = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "ab");
    defer replicas[0].deinit();
    replicas[1] = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "ab");
    defer replicas[1].deinit();
    replicas[2] = try text.Buffer.init(allocator, text.clock.ReplicaId.new(10), id, "ab");
    defer replicas[2].deinit();

    var operations: [3]text.Operation = undefined;
    operations[0] = try replicas[0].edit(&.{.{ .start = 1, .end = 1, .new_text = "A" }});
    defer operations[0].deinit();
    operations[1] = try replicas[1].edit(&.{.{ .start = 1, .end = 1, .new_text = "B" }});
    defer operations[1].deinit();
    operations[2] = try replicas[2].edit(&.{.{ .start = 1, .end = 1, .new_text = "C" }});
    defer operations[2].deinit();

    try replicas[0].applyOps(&.{ operations[2], operations[1], operations[2] });
    try replicas[1].applyOps(&.{ operations[0], operations[2] });
    try replicas[2].applyOps(&.{ operations[1], operations[0], operations[1] });
    for (&replicas) |*replica| try expectText(replica, "aCBAb");
}

test "concurrent deletion and insertion converge without deleting concurrent text" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(64);
    var left = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "abc");
    defer left.deinit();
    var right = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "abc");
    defer right.deinit();

    var deletion = try left.edit(&.{.{ .start = 1, .end = 2, .new_text = "" }});
    defer deletion.deinit();
    var insertion = try right.edit(&.{.{ .start = 1, .end = 1, .new_text = "X" }});
    defer insertion.deinit();
    try left.applyOps(&.{insertion});
    try right.applyOps(&.{deletion});

    const left_text = try contents(&left, allocator);
    defer allocator.free(left_text);
    const right_text = try contents(&right, allocator);
    defer allocator.free(right_text);
    try std.testing.expectEqualStrings(left_text, right_text);
    try std.testing.expectEqualStrings("aXc", left_text);
    try left.validate();
    try right.validate();
}

test "partitioned replicas reconnect with dependent and concurrent edits" {
    const allocator = std.testing.allocator;
    const id = try text.BufferId.new(63);
    var left = try text.Buffer.init(allocator, text.clock.ReplicaId.new(8), id, "root");
    defer left.deinit();
    var right = try text.Buffer.init(allocator, text.clock.ReplicaId.new(9), id, "root");
    defer right.deinit();
    var observer = try text.Buffer.init(allocator, text.clock.ReplicaId.new(10), id, "root");
    defer observer.deinit();

    var l1 = try left.edit(&.{.{ .start = 4, .end = 4, .new_text = "L" }});
    defer l1.deinit();
    var l2 = try left.edit(&.{.{ .start = 5, .end = 5, .new_text = "2" }});
    defer l2.deinit();
    var r1 = try right.edit(&.{.{ .start = 4, .end = 4, .new_text = "R" }});
    defer r1.deinit();

    try observer.applyOps(&.{ l2, r1, l2 });
    try std.testing.expectEqual(@as(usize, 1), observer.deferredOperationCount());
    try observer.applyOps(&.{l1});
    try left.applyOps(&.{r1});
    try right.applyOps(&.{ l2, l1 });
    try observer.applyOps(&.{ r1, l1 });
    try expectText(&left, "rootRL2");
    try expectText(&right, "rootRL2");
    try expectText(&observer, "rootRL2");
}
