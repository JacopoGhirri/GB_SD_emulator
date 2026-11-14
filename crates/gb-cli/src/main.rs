//! Headless runner: loads a ROM, runs it for a limited time, and reports what happened.
//! All file and terminal I/O lives here; `gb-core` never touches the filesystem.

mod image;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use gb_core::mooneye::{run_mooneye, MooneyeVerdict};
use gb_core::{Cartridge, GameBoy, Model};

/// Exit codes. Distinct values make failures easy to tell apart in scripts and CI.
const EXIT_OK: u8 = 0;
const EXIT_FAILED: u8 = 1;
const EXIT_UNSUPPORTED: u8 = 2;

/// How many bytes of serial output to show when a check fails.
const SERIAL_TAIL_BYTES: usize = 256;

#[derive(Parser)]
#[command(
    name = "gb-cli",
    version,
    about = "Headless Game Boy runner for tests and tracing"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Load a ROM, print its header, and optionally run it with checks.
    Run(RunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Path to a .gb or .gbc ROM file.
    rom: PathBuf,

    /// Emulated hardware model.
    #[arg(long, value_enum, default_value_t = ModelArg::Dmg)]
    model: ModelArg,

    /// Stop after this many video frames.
    #[arg(long, value_name = "N")]
    frames: Option<u64>,

    /// Stop after this many CPU instructions.
    #[arg(long, value_name = "N")]
    max_instructions: Option<u64>,

    /// Print the serial output (what the ROM sent over the link port) to stdout.
    #[arg(long)]
    serial: bool,

    /// Exit 0 only if this text appears in the serial output. Blargg tests print "Passed".
    #[arg(long, value_name = "TEXT")]
    expect_serial: Option<String>,

    /// Run until `ld b,b` and check the Mooneye Fibonacci signature (needs --max-instructions).
    #[arg(long)]
    mooneye: bool,

    /// Write the final framebuffer to this PNG file.
    #[arg(long, value_name = "FILE")]
    screenshot: Option<PathBuf>,

    /// Compare the final framebuffer with this reference PNG. Exit non-zero on any difference.
    #[arg(long, value_name = "FILE")]
    compare: Option<PathBuf>,

    /// Write a Gameboy Doctor trace, one line per instruction (needs --max-instructions).
    /// Also makes LY read 0x90 and IF read without its unused bits, as Doctor expects.
    #[arg(long, value_name = "FILE")]
    trace: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModelArg {
    Dmg,
    Cgb,
}

impl From<ModelArg> for Model {
    fn from(m: ModelArg) -> Model {
        match m {
            ModelArg::Dmg => Model::Dmg,
            ModelArg::Cgb => Model::Cgb,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => ExitCode::from(run(&args)),
    }
}

fn run(args: &RunArgs) -> u8 {
    let rom = match std::fs::read(&args.rom) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", args.rom.display());
            return EXIT_FAILED;
        }
    };
    let cart = match Cartridge::from_bytes(rom) {
        Ok(cart) => cart,
        Err(e) => {
            eprintln!("error: {}: {e}", args.rom.display());
            return EXIT_FAILED;
        }
    };
    print_header(&cart);

    // Features that are not implemented fail loudly rather than printing fake data.
    if args.trace.is_some() && args.max_instructions.is_none() {
        eprintln!("error: --trace needs --max-instructions to know when to stop");
        return EXIT_UNSUPPORTED;
    }

    let mut gb = GameBoy::new(cart, args.model.into());
    if args.mooneye {
        let Some(max) = args.max_instructions else {
            eprintln!("error: --mooneye needs --max-instructions as a time limit");
            return EXIT_UNSUPPORTED;
        };
        let (verdict, n) = run_mooneye(&mut gb, max);
        return match verdict {
            MooneyeVerdict::Pass => {
                println!("mooneye: PASS after {n} instructions");
                EXIT_OK
            }
            MooneyeVerdict::Fail(regs) => {
                eprintln!("mooneye: FAIL after {n} instructions, B,C,D,E,H,L = {regs:02X?}");
                EXIT_FAILED
            }
            MooneyeVerdict::Timeout => {
                eprintln!(
                    "mooneye: TIMEOUT, no LD B,B within {n} instructions, PC=0x{:04X}",
                    gb.pc()
                );
                EXIT_FAILED
            }
        };
    }
    let needs_run = args.frames.is_some() || args.max_instructions.is_some();
    if !needs_run {
        // Header-only mode: no stop condition was given, so there is nothing to run.
        return EXIT_OK;
    }

    let mut trace_out = match &args.trace {
        Some(path) => match std::fs::File::create(path) {
            Ok(file) => Some(std::io::BufWriter::new(file)),
            Err(e) => {
                eprintln!("error: cannot create {}: {e}", path.display());
                return EXIT_FAILED;
            }
        },
        None => None,
    };
    if trace_out.is_some() {
        gb.set_doctor_mode(true);
    }
    let mut serial = Vec::new();
    let mut instructions: u64 = 0;

    if let Some(frames) = args.frames {
        for _ in 0..frames {
            gb.run_frame();
            serial.extend(gb.take_serial());
        }
    }
    if let Some(max) = args.max_instructions {
        while instructions < max {
            if let Some(out) = trace_out.as_mut() {
                if gb.is_trace_point() {
                    if let Err(e) = writeln!(out, "{}", gb.trace_line()) {
                        eprintln!("error: writing trace: {e}");
                        return EXIT_FAILED;
                    }
                }
            }
            gb.step();
            instructions += 1;
            serial.extend(gb.take_serial());
        }
    }

    if let Some(out) = trace_out.as_mut() {
        if let Err(e) = out.flush() {
            eprintln!("error: writing trace: {e}");
            return EXIT_FAILED;
        }
    }

    if args.serial {
        print!("{}", String::from_utf8_lossy(&serial));
    }

    let stop = format!(
        "stopped after {instructions} instructions, PC=0x{:04X}",
        gb.pc()
    );
    let mut status = EXIT_OK;

    if let Some(expected) = &args.expect_serial {
        let text = String::from_utf8_lossy(&serial);
        if text.contains(expected.as_str()) {
            println!("serial check passed: found {expected:?}");
        } else {
            eprintln!("serial check FAILED: {expected:?} not found. {stop}");
            eprintln!("last serial output:\n{}", tail(&serial));
            status = EXIT_FAILED;
        }
    }

    if let Some(path) = &args.screenshot {
        if let Err(e) = image::write_png(path, gb.framebuffer()) {
            eprintln!("error: writing {}: {e}", path.display());
            status = EXIT_FAILED;
        } else {
            println!("screenshot written to {}", path.display());
        }
    }

    if let Some(path) = &args.compare {
        match image::compare_with_png(path, gb.framebuffer()) {
            Ok(0) => println!("compare passed: final frame matches {}", path.display()),
            Ok(n) => {
                eprintln!(
                    "compare FAILED: {n} pixels differ from {}. {stop}",
                    path.display()
                );
                status = EXIT_FAILED;
            }
            Err(e) => {
                eprintln!("error: comparing with {}: {e}", path.display());
                status = EXIT_FAILED;
            }
        }
    }

    status
}

