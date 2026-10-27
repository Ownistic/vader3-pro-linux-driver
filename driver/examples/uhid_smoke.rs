fn main() {
    match vader3p_steam::uhid::UhidDevice::create("Vader3P smoke test") {
        Ok(mut d) => {
            println!("created: {}", d.name());
            std::thread::sleep(std::time::Duration::from_millis(400));
            match d.poll_event() {
                Ok(Some(e)) => println!("first event: {e:?}"),
                Ok(None) => println!("no event yet"),
                Err(e) => println!("poll error: {e:#}"),
            }
            let report = vader3p_steam::encode::encode(&Default::default());
            match d.send_input(&report) {
                Ok(()) => println!("input report sent ({} bytes)", report.len()),
                Err(e) => println!("send error: {e:#}"),
            }
        }
        Err(e) => println!("create failed: {e:#}"),
    }
}
