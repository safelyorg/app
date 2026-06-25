use chrono::{DateTime, Datelike, NaiveDate, Utc};

pub fn format_account_age(join_date: NaiveDate) -> String {
    let today = Utc::now().date_naive();
    let years = today.year() - join_date.year();
    let months = today.month() as i32 - join_date.month() as i32;

    let (final_years, final_months) = if months < 0 {
        (years - 1, months + 12)
    } else {
        (years, months)
    };

    match (final_years, final_months) {
        (0, m) => format!("{} months", m),
        (y, 0) => format!("{} years", y),
        (y, m) => format!("{} years {} months", y, m),
    }
}

pub fn format_last_active(last_seen: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(last_seen);

    if diff.num_minutes() < 60 {
        format!("{} minutes ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{} hours ago", diff.num_hours())
    } else if diff.num_days() < 30 {
        format!("{} days ago", diff.num_days())
    } else if diff.num_days() < 365 {
        format!("{} months ago", diff.num_days() / 30)
    } else {
        format!("{} years ago", diff.num_days() / 365)
    }
}
