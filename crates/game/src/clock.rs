//! Today's date on this computer, for the daily challenge.

/// The local date (year, month, day) `days_ago` days before today.
pub fn local_date(days_ago: i64) -> (i32, u32, u32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let days = (now + offset_seconds()).div_euclid(86_400) - days_ago;
    civil_from_days(days)
}

/// The days since 1970-01-01 as a date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// How far local time is ahead of UTC, in seconds.
fn offset_seconds() -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: localtime_r only writes into the struct we give it.
        unsafe {
            let now = libc_time();
            let mut tm: Tm = std::mem::zeroed();
            if localtime_r(&now, &mut tm).is_null() {
                return 0;
            }
            tm.tm_gmtoff
        }
    }
    #[cfg(windows)]
    {
        windows_offset()
    }
}

#[cfg(unix)]
#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const i8,
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    fn time(out: *mut i64) -> i64;
}

#[cfg(unix)]
unsafe fn libc_time() -> i64 {
    unsafe { time(std::ptr::null_mut()) }
}

#[cfg(windows)]
fn windows_offset() -> i64 {
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: [u16; 8],
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: [u16; 8],
        daylight_bias: i32,
    }
    unsafe extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: the function fills in the struct we pass.
    unsafe {
        let mut info: TimeZoneInformation = std::mem::zeroed();
        let mode = GetTimeZoneInformation(&mut info);
        let daylight = if mode == 2 { info.daylight_bias } else { 0 };
        -((info.bias + daylight) as i64) * 60
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
        assert_eq!(super::civil_from_days(20_000), (2024, 10, 4));
    }
}
