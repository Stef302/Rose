use std::fs;
use std::time::Instant;
fn main() {
    let a = std::env::args().nth(1).unwrap();
    let b = std::env::args().nth(2).unwrap();
    let d = Instant::now();
    let data = fs::read(&a).unwrap();
    fs::write(&b, &data).unwrap();
    let n = data.len();
    let ns = d.elapsed().as_nanos();
    println!("RESULT:{n}");
    println!("INTERNAL_NS:{ns}");
}
