use std::{hint::black_box, time::Instant};
use vim_buffer::{BufferManager, ByteOffset, EditOrigin, Point, TextRange};

fn measure(name: &str, iterations: usize, mut operation: impl FnMut()) {
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let elapsed = started.elapsed();
    println!("{name}: {iterations} iterations in {elapsed:?}");
}

fn main() {
    let text = (0..100_000)
        .map(|row| format!("row {row}: αβγ 😀 example text\n"))
        .collect::<String>();
    let mut manager = BufferManager::new();
    let buffer = manager.create(text);

    measure("snapshot clone", 10_000, || {
        black_box(buffer.snapshot());
    });

    let snapshot = buffer.snapshot();
    measure("line lookup", 100_000, || {
        black_box(snapshot.point_to_offset(Point::new(75_000, 8)).unwrap());
    });

    let end = snapshot.len_bytes();
    measure("batched insert/delete", 1_000, || {
        let mut insert = buffer.transaction(EditOrigin::User);
        insert.insert(None, ByteOffset(end), "x");
        black_box(insert.commit(None).unwrap());

        let mut delete = buffer.transaction(EditOrigin::User);
        delete.delete(
            None,
            TextRange::new(ByteOffset(end), ByteOffset(end + 1)).unwrap(),
        );
        black_box(delete.commit(None).unwrap());
    });
}
