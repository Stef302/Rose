use std::collections::HashMap;
use std::fs;
use std::time::Instant;
fn fib(n: i64) -> i64 { if n < 2 { return n; } fib(n-1)+fib(n-2) }
fn main() {
 let t0 = Instant::now();
 let mut total: i64 = 0; for i in 1..=10_000_000i64 { total += i*i; } println!("{}", total);
 let mut x: f64 = 0.0; for i in 1..=1_000_000 { x += 1.0/i as f64; } println!("{}", x);
 let mut r: f64 = 1.0/3.0; for i in 1..=1000 { r += 1.0/i as f64; } println!("{}", r);
 let t = "ete radieux"; let sub = &t[..3]; let mut s = String::new(); for _ in 0..1000 { s = format!("{}{}", sub, s); } println!("{}", s.chars().count());
 let mut piles: Vec<i64> = vec![0]; for i in 1..=10000 { piles.push(i); } println!("{}", piles.len()); println!("{}", piles[5000]);
 let mut tab: HashMap<String,i64> = HashMap::with_capacity(5000); for i in 1..=5000 { tab.insert(format!("k{}", i), (i as i64)*(i as i64)); } println!("{}", tab.len()); println!("{}", tab.get("k2500").copied().unwrap_or(0));
 println!("{}", fib(28));
 let alpha = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";
 let mut buf = vec![0u8; 1_000_000]; let mut st: u64 = 0xC0FFEE;
 for b in buf.iter_mut() { st = st.wrapping_add(0x9e3779b97f4a7c15); let mut z = st; z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9); z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111b9); z ^= z >> 31; *b = alpha[(z & 63) as usize]; }
 println!("{}", buf.len());
 fs::write("global_out.txt", "bench global Rust").unwrap();
 println!("{}", fs::read_to_string("global_out.txt").unwrap());
 let ms = t0.elapsed().as_secs_f64() * 1000.0; println!("{}", ms);
}
