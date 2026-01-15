use clock_steering::{Clock, LeapIndicator, TimeOffset, Timestamp};
use std::env;
use std::process;
use std::time::Duration;

#[cfg(unix)]
use clock_steering::unix::UnixClock;

#[cfg(unix)]
#[derive(Debug, Clone)]
enum ClockType {
    Realtime,
    #[cfg(target_os = "linux")]
    Tai,
    #[cfg(target_os = "linux")]
    Ptp(String),
}

#[cfg(unix)]
impl ClockType {
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "realtime" | "real" => Ok(ClockType::Realtime),
            #[cfg(target_os = "linux")]
            "tai" => Ok(ClockType::Tai),
            #[cfg(target_os = "linux")]
            path if path.starts_with("/dev/ptp") => Ok(ClockType::Ptp(path.to_string())),
            #[cfg(not(target_os = "linux"))]
            "tai" => Err(format!(
                "Clock type '{}' is not supported on this platform. Only 'realtime' is available.",
                s
            )),
            #[cfg(not(target_os = "linux"))]
            path if path.starts_with("/dev/ptp") => Err(format!(
                "Clock type '{}' is not supported on this platform. Only 'realtime' is available.",
                s
            )),
            _ => {
                #[cfg(target_os = "linux")]
                let options = "realtime, tai, /dev/ptp<N>";
                #[cfg(not(target_os = "linux"))]
                let options = "realtime";
                Err(format!(
                    "Unknown clock type '{}'. Valid options: {}",
                    s, options
                ))
            }
        }
    }

    fn create_clock(&self) -> Result<UnixClock, std::io::Error> {
        match self {
            ClockType::Realtime => Ok(UnixClock::CLOCK_REALTIME),
            #[cfg(target_os = "linux")]
            ClockType::Tai => Ok(UnixClock::CLOCK_TAI),
            #[cfg(target_os = "linux")]
            ClockType::Ptp(path) => UnixClock::open(path),
        }
    }

    fn description(&self) -> String {
        match self {
            ClockType::Realtime => "System Realtime Clock (CLOCK_REALTIME)".to_string(),
            #[cfg(target_os = "linux")]
            ClockType::Tai => "System TAI Clock (CLOCK_TAI)".to_string(),
            #[cfg(target_os = "linux")]
            ClockType::Ptp(path) => format!("PTP Hardware Clock ({})", path),
        }
    }
}

fn print_timestamp(label: &str, timestamp: Timestamp) {
    println!(
        "{}: {}.{:09} seconds since Unix epoch",
        label, timestamp.seconds, timestamp.nanos
    );
}

fn print_frequency(label: &str, freq: f64) {
    println!("{}: {:.9} ms/s", label, freq);
}

