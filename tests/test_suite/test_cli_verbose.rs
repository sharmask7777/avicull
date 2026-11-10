//! Tests for --verbose / -v CLI flag parsing across organize and long-running subcommands.

use avicull::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn test_organize_args_verbose_flag() {
    // Default is false
    let cli_default = Cli::parse_from(["avicull", "organize", "--input", "test_input"]);
    if let Commands::Organize(args) = cli_default.command {
        assert!(!args.verbose, "verbose should default to false");
    } else {
        panic!("Expected Commands::Organize");
    }

    // Long flag --verbose
    let cli_long = Cli::parse_from(["avicull", "organize", "--input", "test_input", "--verbose"]);
    if let Commands::Organize(args) = cli_long.command {
        assert!(args.verbose, "--verbose should set verbose to true");
    } else {
        panic!("Expected Commands::Organize");
    }

    // Short flag -v
    let cli_short = Cli::parse_from(["avicull", "organize", "--input", "test_input", "-v"]);
    if let Commands::Organize(args) = cli_short.command {
        assert!(args.verbose, "-v should set verbose to true");
    } else {
        panic!("Expected Commands::Organize");
    }
}

#[test]
fn test_scan_args_verbose_flag() {
    let cli_default = Cli::parse_from(["avicull", "scan", "--input", "test_input"]);
    if let Commands::Scan(args) = cli_default.command {
        assert!(!args.verbose, "verbose should default to false");
    } else {
        panic!("Expected Commands::Scan");
    }

    let cli_long = Cli::parse_from(["avicull", "scan", "--input", "test_input", "--verbose"]);
    if let Commands::Scan(args) = cli_long.command {
        assert!(args.verbose, "--verbose should set verbose to true");
    } else {
        panic!("Expected Commands::Scan");
    }

    let cli_short = Cli::parse_from(["avicull", "scan", "--input", "test_input", "-v"]);
    if let Commands::Scan(args) = cli_short.command {
        assert!(args.verbose, "-v should set verbose to true");
    } else {
        panic!("Expected Commands::Scan");
    }
}

#[test]
fn test_relink_args_verbose_flag() {
    let cli_default = Cli::parse_from(["avicull", "relink"]);
    if let Commands::Relink(args) = cli_default.command {
        assert!(!args.verbose, "verbose should default to false");
    } else {
        panic!("Expected Commands::Relink");
    }

    let cli_long = Cli::parse_from(["avicull", "relink", "--verbose"]);
    if let Commands::Relink(args) = cli_long.command {
        assert!(args.verbose, "--verbose should set verbose to true");
    } else {
        panic!("Expected Commands::Relink");
    }

    let cli_short = Cli::parse_from(["avicull", "relink", "-v"]);
    if let Commands::Relink(args) = cli_short.command {
        assert!(args.verbose, "-v should set verbose to true");
    } else {
        panic!("Expected Commands::Relink");
    }
}

#[test]
fn test_clean_args_verbose_flag() {
    let cli_default = Cli::parse_from(["avicull", "clean"]);
    if let Commands::Clean(args) = cli_default.command {
        assert!(!args.verbose, "verbose should default to false");
    } else {
        panic!("Expected Commands::Clean");
    }

    let cli_long = Cli::parse_from(["avicull", "clean", "--verbose"]);
    if let Commands::Clean(args) = cli_long.command {
        assert!(args.verbose, "--verbose should set verbose to true");
    } else {
        panic!("Expected Commands::Clean");
    }

    let cli_short = Cli::parse_from(["avicull", "clean", "-v"]);
    if let Commands::Clean(args) = cli_short.command {
        assert!(args.verbose, "-v should set verbose to true");
    } else {
        panic!("Expected Commands::Clean");
    }
}

#[test]
fn test_organize_cli_outputs_verbose_progress() -> anyhow::Result<()> {
    use std::fs::{self, File};
    use std::io::Write;
    use std::process::Command;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let input = dir.path().join("trip_verbose");
    fs::create_dir_all(&input)?;

    // Create a mock image
    let img_path = input.join("IMG_1001.JPG");
    File::create(&img_path)?.write_all(b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00\xFF\xDB\x00C\x00\xFF\xD9")?;

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("organize")
        .arg("--input")
        .arg(&input)
        .arg("--mode")
        .arg("in-place")
        .arg("--no-prompt")
        .arg("--verbose")
        .output()?;

    assert!(output.status.success(), "organize must exit successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify key verbose progress checkpoints exist in stdout
    assert!(
        stdout.contains("Scanning source directory"),
        "stdout should inform user about scanning stage: {stdout}"
    );
    assert!(
        stdout.contains("Resolving EXIF metadata"),
        "stdout should inform user about EXIF / GPS resolution: {stdout}"
    );
    assert!(
        stdout.contains("Applying burst clustering"),
        "stdout should inform user about burst clustering: {stdout}"
    );
    assert!(
        stdout.contains("Generating standalone offline HTML photo catalog"),
        "stdout should inform user about catalog generation: {stdout}"
    );

    Ok(())
}

#[test]
fn test_scan_cli_outputs_verbose_progress() -> anyhow::Result<()> {
    use std::fs::{self, File};
    use std::io::Write;
    use std::process::Command;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let input = dir.path().join("trip_scan_verbose");
    fs::create_dir_all(&input)?;

    let img_path = input.join("IMG_2001.JPG");
    File::create(&img_path)?.write_all(
        b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00\xFF\xD9",
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .arg("scan")
        .arg("--input")
        .arg(&input)
        .arg("--verbose")
        .output()?;

    assert!(output.status.success(), "scan must exit successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Scanning source directory"),
        "scan stdout should inform user about scanning: {stdout}"
    );
    assert!(
        stdout.contains("Resolving EXIF metadata"),
        "scan stdout should inform user about metadata resolution: {stdout}"
    );

    Ok(())
}
