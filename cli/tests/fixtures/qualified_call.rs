use std::time::Instant;

fn main() {
    let now = time_source::Clock::now();
    let id = id_gen::Id::random();
    let start = std::time::Instant::now();
    let ordering = std::sync::atomic::Ordering::SeqCst;
    let unknown = other::Thing::new();
    let short = Instant::now();
}
