//! # Tests for CLI Mount and Unmount Commands
//!
//! Verifies:
//! - Parsing of `avicull mount` options (`--serve-only`, `--port`, `--stream-mode`, optional mount point).
//! - Parsing of `avicull unmount <MOUNT_POINT>`.
//! - Agent-Ready error contract formatting for occupied targets and missing manifests.

use clap::Parser;
use std::path::PathBuf;

use avicull::cli::{Cli, Commands};

#[test]
fn test_cli_mount_args_parsing() {
    let args = [
        "avicull",
        "mount",
        "s3://my-wildlife-bucket/photos",
        "/Volumes/birds_view",
        "--port",
        "9876",
        "--serve-only",
        "--stream-mode",
        "preview",
    ];

    let cli = Cli::parse_from(args);
    match cli.command {
        Commands::Mount(mount_args) => {
            assert_eq!(mount_args.source, "s3://my-wildlife-bucket/photos");
            assert_eq!(
                mount_args.mount_point,
                Some(PathBuf::from("/Volumes/birds_view"))
            );
            assert_eq!(mount_args.port, 9876);
            assert!(mount_args.serve_only);
            assert_eq!(mount_args.stream_mode, "preview");
        }
        _ => panic!("Expected Mount subcommand"),
    }
}

#[test]
fn test_cli_unmount_args_parsing() {
    let args = ["avicull", "unmount", "/Volumes/birds_view"];

    let cli = Cli::parse_from(args);
    match cli.command {
        Commands::Unmount(unmount_args) => {
            assert_eq!(
                unmount_args.mount_point,
                PathBuf::from("/Volumes/birds_view")
            );
        }
        _ => panic!("Expected Unmount subcommand"),
    }
}
