//! The process's local calendar date: the only reader of the system clock and time zone.

#[cfg(unix)]
use std::{env, fs, path::PathBuf};
use std::{sync::OnceLock, time::SystemTime};

use jiff::{Timestamp, tz::TimeZone};

use crate::metadata::Date;

/// Today's date in the local time zone, which is resolved once per process.
pub(crate) fn today() -> Date {
    today_at(SystemTime::now(), zone())
}

/// The date of `now` in `zone`. A clock outside jiff's years -9999 to 9999, or before 1 BCE,
/// clamps to `Date::MIN`, so `today` has no panic path.
fn today_at(now: SystemTime, zone: &TimeZone) -> Date {
    // Not `Timestamp::now()`, which panics when the system clock is outside jiff's range.
    let Ok(now) = Timestamp::try_from(now) else {
        return Date::MIN;
    };
    let date = zone.to_datetime(now).date();
    u16::try_from(date.year())
        .ok()
        .zip(u8::try_from(date.month()).ok())
        .zip(u8::try_from(date.day()).ok())
        .and_then(|((year, month), day)| Date::new(year, month, day))
        .unwrap_or(Date::MIN)
}

fn zone() -> &'static TimeZone {
    static ZONE: OnceLock<TimeZone> = OnceLock::new();
    ZONE.get_or_init(|| {
        #[cfg(unix)]
        if let Some(zone) = zoneinfo() {
            return zone;
        }
        // Absolute paths, POSIX rules, Windows names and system zone; anything else is UTC.
        TimeZone::system()
    })
}

