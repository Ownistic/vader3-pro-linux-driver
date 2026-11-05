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

Bluetooth DInput controllers are detected automatically, including hotplug and
multiple controllers. The daemon waits when none are connected. Set the optional
`bluetooth_address` configuration value to restrict operation to one controller.
Rumble uses Nordic UART rather than HID output reports; no pairing is performed.

Requires access to the controller's hidraw and evdev nodes and /dev/uhid.
See the README for permissions."
    );
}
