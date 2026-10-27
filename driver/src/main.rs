use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut config_path: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("vader3p-steam {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-c" | "--config" => match args.next() {
                Some(p) => config_path = Some(PathBuf::from(p)),
                None => {
                    eprintln!("error: --config needs a path");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("error: unknown argument {other:?}");
                eprintln!("try --help");
                return ExitCode::from(2);
            }
        }
    }

    match vader3p_steam::run(config_path.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "\
vader3p-steam: translate a Flydigi Vader 3 Pro to a virtual gamepad

USAGE:
    vader3p-steam [OPTIONS]

OPTIONS:
    -c, --config <PATH>    configuration file
                           (default: $XDG_CONFIG_HOME/vader3p-steam/config.toml)
    -h, --help             show this help
    -V, --version          show the version

The controller must already be connected. Bluetooth DInput is the supported
transport; rumble needs `bluetooth_address` in the configuration because it is
delivered over the Nordic UART characteristic rather than HID output reports.

Requires access to /dev/uinput and to the controller's hidraw node. See the
project README for the scoped permission setup."
    );
}