/// `/etc/localtime` when `TZ` is unset, or a relative zone name under `$TZDIR` or
/// `/usr/share/zoneinfo`, so names resolve without jiff's zoneinfo database scan.
#[cfg(unix)]
fn zoneinfo() -> Option<TimeZone> {
    let Some(tz) = env::var_os("TZ") else {
        let bytes = fs::read("/etc/localtime").ok()?;
        return TimeZone::tzif("Local", &bytes).ok();
    };
    let tz = tz.into_string().ok()?;
    let name = tz.strip_prefix(':').unwrap_or(&tz);
    let relative = !name.is_empty()
        && !name.starts_with('/')
        && !name.split('/').any(|part| part.is_empty() || part == "..");
    if !relative {
        return None;
    }
    let dir = env::var_os("TZDIR")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("/usr/share/zoneinfo"), PathBuf::from);
    let bytes = fs::read(dir.join(name)).ok()?;
    TimeZone::tzif(name, &bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::tz;
    use std::{env, process::Command};

    const UTC: i8 = 0;

    /// Runs `today` in a child test process with `TZ` (and optionally `TZDIR`) set as given.
    fn child_today(tz: Option<&str>, tzdir: Option<&std::path::Path>) -> String {
        let mut command = Command::new(env::current_exe().unwrap());
        command.args([
            "--ignored",
            "--exact",
            "clock::tests::today_helper",
            "--nocapture",
            "--test-threads=1",
        ]);
        match tz {
            Some(tz) => command.env("TZ", tz),
            None => command.env_remove("TZ"),
        };
        match tzdir {
            Some(dir) => command.env("TZDIR", dir),
            None => command.env_remove("TZDIR"),
        };
        let output = command.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(output.status.success(), "{tz:?}: {stdout}");
        stdout
            .lines()
            // The harness prints the test name on the same line.
            .find_map(|line| line.split_once("today=")?.1.get(..10))
            .unwrap_or_else(|| panic!("{tz:?}: no date in {stdout:?}"))
            .to_owned()
    }

    fn date_at(hours: i8) -> String {
        Timestamp::now()
            .to_zoned(TimeZone::fixed(tz::offset(hours)))
            .date()
            .to_string()
    }

    /// The child's date and the expected date at a fixed offset, retried once across midnight.
    fn assert_today(tz: Option<&str>, tzdir: Option<&std::path::Path>, hours: i8) {
        for _ in 0..2 {
            let before = date_at(hours);
            let today = child_today(tz, tzdir);
            if before == date_at(hours) {
                assert_eq!(today, before, "TZ={tz:?} TZDIR={tzdir:?}");
                return;
            }
        }
        panic!("the date changed twice while TZ={tz:?} ran");
    }

    #[test]
    #[ignore = "child helper, run under a chosen TZ by the clock tests"]
    fn today_helper() {
        println!("today={}", today());
    }

    #[test]
    fn today_should_follow_posix_rules_and_iana_names() {
        assert_today(Some("UTC0"), None, UTC);
        assert_today(Some("<+14>-14"), None, 14);
        assert_today(Some("<-12>12"), None, -12);
        // The `Etc` sign is inverted: `Etc/GMT-14` is UTC+14.
        assert_today(Some("Etc/GMT-14"), None, 14);
        assert_today(Some("Etc/GMT+12"), None, -12);
    }

    #[test]
    fn today_should_fall_back_to_utc_for_unknown_or_empty_zones() {
        assert_today(Some("Bogus/Zone"), None, UTC);
        assert_today(Some(""), None, UTC);
    }

    #[test]
    fn today_without_tz_should_use_the_system_zone() {
        let today = jiff::civil::Date::strptime("%Y-%m-%d", child_today(None, None)).unwrap();
        let utc = Timestamp::now().to_zoned(TimeZone::UTC).date();
        let days = (today - utc).get_days().abs();
        assert!(days <= 1, "{today} is {days} days from UTC {utc}");
    }

    #[cfg(unix)]
    #[test]
    fn today_should_resolve_colon_paths_tzdir_and_refuse_parent_components() {
        // UTC+14 and UTC-12 are 26 hours apart, so at any hour at least one of them is on a
        // different date from UTC, and a lookup that fell back to UTC fails one assertion.
        assert_today(Some(":Etc/GMT-14"), None, 14);
        assert_today(Some(":Etc/GMT+12"), None, -12);
        assert_today(Some("/usr/share/zoneinfo/Etc/GMT-14"), None, 14);
        assert_today(Some(":/usr/share/zoneinfo/Etc/GMT+12"), None, -12);

        let root = tempfile::tempdir().unwrap();
        let zones = root.path().join("zones");
        fs::create_dir_all(zones.join("Made")).unwrap();
        fs::create_dir_all(root.path().join("Etc")).unwrap();
        for (name, hours) in [("GMT-14", 14), ("GMT+12", -12)] {
            let zone = fs::read(format!("/usr/share/zoneinfo/Etc/{name}")).unwrap();
            let made_up = format!("Made/Up{hours}");
            fs::write(zones.join(&made_up), &zone).unwrap();
            fs::write(root.path().join("Etc").join(name), &zone).unwrap();
            assert_today(Some(&made_up), Some(&zones), hours);
            assert_today(Some(&format!(":{made_up}")), Some(&zones), hours);
            // A name may not leave the zone directory, even when the target is a real zone.
            assert_today(Some(&format!("../Etc/{name}")), Some(&zones), UTC);
            assert_today(Some(&made_up.replace('/', "//")), Some(&zones), UTC);
        }
    }

    #[test]
    fn clocks_outside_the_civil_range_should_clamp_to_the_minimum_date() {
        use std::time::Duration;
        // About year 14,600: past jiff's range, so the timestamp conversion itself fails.
        let future = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(400_000_000_000));
        // About 1 BCE: inside jiff's range, but a negative year.
        let ancient = SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(62_200_000_000));
        for now in [future, ancient].into_iter().flatten() {
            assert_eq!(today_at(now, &TimeZone::UTC), Date::MIN, "{now:?}");
        }
        assert_eq!(
            today_at(
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_584_000),
                &TimeZone::UTC
            ),
            Date::new(2026, 10, 21).unwrap()
        );
    }

    #[test]
    fn today_should_reuse_the_cached_zone() {
        assert!(std::ptr::eq(zone(), zone()));
        let first = today();
        let second = today();
        assert!(second == first || second == first.succ().unwrap());
    }
}
