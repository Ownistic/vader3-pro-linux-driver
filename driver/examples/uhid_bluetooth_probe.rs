//! Neutral-only UHID diagnostic. It opens no physical controller or BlueZ
//! connection. Kernel binding observations do not establish Steam UI behavior.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use vader3p_steam::edge::EdgePad;
use vader3p_steam::identity::MacAddress;
use vader3p_steam::input::State;

const TICK: Duration = Duration::from_millis(4);
const DURATION: Duration = Duration::from_secs(12);

fn service(pad: &mut EdgePad) -> Result<()> {
    // Discard host feedback: this diagnostic has no physical feedback sink.
    let _ = pad.poll_host()?;
    if pad.started() {
        pad.send(
            &State::default(),
            false,
            None,
            [1, 1],
            true,
            Duration::from_millis(100),
        )?;
    }
    Ok(())
}

fn observation(event: &str, a: Option<&EdgePad>, b: &EdgePad, virtual_a: MacAddress) -> Result<()> {
    println!(
        "{{\"event\":\"{}\",\"virtual_a\":\"{}\",\"virtual_b\":\"{}\",\"a_started\":{},\"b_started\":{}}}",
        event,
        virtual_a,
        b.virtual_address(),
        a.is_some_and(EdgePad::started),
        b.started()
    );
    io::stdout().flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let physical_a: MacAddress = "12:34:56:78:9A:BC".parse()?;
    let physical_b: MacAddress = "01:23:45:67:89:AB".parse()?;
    let mut a = Some(EdgePad::create("Flydigi neutral probe A", physical_a)?);
    let mut b = EdgePad::create("Flydigi neutral probe B", physical_b)?;
    let virtual_a = physical_a.virtual_address();
    let start = Instant::now();
    let mut removed = false;
    let mut recreated = false;
    let mut observed = [false; 3];
    let mut removed_at = None;
    let mut recreated_at = None;

    loop {
        let elapsed = start.elapsed();
        if elapsed >= DURATION {
            break;
        }
        if !removed && elapsed >= Duration::from_secs(4) {
            drop(a.take());
            removed = true;
            removed_at = Some(Instant::now());
        }
        if !recreated && elapsed >= Duration::from_secs(8) {
            a = Some(EdgePad::create("Flydigi neutral probe A", physical_a)?);
            recreated = true;
            recreated_at = Some(Instant::now());
        }
        if let Some(pad) = &mut a {
            service(pad)?;
        }
        service(&mut b)?;
        let ready = [
            !removed && elapsed >= Duration::from_secs(1),
            removed
                && !recreated
                && removed_at.is_some_and(|t: Instant| t.elapsed() >= Duration::from_secs(1)),
            recreated_at.is_some_and(|t: Instant| t.elapsed() >= Duration::from_secs(1)),
        ];
        for (index, label) in ["both", "b_only", "recreated"].iter().enumerate() {
            if !observed[index] && ready[index] {
                observation(label, a.as_ref(), &b, virtual_a)?;
                observed[index] = true;
            }
        }
        std::thread::sleep(TICK.min(DURATION.saturating_sub(start.elapsed())));
    }
    // Each EdgePad drops and closes its own UHID device here.
    Ok(())
}
