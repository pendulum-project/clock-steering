use crate::{Clock, HoldFrequency, LeapIndicator, Timestamp};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct UnixClock {
    clock: libc::clockid_t,
}

impl UnixClock {
    pub const CLOCK_REALTIME: Self = UnixClock {
        clock: libc::CLOCK_REALTIME,
    };

    fn clock_adjtime(&self, timex: &mut libc::timex) -> Result<(), Error> {
        // We don't care about the time status, so the non-error
        // information in the return value of clock_adjtime can be ignored.
        //
        // # Safety
        //
        // The clock_adjtime call is safe because the reference always
        // points to a valid libc::timex.
        //
        // using an invalid clock id is safe. `clock_adjtime` will return an EINVAL
        // error https://man.archlinux.org/man/clock_adjtime.2.en#EINVAL~4
        #[cfg(target_os = "linux")]
        use libc::clock_adjtime as adjtime;

        #[cfg(any(target_os = "freebsd", target_os = "macos"))]
        unsafe fn adjtime(clk_id: libc::clockid_t, buf: *mut libc::timex) -> libc::c_int {
            assert_eq!(
                clk_id,
                libc::CLOCK_REALTIME,
                "only the REALTIME clock is supported"
            );

            libc::ntp_adjtime(buf)
        }

        if unsafe { adjtime(self.clock, timex) } == -1 {
            Err(convert_errno())
        } else {
            Ok(())
        }
    }

    fn ntp_adjtime(timex: &mut libc::timex) -> Result<(), Error> {
        #[cfg(any(target_os = "freebsd", target_os = "macos", target_env = "gnu"))]
        use libc::ntp_adjtime as adjtime;

        // ntp_adjtime is equivalent to adjtimex for our purposes
        //
        // https://man7.org/linux/man-pages/man2/adjtimex.2.html
        #[cfg(all(target_os = "linux", target_env = "musl"))]
        use libc::adjtimex as adjtime;

        // We don't care about the time status, so the non-error
        // information in the return value of ntp_adjtime can be ignored.
        // The ntp_adjtime call is safe because the reference always
        // points to a valid libc::timex.
        if unsafe { adjtime(timex) } == -1 {
            Err(convert_errno())
        } else {
            Ok(())
        }
    }

    /// Adjust the clock state with a [`libc::timex`] specifying the desired changes.
    ///
    /// Note that [`libc::timex`] has a different layout between different operating systems, and
    /// not all fields are available on all operating systems. Keep this in mind when writing
    /// platform-independent code.
    pub fn adjtime(&self, timex: &mut libc::timex) -> Result<(), Error> {
        if self.clock == libc::CLOCK_REALTIME {
            Self::ntp_adjtime(timex)
        } else {
            self.clock_adjtime(timex)
        }
    }

    #[cfg_attr(target_os = "linux", allow(unused))]
    fn clock_gettime(&self) -> Result<libc::timespec, Error> {
        let mut timespec = EMPTY_TIMESPEC;

        // # Safety
        //
        // using an invalid clock id is safe. `clock_adjtime` will return an EINVAL
        // error https://linux.die.net/man/3/clock_gettime
        //
        // The timespec pointer is valid.
        cerr(unsafe { libc::clock_gettime(self.clock, &mut timespec) })?;

        Ok(timespec)
    }

    #[cfg_attr(target_os = "linux", allow(unused))]
    fn clock_settime(&self, mut timespec: libc::timespec) -> Result<(), Error> {
        while timespec.tv_nsec > 1_000_000_000 {
            timespec.tv_sec += 1;
            timespec.tv_nsec -= 1_000_000_000;
        }

        // # Safety
        //
        // using an invalid clock id is safe. `clock_adjtime` will return an EINVAL
        // error https://linux.die.net/man/3/clock_settime
        //
        // The timespec pointer is valid.
        unsafe { cerr(libc::clock_settime(self.clock, &timespec))? };

        Ok(())
    }

