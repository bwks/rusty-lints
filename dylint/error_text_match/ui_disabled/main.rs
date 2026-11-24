use std::io;

fn main() {
    let error = io::Error::other("out of space");
    if error.to_string().contains("space") {}
}