fn print_header(cart: &Cartridge) {
    let h = cart.header();
    println!("title:        {}", h.title);
    println!(
        "cgb flag:     0x{:02X} ({})",
        h.cgb_flag,
        cgb_description(h.cgb_flag)
    );
    println!(
        "cart type:    0x{:02X} {}",
        h.cart_type.0,
        h.cart_type.name()
    );
    println!("rom size:     {} bytes", h.rom_size);
    println!("ram size:     {} bytes", h.ram_size);
    println!(
        "header sum:   0x{:02X} stored, 0x{:02X} computed -> {}",
        h.checksum_stored,
        h.checksum_computed,
        if h.checksum_ok { "OK" } else { "BAD" }
    );
}

fn cgb_description(flag: u8) -> &'static str {
    match flag {
        0xC0 => "CGB only",
        0x80 => "CGB enhanced, also runs on DMG",
        _ => "DMG only",
    }
}

/// The last `SERIAL_TAIL_BYTES` of serial output, as text.
fn tail(serial: &[u8]) -> String {
    let start = serial.len().saturating_sub(SERIAL_TAIL_BYTES);
    String::from_utf8_lossy(&serial[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        // Catches clap misconfiguration (duplicate flags, bad defaults) at test time.
        <Cli as clap::CommandFactory>::command().debug_assert();
    }

    #[test]
    fn parses_run_with_stop_and_check_options() {
        let cli = Cli::try_parse_from([
            "gb-cli",
            "run",
            "x.gb",
            "--max-instructions",
            "100",
            "--expect-serial",
            "Passed",
        ])
        .unwrap();
        let Command::Run(args) = cli.command;
        assert_eq!(args.max_instructions, Some(100));
        assert_eq!(args.expect_serial.as_deref(), Some("Passed"));
        assert!(args.frames.is_none());
    }

    #[test]
    fn tail_keeps_only_the_end() {
        let data: Vec<u8> = (0..1000u32).map(|i| b'a' + (i % 26) as u8).collect();
        assert_eq!(tail(&data).len(), SERIAL_TAIL_BYTES);
        assert_eq!(tail(b"short"), "short");
    }
}