    #[cfg_attr(target_os = "linux", allow(unused))]
    fn step_clock_timespec(&self, offset: Duration) -> Result<Timestamp, Error> {
        let offset_secs = offset.as_secs();
        let offset_nanos = offset.subsec_nanos();

        let mut timespec = self.clock_gettime()?;

        // see https://github.com/rust-lang/libc/issues/1848
        #[cfg_attr(target_env = "musl", allow(deprecated))]
        {
            timespec.tv_sec += offset_secs as libc::time_t;
            timespec.tv_nsec += offset_nanos as libc::c_long;
        }

        self.clock_settime(timespec)?;

        Ok(current_time_timespec(timespec, Precision::Nano))
    }

    #[cfg(target_os = "linux")]
    fn step_clock_timex(&self, offset: Duration) -> Result<Timestamp, Error> {
        let secs = offset.as_secs();
        let nanos = offset.subsec_nanos();

        let mut timex = libc::timex {
            modes: libc::ADJ_SETOFFSET | libc::MOD_NANO,
            time: libc::timeval {
                tv_sec: secs as _,
                tv_usec: nanos as libc::suseconds_t,
            },
            ..EMPTY_TIMEX
        };

        self.adjtime(&mut timex)?;
        self.extract_current_time(&timex)
    }

    fn extract_current_time(&self, _timex: &libc::timex) -> Result<Timestamp, Error> {
        #[cfg(target_os = "linux")]
        // hardware clocks may not report the timestamp
        if _timex.time.tv_sec != 0 && _timex.time.tv_usec != 0 {
            // in a timex, the status flag determines precision
            let precision = match _timex.status & libc::STA_NANO {
                0 => Precision::Micro,
                _ => Precision::Nano,
            };

            return Ok(current_time_timeval(_timex.time, precision));
        }

        // clock_gettime always gives nanoseconds
        let timespec = self.clock_gettime()?;
        Ok(current_time_timespec(timespec, Precision::Nano))
    }

    #[inline(always)]
    fn update_timex<F>(&self, f: F) -> Result<(), Error>
    where
        F: FnOnce(libc::timex) -> libc::timex,
    {
        let mut timex = EMPTY_TIMEX;
        self.adjtime(&mut timex)?;

        timex = f(timex);

        self.adjtime(&mut timex)
    }

    #[inline(always)]
    fn update_status<F>(&self, f: F) -> Result<(), Error>
    where
        F: FnOnce(libc::c_int) -> libc::c_int,
    {
        self.update_timex(|mut timex| {
            // We are setting the status bits
            timex.modes = libc::MOD_STATUS;

            // update the status flags
            timex.status = f(timex.status);

            timex
        })
    }

    /// Modify the frequency by a multiplier. To change the frequency to a fixed value, use
    /// [`UnixClock::set_frequency`].
    ///
    /// For example, if the clock is at 10.0 mhz, but should run at 10.1 mhz,
    /// then the frequency_multiplier should be 1.01.
    pub fn adjust_frequency(
        &mut self,
        frequency_multiplier: f64,
        hold: HoldFrequency,
    ) -> Result<Timestamp, Error> {
        let mut timex = EMPTY_TIMEX;
        self.adjtime(&mut timex)?;

        const M: f64 = 1_000_000.0;

        // In struct timex, freq, ppsfreq, and stabil are ppm (parts per million) with a
        // 16-bit fractional part, which means that a value of 1 in one of those fields
        // actually means 2^-16 ppm, and 2^16=65536 is 1 ppm.  This is the case for both
        // input values (in the case of freq) and output values.
        let current_ppm = (timex.freq >> 16) as f64 + ((timex.freq & 0xffff) as f64 / 65536.0);

        // we need to recover the current frequency multiplier from the PPM value.
        // The ppm is an offset from the main frequency, so it's the base +- the ppm
        // expressed as a percentage. Ppm is in the opposite direction from the
        // speed factor. A postive ppm means the clock is running slower, so we use its
        // negative.
        let current_frequency_multiplier = 1.0 - (current_ppm / M);

        // Now multiply the frequencies
        let new_frequency_multiplier = current_frequency_multiplier * frequency_multiplier;

        // Get back the new ppm value by subtracting the 1.0 base from it, changing the
        // percentage to the ppm again and then negating it.
        let new_ppm = -((new_frequency_multiplier - 1.0) * M);

        self.set_frequency(new_ppm, hold)
    }

