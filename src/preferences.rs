use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};

pub const STORAGE_KEY: &str = "sannygit.preferences";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileDoubleClick {
    #[default]
    ShowDiff,
    ToggleStage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateFormat {
    #[default]
    Iso,
    DayMonthYear,
    MonthDayYear,
}

impl DateFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Iso => "yyyy-MM-dd",
            Self::DayMonthYear => "dd.MM.yyyy",
            Self::MonthDayYear => "MM/dd/yyyy",
        }
    }

    fn pattern(self) -> &'static str {
        match self {
            Self::Iso => "%Y-%m-%d",
            Self::DayMonthYear => "%d.%m.%Y",
            Self::MonthDayYear => "%m/%d/%Y",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeFormat {
    #[default]
    Hours24,
    Hours12,
}

impl TimeFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hours24 => "HH:mm",
            Self::Hours12 => "hh:mm AM/PM",
        }
    }

    fn pattern(self) -> &'static str {
        match self {
            Self::Hours24 => "%H:%M",
            Self::Hours12 => "%I:%M %p",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub language: Language,
    pub git_executable: String,
    pub follow_system_theme: bool,
    pub show_welcome: bool,
    pub file_double_click: FileDoubleClick,
    pub date_format: DateFormat,
    pub time_format: TimeFormat,
    pub verbose_dates: bool,
    pub skip_sync_confirmation: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::English,
            git_executable: String::new(),
            follow_system_theme: true,
            show_welcome: true,
            file_double_click: FileDoubleClick::default(),
            date_format: DateFormat::default(),
            time_format: TimeFormat::default(),
            verbose_dates: false,
            skip_sync_confirmation: false,
        }
    }
}

impl Preferences {
    pub fn commit_date(&self, raw: &str) -> String {
        self.commit_date_at(raw, Local::now())
    }

    pub fn commit_date_at<Tz: TimeZone>(&self, raw: &str, now: DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        let Ok(date) = DateTime::parse_from_rfc3339(raw) else {
            return raw.to_owned();
        };
        let date = date.with_timezone(&now.timezone());
        let days = now
            .date_naive()
            .signed_duration_since(date.date_naive())
            .num_days();
        let day = match (self.verbose_dates, days) {
            (true, 0) => "Today".into(),
            (true, 1) => "Yesterday".into(),
            _ => date.format(self.date_format.pattern()).to_string(),
        };
        format!("{day} {}", date.format(self.time_format.pattern()))
    }

    pub fn date_example(&self) -> String {
        Local::now().format(self.date_format.pattern()).to_string()
    }

    pub fn time_example(&self) -> String {
        Local::now().format(self.time_format.pattern()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_use_selected_formats_and_local_calendar_days() {
        let now = DateTime::parse_from_rfc3339("2026-11-09T00:15:00+07:00").unwrap();
        let mut preferences = Preferences {
            date_format: DateFormat::DayMonthYear,
            time_format: TimeFormat::Hours12,
            ..Default::default()
        };
        assert_eq!(
            preferences.commit_date_at("2026-11-08T18:00:00Z", now),
            "09.11.2026 01:00 AM"
        );
        preferences.verbose_dates = true;
        assert_eq!(
            preferences.commit_date_at("2026-11-08T18:00:00Z", now),
            "Today 01:00 AM"
        );
        assert_eq!(
            preferences.commit_date_at("2026-11-08T16:30:00Z", now),
            "Yesterday 11:30 PM"
        );
        preferences.time_format = TimeFormat::Hours24;
        preferences.date_format = DateFormat::MonthDayYear;
        assert_eq!(
            preferences.commit_date_at("2026-11-07T05:00:00Z", now),
            "11/07/2026 12:00"
        );
        assert_eq!(
            preferences.commit_date_at("2026-11-10T05:00:00Z", now),
            "11/10/2026 12:00"
        );
        assert_eq!(
            preferences.commit_date_at("invalid date", now),
            "invalid date"
        );
    }

    #[test]
    fn preferences_round_trip_and_missing_fields_use_defaults() {
        let preferences = Preferences {
            file_double_click: FileDoubleClick::ToggleStage,
            verbose_dates: true,
            skip_sync_confirmation: true,
            git_executable: "/Applications/Git Tools/git".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&preferences).unwrap();
        assert_eq!(
            serde_json::from_str::<Preferences>(&json).unwrap(),
            preferences
        );
        assert_eq!(
            serde_json::from_str::<Preferences>("{}").unwrap(),
            Preferences::default()
        );
    }
}
