fn compute() -> u32 {
    7
}

fn main() {
    let value = dbg!(compute());
    assert_eq!(value, 7);
}