    /// Enable the kernel phase-locked loop (PLL). This is a feature used by the standard NTP
    /// algorithm. Other clock discipline algorithms (custom NTP, PTP) should not enable this
    /// setting.
    pub fn enable_kernel_ntp_algorithm(&self) -> Result<(), Error> {
        let mut timex = EMPTY_TIMEX;
        self.adjtime(&mut timex)?;

        // We are setting the status bits
        timex.modes = libc::MOD_STATUS;

        // Enable the kernel phase locked loop
        timex.status |= libc::STA_PLL;

        // Disable the frequency locked loop; disable pps input based time and frequency control
        timex.status &= !(libc::STA_FLL | libc::STA_PPSTIME | libc::STA_PPSFREQ);

        self.adjtime(&mut timex)
    }

    /// Disable all kernel clock discipline. It is all your responsibility now.
    ///
    /// The disabled settings are:
    ///
    /// - [`libc::STA_PLL`]: kernel phase-locked loop
    /// - [`libc::STA_FLL`]: kernel frequency-locked loop
    /// - [`libc::STA_PPSTIME`]: pulse-per-second time
    /// - [`libc::STA_PPSFREQ`]: pulse-per-second frequency discipline
    pub fn disable_kernel_ntp_algorithm(&self) -> Result<(), Error> {
        let mut timex = EMPTY_TIMEX;
        self.adjtime(&mut timex)?;

        // We are setting the status bits
        timex.modes = libc::MOD_STATUS;

        // Disable all kernel time control loops (phase lock, frequency lock, pps time and pps frequency).
        timex.status &= !(libc::STA_PLL | libc::STA_FLL | libc::STA_PPSTIME | libc::STA_PPSFREQ);

        // ignore if we cannot disable the kernel time control loops (e.g. external clocks)
        Error::ignore_not_supported(self.adjtime(&mut timex))
    }
}

impl Clock for UnixClock {
    type Error = Error;

    fn now(&self) -> Result<Timestamp, Self::Error> {
        let mut ntp_kapi_timex = EMPTY_TIMEX;

        self.adjtime(&mut ntp_kapi_timex)?;

        self.extract_current_time(&ntp_kapi_timex)
    }

    fn resolution(&self) -> Result<Timestamp, Self::Error> {
        let mut timespec = EMPTY_TIMESPEC;

        cerr(unsafe { libc::clock_gettime(self.clock, &mut timespec) })?;

        Ok(current_time_timespec(timespec, Precision::Nano))
    }

    fn set_frequency(&self, frequency: f64, hold: HoldFrequency) -> Result<Timestamp, Self::Error> {
        let mut timex = EMPTY_TIMEX;

        // set the frequency and status (because ADJ_FREQHOLD)
        timex.modes = match hold {
            HoldFrequency::Enable => libc::MOD_FREQUENCY | libc::MOD_STATUS,
            HoldFrequency::Disable => libc::MOD_FREQUENCY,
        };

        // NTP Kapi expects frequency adjustment in units of 2^-16 ppm
        // but our input is in units of seconds drift per second, so convert.
        timex.freq = (frequency * 65536e6) as libc::c_long;

        // Hold frequency. Normally adjustments made via ADJ_OFFSET result in dampened
        // frequency adjustments also being made. So a single call corrects the
        // current offset, but as offsets in the same direction
        // are made repeatedly, the small frequency adjustments will accumulate to fix
        // the long-term skew.
        //
        // This flag prevents the small frequency adjustment from being made when
        // correcting for an ADJ_OFFSET value.
        //
        // NOTE: the status field is ignored if hold == HoldFrequency::Disable
        timex.status |= libc::STA_FREQHOLD;

        self.adjtime(&mut timex)?;
        self.extract_current_time(&timex)
    }

    #[cfg(target_os = "linux")]
    fn step_clock(&self, offset: Duration) -> Result<Timestamp, Self::Error> {
        self.step_clock_timex(offset)
    }

