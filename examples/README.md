# Clock Steering Examples

This directory contains examples demonstrating how to use the `clock-steering` library.

## Running Examples

To run an example:

```bash
cargo run --example basic

# Selecting  specific clocks
cargo run --example basic -- --clock realtime --capabilities
cargo run --example basic -- --clock tai --time
sudo cargo run --example basic -- --clock /dev/ptp0 --frequency
```

You can also run specific demonstrations:

## Root Privileges

⚠️ **Important**: Many clock operations require root privileges to modify the system clock. When running without root privileges, you'll see permission errors for operations that attempt to modify the clock.

To run with root privileges:

```bash
cargo build --example basic
sudo ./target/debug/examples/basic
```

## Available Options

The `basic` example supports these command-line options:

### Clock Selection Options:
- `--clock TYPE`: Specify which clock to use (default: realtime)
  - `realtime` - System realtime clock (CLOCK_REALTIME)
  - `tai` - System TAI clock (CLOCK_TAI)  
  - `/dev/ptpN` - PTP hardware clock device path

### Operation Options:
- `--help, -h`: Show help message
- `--time`: Demonstrate time reading operations
- `--capabilities`: Demonstrate clock adjustment limits detection
- `--frequency`: Demonstrate frequency adjustment operations  
- `--step`: Demonstrate clock stepping operations
- `--leap`: Demonstrate leap second operations
- `--tai`: Demonstrate TAI offset operations
- `--errors`: Demonstrate error estimation operations
- `--kernel-ntp`: Demonstrate kernel NTP algorithm operations
- `--all`: Run all demonstrations (default)

## Safety Warning

These examples will attempt to modify your system clock! Run with caution and preferably in a test environment or virtual machine. The examples try to make minimal changes and revert them when possible, but system clock modifications can affect system behavior.
