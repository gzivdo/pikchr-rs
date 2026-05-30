use std::io::Read;
fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    match pikchr_rs::pikchr(&s, Default::default()) {
        Ok(svg) => print!("{svg}"),
        Err(e) => eprintln!("ERR: {e}"),
    }
}
