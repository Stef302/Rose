use std::time::Instant;
fn main() {
    let d = Instant::now();
    let ns = d.elapsed().as_nanos();
    println!("RESULT:0");
    println!("INTERNAL_NS:{ns}");
}
