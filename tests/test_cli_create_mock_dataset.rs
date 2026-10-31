use clap::Parser;

#[test]
fn test_cli_create_mock_dataset_command_parses() {
    let args = [
        "avicull",
        "create-mock-dataset",
        "--source",
        "/tmp/some_source",
        "--target",
        "/tmp/some_target",
        "--force",
    ];

    let cli = avicull::cli::Cli::try_parse_from(args).expect("Failed to parse create-mock-dataset");
    match cli.command {
        avicull::cli::Commands::CreateMockDataset(cmd_args) => {
            assert_eq!(cmd_args.source.to_str().unwrap(), "/tmp/some_source");
            assert_eq!(
                cmd_args.target.unwrap().to_str().unwrap(),
                "/tmp/some_target"
            );
            assert!(cmd_args.force);
        }
        _ => panic!("Expected Commands::CreateMockDataset"),
    }
}
