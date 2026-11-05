//! Bounded worker-level diagnostic for the configured BLE motion path.
//!
//! The worker first listens passively, then may send one AC 01 per subscription.
//! It does not create a virtual pad, send rumble, or issue AC off.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use vader3p_steam::ble_worker::{ActivationState, BleWorker};
use vader3p_steam::config::{Backend, Config};
use vader3p_steam::motion::RawMotion;

struct Options {
    config: PathBuf,
    seconds: u64,
}

fn usage() -> &'static str {
    "Usage: ble_motion_probe --config PATH [--seconds 1..30]\n\
     Bounded worker diagnostic (default observation time: 8 seconds)."
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut args = env::args().skip(1);
    let mut config = None;
    let mut seconds = 8;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--config requires a path".to_string())?;
                config = Some(PathBuf::from(path));
            }
            "--seconds" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--seconds requires an integer".to_string())?;
                seconds = value
                    .parse::<u64>()
                    .map_err(|_| "--seconds must be an integer from 1 to 30".to_string())?;
                if !(1..=30).contains(&seconds) {
                    return Err("--seconds must be an integer from 1 to 30".into());
                }
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let config = config.ok_or_else(|| "--config PATH is required".to_string())?;
    Ok(Some(Options { config, seconds }))
}

#[derive(Default)]
struct RawSummary {
    first_min: Option<i16>,
    first_max: Option<i16>,
    second_min: Option<i16>,
    second_max: Option<i16>,
    samples: u64,
    distinct_changes: u64,
    last: Option<RawMotion>,
}

impl RawSummary {
    fn observe(&mut self, sample: RawMotion) {
        self.first_min = Some(self.first_min.map_or(sample.first, |v| v.min(sample.first)));
        self.first_max = Some(self.first_max.map_or(sample.first, |v| v.max(sample.first)));
        self.second_min = Some(
            self.second_min
                .map_or(sample.second, |v| v.min(sample.second)),
        );
        self.second_max = Some(
            self.second_max
                .map_or(sample.second, |v| v.max(sample.second)),
        );
        self.samples = self.samples.saturating_add(1);
        if self.last.is_some_and(|last| last != sample) {
            self.distinct_changes = self.distinct_changes.saturating_add(1);
        }
        self.last = Some(sample);
    }
}

fn run(options: Options) -> ExitCode {
    let config = match Config::load(&options.config) {
        Ok(config) => config,
        Err(_) => {
            eprintln!("Error: could not load or validate configuration.");
            return ExitCode::FAILURE;
        }
    };
    if config.pad.backend != Backend::Uhid
        || !config.motion.enabled
        || config.bluetooth_address.is_none()
    {
        eprintln!("Error: config must opt in to uhid motion and include a Bluetooth address.");
        return ExitCode::FAILURE;
    }
    let address = config.bluetooth_address.expect("checked above");
    println!(
        "BLE worker diagnostic; passive first, at most one selector attempt per subscription."
    );
    let worker = BleWorker::start(address, true, false);
    let assessment_deadline = Instant::now() + Duration::from_secs(10);
    let mut observation_deadline = None;
    let mut summary = RawSummary::default();
    let mut sampled_notifications = 0;

    loop {
        let diagnostics = worker.diagnostics();
        match diagnostics.activation {
            ActivationState::Unverified => {
                eprintln!(
                    "Activation unverified after bounded assessment; selector method errors: {}",
                    diagnostics.selector_write_failures
                );
                return ExitCode::FAILURE;
            }
            ActivationState::ObservedData | ActivationState::ObservedSelector => {
                observation_deadline
                    .get_or_insert_with(|| Instant::now() + Duration::from_secs(options.seconds));
            }
            ActivationState::Idle | ActivationState::Awaiting => {}
        }
        if diagnostics.motion_notifications > sampled_notifications {
            if let Some(sample) = worker.motion_sample() {
                summary.observe(sample.raw);
                sampled_notifications = diagnostics.motion_notifications;
            }
        }
        if observation_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        if observation_deadline.is_none() && Instant::now() >= assessment_deadline {
            eprintln!("Activation assessment did not complete within 10 seconds; selector method errors: {}", diagnostics.selector_write_failures);
            return ExitCode::FAILURE;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    let diagnostics = worker.diagnostics();
    if diagnostics.motion_notifications > sampled_notifications {
        if let Some(sample) = worker.motion_sample() {
            summary.observe(sample.raw);
        }
    }
    println!("Subscribed: {}", diagnostics.subscribed);
    println!("Activation assessment: {:?}", diagnostics.activation);
    println!(
        "Selector method error count: {}",
        diagnostics.selector_write_failures
    );
    println!(
        "Observed selector status: {:?}",
        diagnostics.selector_status
    );
    println!(
        "Motion notification count: {}",
        diagnostics.motion_notifications
    );
    if let (Some(first_min), Some(first_max), Some(second_min), Some(second_max)) = (
        summary.first_min,
        summary.first_max,
        summary.second_min,
        summary.second_max,
    ) {
        println!(
            "Raw sample ranges (not calibrated physical axes): first={first_min}..={first_max}, second={second_min}..={second_max}"
        );
        println!(
            "Distinct sampled changes: {}; sampled notification snapshots: {}",
            summary.distinct_changes, summary.samples
        );
    } else {
        println!("Raw sample ranges: none observed");
    }
    if summary.samples == 0 || !diagnostics.subscribed || worker.motion_sample().is_none() {
        eprintln!("Unresolved: no current subscribed motion sample observed during the window.");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    match parse_options() {
        Ok(None) => {
            println!("{}", usage());
            ExitCode::SUCCESS
        }
        Ok(Some(options)) => run(options),
        Err(error) => {
            eprintln!("{error}\n{}", usage());
            ExitCode::FAILURE
        }
    }
}
