//! dio CLI — JavaScript deobfuscation from the command line.

use std::fs;
use std::io::{self, Read};
use std::process;

use clap::Parser;
use dio_core::{Deobfuscator, Preset};

/// dio — JavaScript deobfuscation tool.
///
/// Reads obfuscated JavaScript, applies deobfuscation transforms,
/// and outputs the cleaned result.
#[derive(Parser)]
#[command(name = "dio", version, about)]
struct Arguments {
    /// Input file path. Use "-" to read from stdin.
    input: String,

    /// Output file path. Defaults to stdout.
    #[arg(short, long)]
    output: Option<String>,

    /// Maximum number of transform iterations (default: unlimited).
    #[arg(long)]
    max_iterations: Option<usize>,

    /// Transformer preset for a specific obfuscation tool.
    /// Options: generic (default), obfuscator-io, datadome, jsfuck.
    #[arg(long, default_value = "generic")]
    preset: String,

    /// Print transform diagnostics to stderr.
    #[arg(long)]
    diagnostics: bool,
}

fn main() {
    let arguments = Arguments::parse();

    // Read input.
    let source = if arguments.input == "-" {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .unwrap_or_else(|error| {
                eprintln!("Error reading stdin: {error}");
                process::exit(1);
            });
        buffer
    } else {
        fs::read_to_string(&arguments.input).unwrap_or_else(|error| {
            eprintln!("Error reading {}: {error}", arguments.input);
            process::exit(1);
        })
    };

    // Resolve preset.
    let preset = Preset::from_name(&arguments.preset).unwrap_or_else(|| {
        eprintln!(
            "Unknown preset '{}'. Available: {}",
            arguments.preset,
            Preset::all_names().join(", ")
        );
        process::exit(1);
    });

    // Build deobfuscator.
    let mut deobfuscator = Deobfuscator::with_preset(preset);
    if let Some(max_iterations) = arguments.max_iterations {
        deobfuscator = deobfuscator.with_max_iterations(max_iterations);
    }

    if arguments.diagnostics {
        deobfuscator = deobfuscator.with_diagnostics_callback(|diagnostics| {
            eprintln!("{diagnostics}");
        });
    }

    // Run deobfuscation.
    let result = deobfuscator.deobfuscate(&source);

    // Write output.
    if let Some(output_path) = &arguments.output {
        fs::write(output_path, &result).unwrap_or_else(|error| {
            eprintln!("Error writing {output_path}: {error}");
            process::exit(1);
        });
    } else {
        print!("{result}");
    }
}