    #[cfg(any(target_os = "freebsd", target_os = "macos"))]
    fn step_clock(&self, offset: Duration) -> Result<Timestamp, Self::Error> {
        self.step_clock_timespec(offset)
    }

    fn set_leap_seconds(&self, leap_status: LeapIndicator) -> Result<(), Self::Error> {
        self.update_status(|status| status | leap_status.as_status_bit())
    }

    fn error_estimate_update(
        &self,
        est_error: Duration,
        max_error: Duration,
    ) -> Result<(), Self::Error> {
        let mut timex = EMPTY_TIMEX;
        timex.modes = libc::MOD_ESTERROR | libc::MOD_MAXERROR;
        timex.esterror = est_error.as_nanos() as libc::c_long / 1000;
        timex.maxerror = max_error.as_nanos() as libc::c_long / 1000;
        Error::ignore_not_supported(self.adjtime(&mut timex))
    }
}

#[derive(Debug, Copy, Clone, thiserror::Error, PartialEq, Eq, Hash)]
pub enum Error {
    /// Insufficient permissions to interact with the clock.
    #[error("Insufficient permissions to interact with the clock.")]
    NoPermission,
    /// Invalid operation requested
    #[error("Invalid operation requested")]
    Invalid,
    /// Clock device has gone away
    #[error("Clock device has gone away")]
    NoDevice,
    /// Clock operation requested is not supported by operating system.
    #[error("Clock operation requested is not supported by operating system.")]
    NotSupported,
    /// Invalid clock path
    #[error("Invalid clock path")]
    InvalidClockPath,
}

impl Error {
    /// Turn the `Error::NotSupported` error variant into `Ok(())`, to silently
    /// ignore operations that are not supported by the current clock. All
    /// other input values are untouched.
    pub fn ignore_not_supported(res: Result<(), Error>) -> Result<(), Error> {
        match res {
            Err(Error::NotSupported) => Ok(()),
            other => other,
        }
    }
}

fn error_number() -> libc::c_int {
    #[cfg(target_os = "linux")]
    unsafe {
        *libc::__errno_location()
    }

    #[cfg(not(target_os = "linux"))]
    unsafe {
        *libc::__error()
    }
}

// Convert those error numbers that can occur for calls to the following
// functions
// - ntp_adjtimex https://man7.org/linux/man-pages/man3/ntp_adjtime.3.html
// - clock_gettime & clock_settime https://man7.org/linux/man-pages/man3/clock_gettime.3.html
fn convert_errno() -> Error {
    match error_number() {
        libc::EINVAL => Error::Invalid,
        // The documentation is a bit unclear if this can happen with
        // non-dynamic clocks like the ntp kapi clock, however lets
        // deal with it just in case.
        libc::ENODEV => Error::NoDevice,
        libc::EOPNOTSUPP => Error::NotSupported,
        libc::EPERM | libc::EACCES => Error::NoPermission,
        libc::EFAULT => unreachable!("we always pass in valid (accessible) buffers"),
        // No other errors should occur
        other => {
            let error = std::io::Error::from_raw_os_error(other);
            unreachable!("error code `{other}` ({error:?}) should not occur")
        }
    }
}

fn cerr(c_int: libc::c_int) -> Result<(), Error> {
    if c_int == -1 {
        Err(convert_errno())
    } else {
        Ok(())
    }
}

pub(crate) enum Precision {
    Nano,
    #[cfg_attr(any(target_os = "freebsd", target_os = "macos"), allow(unused))]
    Micro,
}

#[cfg_attr(target_os = "linux", allow(unused))]
fn current_time_timespec(timespec: libc::timespec, precision: Precision) -> Timestamp {
    let mut seconds = timespec.tv_sec;

    let mut nanos = match precision {
        Precision::Nano => timespec.tv_nsec as i32,
        Precision::Micro => (timespec.tv_nsec as i32)
            .checked_mul(1000)
            .unwrap_or_default(),
    };

    // on macOS (at least) we've observed higher nanosecond counts that appear valid
    while nanos > 1_000_000_000 {
        seconds = seconds.wrapping_add(1);
        nanos -= 1_000_000_000;
    }

    // we disallow negative nanoseconds
    while nanos < 0 {
        seconds = seconds.wrapping_sub(1);
        nanos += 1_000_000_000;
    }

    Timestamp {
        seconds,
        nanos: nanos as u32,
        subnanos: 0,
    }
}

