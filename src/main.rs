use std::io::{self, Read};

use anyhow::Context;

const WIDE_SPACE: char = '\u{3000}';
const FULLWIDTH_OFFSET: u32 = 0xFEE0;

fn fw_char(c: char) -> char {
    match c {
        ' ' => WIDE_SPACE,
        '!'..='~' => char::from_u32((c as u32) + FULLWIDTH_OFFSET).unwrap(),
        _ => c,
    }
}

fn run() -> anyhow::Result<()> {
    let mut text = String::new();
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some() {
        while let Some(arg) = args.next() {
            text.extend(arg.chars().map(fw_char));
            if args.peek().is_some() {
                text.push(WIDE_SPACE);
            }
        }
    } else {
        let mut input = String::new();
        io::stdin()
            .lock()
            .read_to_string(&mut input)
            .context("failed to read stdin")?;

        if input.ends_with('\n') {
            input.pop();
        }
        text.extend(input.chars().map(fw_char));
    }
    println!("{text}");

    arboard::Clipboard::new()
        .context("failed to init clipboard")?
        .set_text(&text)
        .context("failed to set clipboard contents")?;

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}
