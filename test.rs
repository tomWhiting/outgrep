fn main() {
    let x = 42;
    if x > 0 {
        println\!("positive");
    }
    let result = match x {
        0 => "zero",
        _ => "other"
    };
}
