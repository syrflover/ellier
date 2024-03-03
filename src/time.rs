use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time(pub u64, pub u64, pub u64);

impl From<Duration> for Time {
    fn from(value: Duration) -> Self {
        (&value).into()
    }
}

impl From<&Duration> for Time {
    /// returns
    /// (hours, minutes, seconds)
    fn from(duration: &Duration) -> Self {
        let mut seconds = duration.as_secs();

        let hours = seconds / 3600;

        seconds -= hours * 3600;

        let minutes = seconds / 60;

        seconds -= minutes * 60;

        Time(hours, minutes, seconds)
    }
}

impl Time {
    pub fn to_unknown_readable(separator: &str) -> String {
        ["??", "??", "??"].join(separator)
    }

    pub fn to_readable(&self, separator: &str) -> String {
        let Time(h, m, s) = *self;

        fn make_to_least_two_chars(x: u64) -> String {
            let x = x.to_string();
            if x.chars().count() == 1 {
                "0".to_owned() + &x
            } else {
                x
            }
        }

        [h, m, s].map(make_to_least_two_chars).join(separator)
    }
}
