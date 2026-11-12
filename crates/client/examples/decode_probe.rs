//! Developer aid: run a recording through rodio, the plain way and the way the app does.
//! `cargo run -p bb-client --example decode_probe -- memo.m4a`
fn main() {
    let path = std::env::args().nth(1).expect("usage: decode_probe <file>");
    let bytes = std::fs::read(&path).expect("read");
    match rodio::Decoder::try_from(std::io::Cursor::new(bytes.clone())) {
        Ok(d) => println!("plain try_from: ok, {} samples", d.count()),
        Err(err) => println!("plain try_from: {err}"),
    }
    let len = bytes.len() as u64;
    match rodio::Decoder::builder().with_data(std::io::Cursor::new(bytes)).with_byte_len(len).with_seekable(true).build() {
        Ok(d) => println!("seekable with length (the app): ok, {} samples", d.count()),
        Err(err) => println!("seekable with length (the app): {err}"),
    }
}