fn demonstrate_time_reading(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Time Reading Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Get current time
        let now = clock.now()?;
        print_timestamp("Current time", now);

        // Get clock resolution
        let resolution = clock.resolution()?;
        print_timestamp("Clock resolution", resolution);
    }

    #[cfg(not(unix))]
    {
        println!("Time reading is only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_capabilities(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Clock Capabilities Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        match clock.capabilities() {
            Ok(caps) => {
                println!("Clock capabilities detected:");

                println!("  Frequency adjustment:");
                let max_freq = caps.max_frequency_ppm();
                println!("    Maximum range: {} ppb", max_freq);

                println!("  Offset adjustment:");
                let max_offset = caps.max_offset_ns();
                println!("    Maximum range: {} nanoseconds", max_offset);
            }
            Err(e) => println!("Failed to get clock capabilities: {}", e),
        }
    }

    #[cfg(not(unix))]
    {
        println!("Capabilities detection is only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_frequency_operations(
    clock_type: &ClockType,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Frequency Operations Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Get current frequency
        match clock.get_frequency() {
            Ok(freq) => print_frequency("Current frequency", freq),
            Err(e) => println!("Failed to get frequency: {} (may require root)", e),
        }

        // Try to set a small frequency adjustment (requires root)
        let small_adjustment = 0.001; // 0.001 ms/s
        match clock.set_frequency(small_adjustment) {
            Ok(timestamp) => {
                print_frequency("Set frequency to", small_adjustment);
                print_timestamp("Adjustment applied at", timestamp);

                // Reset frequency back to 0
                match clock.set_frequency(0.0) {
                    Ok(reset_time) => {
                        println!("Frequency reset to 0.0 ms/s");
                        print_timestamp("Reset applied at", reset_time);
                    }
                    Err(e) => println!("Failed to reset frequency: {}", e),
                }
            }
            Err(e) => {
                println!("Failed to set frequency: {} (requires root privileges)", e)
            }
        }
    }

    #[cfg(not(unix))]
    {
        println!("Frequency operations are only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_clock_stepping(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Clock Stepping Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Try to step the clock by a very small amount (1 microsecond)
        let small_offset = TimeOffset {
            seconds: 0,
            nanos: 1_000, // 1 microsecond
        };

        match clock.step_clock(small_offset) {
            Ok(timestamp) => {
                println!("Clock stepped by 1 microsecond");
                print_timestamp("Step applied at", timestamp);

                // Step back to undo the change
                let reverse_offset = TimeOffset {
                    seconds: 0,
                    nanos: 1_000, // This will be subtracted
                };

                match clock.step_clock(TimeOffset {
                    seconds: -reverse_offset.seconds,
                    nanos: 0,
                }) {
                    Ok(undo_time) => {
                        println!("Clock step reversed");
                        print_timestamp("Reversal applied at", undo_time);
                    }
                    Err(e) => println!("Failed to reverse clock step: {}", e),
                }
            }
            Err(e) => println!("Failed to step clock: {} (requires root privileges)", e),
        }
    }

    #[cfg(not(unix))]
    {
        println!("Clock stepping is only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_leap_seconds(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Leap Seconds Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Try to set leap second indicator
        match clock.set_leap_seconds(LeapIndicator::NoWarning) {
            Ok(()) => println!("Leap second indicator set to NoWarning"),
            Err(e) => println!("Failed to set leap seconds: {} (may require root)", e),
        }

        // Demonstrate other leap second indicators
        for &indicator in &[
            LeapIndicator::Leap61,
            LeapIndicator::Leap59,
            LeapIndicator::Unknown,
            LeapIndicator::NoWarning, // Reset to normal
        ] {
            match clock.set_leap_seconds(indicator) {
                Ok(()) => println!("Leap second indicator set to {:?}", indicator),
                Err(e) => println!("Failed to set leap seconds to {:?}: {}", indicator, e),
            }
        }
    }

    #[cfg(not(unix))]
    {
        println!("Leap seconds operations are only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_tai_operations(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TAI Operations Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Try to get current TAI offset
        match clock.get_tai() {
            Ok(tai_offset) => println!("Current TAI offset: {} seconds", tai_offset),
            Err(e) => println!("Failed to get TAI offset: {}", e),
        }

        // Try to set TAI offset (requires root)
        let new_tai_offset = 37; // Current TAI-UTC offset as of 2023
        match clock.set_tai(new_tai_offset) {
            Ok(()) => println!("TAI offset set to {} seconds", new_tai_offset),
            Err(e) => println!("Failed to set TAI offset: {} (requires root privileges)", e),
        }
    }

    #[cfg(not(unix))]
    {
        println!("TAI operations are only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_error_estimation(clock_type: &ClockType) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Error Estimation Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Provide error estimates to the kernel
        let estimated_error = Duration::from_micros(100); // 100 microseconds
        let maximum_error = Duration::from_millis(1); // 1 millisecond

        match clock.error_estimate_update(estimated_error, maximum_error) {
            Ok(()) => println!(
                "Error estimates updated: estimated={}μs, maximum={}ms",
                estimated_error.as_micros(),
                maximum_error.as_millis()
            ),
            Err(e) => println!("Failed to update error estimates: {} (may require root)", e),
        }
    }

    #[cfg(not(unix))]
    {
        println!("Error estimation is only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn demonstrate_kernel_ntp_disable(
    clock_type: &ClockType,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Kernel NTP Algorithm Demo ===");

    #[cfg(unix)]
    {
        println!("Using: {}", clock_type.description());
        let clock = clock_type.create_clock()?;

        // Try to disable kernel NTP algorithm
        match clock.disable_kernel_ntp_algorithm() {
            Ok(()) => println!("Kernel NTP algorithm disabled successfully"),
            Err(e) => println!(
                "Failed to disable kernel NTP algorithm: {} (requires root privileges)",
                e
            ),
        }
    }

    #[cfg(not(unix))]
    {
        println!("Kernel NTP operations are only supported on Unix systems in this example.");
    }

    println!();
    Ok(())
}

fn print_help() {
    println!("Clock Steering Example Binary");
    println!("============================");
    println!();
    println!("This example demonstrates various clock steering operations.");
    println!("Many operations require root privileges to modify the system clock.");
    println!();
    println!("Usage:");
    println!("  cargo run --example basic -- [OPTIONS]");
    println!();
    println!("Clock Selection:");
    println!("  --clock TYPE    Specify which clock to use (default: realtime)");
    println!("                  Options:");
    println!("                    realtime  - System realtime clock (CLOCK_REALTIME)");
    #[cfg(target_os = "linux")]
    {
        println!("                    tai       - System TAI clock (CLOCK_TAI)");
        println!("                    /dev/ptpN - PTP hardware clock device path");
    }
    #[cfg(not(target_os = "linux"))]
    {
        println!("                  Note: TAI clock and PTP devices only available on Linux");
    }
    println!();
    println!("Operation Options:");
    println!("  --help, -h      Show this help message");
    println!("  --time          Demonstrate time reading operations");
    println!("  --capabilities  Demonstrate clock capabilities detection");
    println!("  --frequency     Demonstrate frequency adjustment operations");
    println!("  --step          Demonstrate clock stepping operations");
    println!("  --leap          Demonstrate leap second operations");
    println!("  --tai           Demonstrate TAI offset operations");
    println!("  --errors        Demonstrate error estimation operations");
    println!("  --kernel-ntp    Demonstrate kernel NTP algorithm operations");
    println!("  --all           Run all demonstrations (default)");
    println!();
    println!("Examples:");
    println!("  cargo run --example basic -- --time");
    #[cfg(target_os = "linux")]
    {
        println!("  cargo run --example basic -- --clock tai --capabilities");
        println!("  sudo cargo run --example basic -- --clock /dev/ptp0 --frequency");
    }
    #[cfg(not(target_os = "linux"))]
    {
        println!("  cargo run --example basic -- --capabilities");
    }
    println!();
    println!("Warning: This example will attempt to modify your system clock!");
    println!("Run with caution and preferably in a test environment.");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    #[cfg(unix)]
    let mut clock_type = ClockType::Realtime;
    let mut operation = None;

    // Parse command line arguments
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--clock" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --clock requires a clock type argument");
                    eprintln!("Use --help for usage information.");
                    process::exit(1);
                }
                #[cfg(unix)]
                {
                    match ClockType::from_str(&args[i + 1]) {
                        Ok(ct) => clock_type = ct,
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            process::exit(1);
                        }
                    }
                }
                i += 2; // Skip the clock type argument
                continue;
            }
            "--time" => operation = Some("time"),
            "--capabilities" => operation = Some("capabilities"),
            "--frequency" => operation = Some("frequency"),
            "--step" => operation = Some("step"),
            "--leap" => operation = Some("leap"),
            "--tai" => operation = Some("tai"),
            "--errors" => operation = Some("errors"),
            "--kernel-ntp" => operation = Some("kernel-ntp"),
            "--all" => operation = Some("all"),
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                eprintln!("Use --help for usage information.");
                process::exit(1);
            }
        }
        i += 1;
    }

    #[cfg(unix)]
    {
        // Execute the requested operation
        match operation {
            Some("time") => {
                if let Err(e) = demonstrate_time_reading(&clock_type) {
                    eprintln!("Time reading demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("capabilities") => {
                if let Err(e) = demonstrate_capabilities(&clock_type) {
                    eprintln!("Capabilities demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("frequency") => {
                if let Err(e) = demonstrate_frequency_operations(&clock_type) {
                    eprintln!("Frequency demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("step") => {
                if let Err(e) = demonstrate_clock_stepping(&clock_type) {
                    eprintln!("Clock stepping demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("leap") => {
                if let Err(e) = demonstrate_leap_seconds(&clock_type) {
                    eprintln!("Leap seconds demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("tai") => {
                if let Err(e) = demonstrate_tai_operations(&clock_type) {
                    eprintln!("TAI operations demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("errors") => {
                if let Err(e) = demonstrate_error_estimation(&clock_type) {
                    eprintln!("Error estimation demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("kernel-ntp") => {
                if let Err(e) = demonstrate_kernel_ntp_disable(&clock_type) {
                    eprintln!("Kernel NTP demo failed: {}", e);
                    process::exit(1);
                }
                return;
            }
            Some("all") | None => {
                // Run all demonstrations
            }
            _ => unreachable!(),
        }

        println!("Clock Steering Library Example");
        println!("==============================");
        println!();
        println!("Using: {}", clock_type.description());
        println!("This example demonstrates the clock-steering library functionality.");
        println!(
            "Note: Many operations require root privileges and will show permission errors otherwise."
        );
        println!();

        // Run all demonstrations
        let demos = [
            (
                "Time Reading",
                demonstrate_time_reading
                    as fn(&ClockType) -> Result<(), Box<dyn std::error::Error>>,
            ),
            ("Clock Capabilities", demonstrate_capabilities),
            ("Frequency Operations", demonstrate_frequency_operations),
            ("Clock Stepping", demonstrate_clock_stepping),
            ("Leap Seconds", demonstrate_leap_seconds),
            ("TAI Operations", demonstrate_tai_operations),
            ("Error Estimation", demonstrate_error_estimation),
            ("Kernel NTP", demonstrate_kernel_ntp_disable),
        ];

        for (name, demo_fn) in &demos {
            if let Err(e) = demo_fn(&clock_type) {
                eprintln!("{} demo failed: {}", name, e);
            }
        }

        println!("Example completed!");
        println!();
        println!("To run individual demonstrations, use:");
        println!("  cargo run --example basic_usage -- --help");
    }

    #[cfg(not(unix))]
    {
        println!("This example only works on Unix-like systems (Linux, macOS, etc.)");
        println!("The clock-steering library currently only supports Unix platforms.");
    }
}
