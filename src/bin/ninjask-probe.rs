//! The declared-state layer as a process: length-prefixed JSON frames in, length-prefixed JSON frames out.
//!
//! `ninjask-probe --dex FORMAT_EXPORT.json [--in PATH --out PATH]` loads one format export (the engine refuses one that is not
//! from its pinned Showdown commit) and serves `declared::wire` requests until its input closes. With no `--in`/`--out` it
//! reads standard input and writes standard output; the paths name named pipes the caller made, opened here, for a caller
//! that needs blocking reads. A frame is a 4-byte little-endian length and that many bytes of JSON. The process never
//! panics on a frame it cannot read (it answers with an error); a panic inside the engine ends the process and the caller
//! sees the stream close, and routes to the reference probe.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use ninjask::declared::{wire, Prober};
use ninjask::Dex;

/// The largest frame accepted (a batch of tens of thousands of queries is a few megabytes).
const MAX_FRAME: usize = 256 << 20;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let Some(dex_path) = flag("--dex") else {
        eprintln!("usage: ninjask-probe --dex FORMAT_EXPORT.json [--in PATH --out PATH]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&dex_path).unwrap_or_else(|e| {
        eprintln!("ninjask-probe: cannot read the format export: {e}");
        std::process::exit(2);
    });
    let dex = Dex::from_json(&text).unwrap_or_else(|e| {
        eprintln!("ninjask-probe: {e}");
        std::process::exit(2);
    });
    let mut prober = Prober::new(Arc::new(dex));
    let mut input: Box<dyn Read> = match flag("--in") {
        Some(p) => Box::new(BufReader::new(File::open(p).expect("open --in"))),
        None => Box::new(BufReader::new(std::io::stdin())),
    };
    let mut output: Box<dyn Write> = match flag("--out") {
        Some(p) => Box::new(BufWriter::new(
            File::options().write(true).open(p).expect("open --out"),
        )),
        None => Box::new(BufWriter::new(std::io::stdout())),
    };
    let mut len = [0u8; 4];
    let mut frame = Vec::new();
    loop {
        match input.read_exact(&mut len) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return,
            Err(e) => {
                eprintln!("ninjask-probe: {e}");
                std::process::exit(1);
            }
        }
        let n = u32::from_le_bytes(len) as usize;
        if n > MAX_FRAME {
            eprintln!("ninjask-probe: a frame of {n} bytes is over the limit");
            std::process::exit(1);
        }
        frame.resize(n, 0);
        if input.read_exact(&mut frame).is_err() {
            eprintln!("ninjask-probe: the input ended inside a frame");
            std::process::exit(1);
        }
        let response = wire::serve(&mut prober, &frame);
        let ok = output
            .write_all(&(response.len() as u32).to_le_bytes())
            .and_then(|()| output.write_all(&response))
            .and_then(|()| output.flush());
        if ok.is_err() {
            return;
        }
    }
}