#[cfg_attr(not(target_os = "linux"), allow(unused))]
fn current_time_timeval(timespec: libc::timeval, precision: Precision) -> Timestamp {
    let seconds = timespec.tv_sec;
    let nanos = match precision {
        Precision::Nano => timespec.tv_usec as u32,
        Precision::Micro => (timespec.tv_usec as u32)
            .checked_mul(1000)
            .unwrap_or_default(),
    };

    Timestamp {
        seconds,
        nanos,
        subnanos: 0,
    }
}

const EMPTY_TIMESPEC: libc::timespec = libc::timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

// Libc has no good other way of obtaining this, so let's at least make our
// functions more readable.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub const EMPTY_TIMEX: libc::timex = libc::timex {
    modes: 0,
    offset: 0,
    freq: 0,
    maxerror: 0,
    esterror: 0,
    status: 0,
    constant: 0,
    precision: 0,
    tolerance: 0,
    time: libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    },
    tick: 0,
    ppsfreq: 0,
    jitter: 0,
    shift: 0,
    stabil: 0,
    jitcnt: 0,
    calcnt: 0,
    errcnt: 0,
    stbcnt: 0,
    tai: 0,
    __unused1: 0,
    __unused2: 0,
    __unused3: 0,
    __unused4: 0,
    __unused5: 0,
    __unused6: 0,
    __unused7: 0,
    __unused8: 0,
    __unused9: 0,
    __unused10: 0,
    __unused11: 0,
};

#[cfg(all(target_os = "linux", target_env = "musl"))]
pub const EMPTY_TIMEX: libc::timex = libc::timex {
    modes: 0,
    offset: 0,
    freq: 0,
    maxerror: 0,
    esterror: 0,
    status: 0,
    constant: 0,
    precision: 0,
    tolerance: 0,
    time: libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    },
    tick: 0,
    ppsfreq: 0,
    jitter: 0,
    shift: 0,
    stabil: 0,
    jitcnt: 0,
    calcnt: 0,
    errcnt: 0,
    stbcnt: 0,
    tai: 0,
    __padding: [0; 11],
};

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub const EMPTY_TIMEX: libc::timex = libc::timex {
    modes: 0,
    offset: 0,
    freq: 0,
    maxerror: 0,
    esterror: 0,
    status: 0,
    constant: 0,
    precision: 0,
    tolerance: 0,
    ppsfreq: 0,
    jitter: 0,
    shift: 0,
    stabil: 0,
    jitcnt: 0,
    calcnt: 0,
    errcnt: 0,
    stbcnt: 0,
};

impl LeapIndicator {
    fn as_status_bit(self) -> libc::c_int {
        match self {
            LeapIndicator::NoWarning => 0,
            LeapIndicator::Leap61 => libc::STA_INS,
            LeapIndicator::Leap59 => libc::STA_DEL,
            LeapIndicator::Unknown => libc::STA_UNSYNC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_now_does_not_crash() {
        let clock = UnixClock::CLOCK_REALTIME;
        assert_ne!(clock.now().unwrap(), Timestamp::default(),);
    }

    #[test]
    fn realtime_gettime() {
        let clock = UnixClock::CLOCK_REALTIME;
        let time = clock.clock_gettime().unwrap();

        assert_ne!((time.tv_sec, time.tv_nsec), (0, 0))
    }

    #[test]
    #[ignore = "requires permissions, useful for testing permissions"]
    fn ptp0_gettime() {
        let clock = UnixClock::CLOCK_REALTIME;
        let time = clock.clock_gettime().unwrap();

        assert_ne!((time.tv_sec, time.tv_nsec), (0, 0))
    }

    #[test]
    #[ignore = "requires permissions, useful for testing permissions"]
    fn step_clock() {
        UnixClock::CLOCK_REALTIME
            .step_clock(Duration::new(0, 0))
            .unwrap();
    }
}
