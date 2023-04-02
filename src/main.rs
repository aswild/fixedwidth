use std::io::Read;

use anyhow::Context;
use arboard::{Clipboard, SetExtLinux};
use clap::{Arg, ArgAction, ArgGroup};

const WIDE_SPACE: char = '\u{3000}';
const FULLWIDTH_OFFSET: u32 = 0xFEE0;

fn fw_char(c: char) -> char {
    match c {
        ' ' => WIDE_SPACE,
        '!'..='~' => char::from_u32((c as u32) + FULLWIDTH_OFFSET).unwrap(),
        _ => c,
    }
}

#[derive(Debug, Clone, Copy)]
enum WaitMode {
    NoWait,
    Foreground,
    Background,
}

fn set_clipboard(text: &str, wait: WaitMode) -> anyhow::Result<()> {
    /// Inner function to do *all* of the clipboard stuff, but without any fork shennanigans. This
    /// may run in the main parent or child process.
    fn inner(text: &str, wait: bool) -> anyhow::Result<()> {
        let mut cb = Clipboard::new().context("failed to init clipboard")?;
        let mut set = cb.set();
        if wait {
            set = set.wait();
        }
        set.text(text).context("failed to set clipboard contents")
    }

    match wait {
        WaitMode::NoWait => inner(text, false),
        WaitMode::Foreground => inner(text, true),
        WaitMode::Background => {
            // Fork to the background, then set the clipboard and wait in the background process.
            // The parent will return Ok immediately unless fork failed.
            //
            // This is just a single fork and then disown, we don't do setsid() and double-fork
            // like a "proper" daemon, because it doesn't seem necessary. We also keep stdio open
            // so we can print errors if needed.
            //
            // SAFETY: "After a fork() in a multithreaded program, the child can safely call only
            // async-signal-safe functions until it calls execve(2)". This translates to: we MUST
            // fork only when the process is single-threaded. Specifically, we MUST NOT initialize
            // or touch any of the clipboard handling in the parent and then use it from the child,
            // because arboard spawns a helper thread for X11 clipboard handling. As long as we're
            // single threaded at this point, the fork is safe, and all subsequent threads are only
            // used in the context of the child process.
            match unsafe { libc::fork() } {
                // fork failed
                -1 => Err(std::io::Error::last_os_error()).context("fork failed"),

                // child process, set the clipboard and exit.
                0 => {
                    let retcode = match inner(text, true) {
                        Ok(()) => 0,
                        Err(err) => {
                            eprintln!("fw clipboard error: {err:#}");
                            1
                        }
                    };
                    std::process::exit(retcode);
                }

                // parent process, return success immediately, implicitly disown the child.
                _child_pid => Ok(()),
            }
        }
    }
}

fn env_is_nonempty(var: &str) -> bool {
    match std::env::var_os(var) {
        Some(val) => !val.is_empty(),
        None => false,
    }
}

fn run() -> anyhow::Result<()> {
    let args = clap::command!()
        .about("Convert text to fullwidth glyphs (for cate memes)")
        .arg(
            Arg::new("no-clipboard")
                .short('n')
                .long("no-clipboard")
                .action(ArgAction::SetTrue)
                .help("Don't copy the output to the system clipboard"),
        )
        .arg(
            Arg::new("no-wait")
                .short('W')
                .long("no-wait")
                .action(ArgAction::SetTrue)
                .help(
                    "Don't wait for the clipboard to be reset before exiting. (This is the \
                       default if a desktop session is detected)",
                ),
        )
        .arg(
            Arg::new("foreground-wait")
                .short('F')
                .long("foreground-wait")
                .action(ArgAction::SetTrue)
                .help(
                    "Wait for the clipboard to be reset in the foreground rather than \
                       forking to the background",
                ),
        )
        .arg(
            Arg::new("text")
                .action(ArgAction::Append)
                .required(false)
                .help(
                    "Text strings to convert. Arguments will be joined with spaces. \
                      Omit to read stdin instead.",
                ),
        )
        .group(
            // our clipboard arguments are multually-exclusive
            ArgGroup::new("clipboard-args")
                .args(["no-clipboard", "no-wait", "foreground-wait"])
                .required(false)
                .multiple(false),
        )
        .get_matches();

    let mut text = String::new();
    if args.contains_id("text") {
        let mut words = args.get_many::<String>("text").unwrap().peekable();
        while let Some(word) = words.next() {
            text.extend(word.chars().map(fw_char));
            if words.peek().is_some() {
                text.push(WIDE_SPACE);
            }
        }
    } else {
        let mut input = String::new();
        std::io::stdin()
            .lock()
            .read_to_string(&mut input)
            .context("failed to read stdin")?;

        if input.ends_with('\n') {
            input.pop();
        }
        text.extend(input.chars().map(fw_char));
    }
    println!("{text}");

    if !args.get_flag("no-clipboard") && env_is_nonempty("DISPLAY") {
        let mode = if args.get_flag("no-wait") {
            WaitMode::NoWait
        } else if args.get_flag("foreground-wait") {
            WaitMode::Foreground
        } else if env_is_nonempty("XDG_CURRENT_DESKTOP") {
            // In Gnome, it seems like we can get away with setting the clipboard then immediately
            // exiting. I guess something else in the desktop session picks it up.
            // TODO verify that this is the right env var to check
            WaitMode::NoWait
        } else {
            // by default if we don't think we're in a desktop session, fork to the background to
            // wait and serve clipboard requests.
            WaitMode::Background
        };

        set_clipboard(&text, mode)?;
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}
