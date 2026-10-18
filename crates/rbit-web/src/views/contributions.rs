//! Shared, server-rendered contribution calendar for accounts and repositories.
use askama::Template;
use askama_web::WebTemplate;
use rbit_services::contributions::Contributions;
use time::{Date, Duration, Month, OffsetDateTime};

use super::{layout::Page, repo::RepoHeader};

#[derive(Debug, Clone)]
pub struct ContributionCalendar {
    pub year: i32,
    pub years: Vec<i32>,
    pub total: usize,
    pub active_days: usize,
    pub longest_streak: usize,
    pub repositories: usize,
    pub partial: bool,
    pub days: Vec<ContributionDay>,
    pub months: Vec<ContributionMonth>,
    pub quarters: Vec<ContributionQuarter>,
    pub scope: &'static str,
    pub tab: &'static str,
}

#[derive(Debug, Clone)]
pub struct ContributionDay {
    pub x: usize,
    pub y: usize,
    pub date: String,
    pub count: usize,
    pub level: usize,
}

#[derive(Debug, Clone)]
pub struct ContributionMonth {
    pub label: &'static str,
    pub x: usize,
    pub count: usize,
    pub bar_height: usize,
    pub bar_y: usize,
}

#[derive(Debug, Clone)]
pub struct ContributionQuarter {
    pub label: &'static str,
    pub days: Vec<ContributionDay>,
    pub months: Vec<ContributionMonth>,
}

impl ContributionCalendar {
    pub fn new(year: Option<i32>, scope: &'static str, tab: &'static str) -> Self {
        let current = OffsetDateTime::now_utc().year();
        // Imported history can predate the account or repository's creation here.
        let first = 1970;
        Self {
            year: year.unwrap_or(current).clamp(first, current),
            years: (first..=current).rev().collect(),
            total: 0,
            active_days: 0,
            longest_streak: 0,
            repositories: 0,
            partial: false,
            days: Vec::new(),
            months: Vec::new(),
            quarters: Vec::new(),
            scope,
            tab,
        }
    }

    pub fn bounds(&self) -> (i64, i64) {
        let start =
            Date::from_calendar_date(self.year, Month::January, 1).expect("valid calendar year").midnight().assume_utc().unix_timestamp();
        let end = Date::from_calendar_date(self.year + 1, Month::January, 1)
            .expect("valid calendar year")
            .midnight()
            .assume_utc()
            .unix_timestamp();
        let tomorrow = (OffsetDateTime::now_utc().date() + Duration::days(1)).midnight().assume_utc().unix_timestamp();
        (start, end.min(tomorrow))
    }

    #[must_use]
    pub fn populate(mut self, contributions: &Contributions) -> Self {
        let start = Date::from_calendar_date(self.year, Month::January, 1).expect("valid calendar year");
        let end = Date::from_calendar_date(self.year, Month::December, 31).expect("valid calendar year");
        let today = OffsetDateTime::now_utc().date();
        let offset = usize::from(start.weekday().number_days_from_sunday());
        let peak = contributions.days.values().copied().max().unwrap_or(1).max(1);
        let mut streak = 0;
        let mut monthly = [0; 12];
        let mut date = start;
        while date <= end.min(today) {
            let day = date.midnight().assume_utc().unix_timestamp().div_euclid(86400);
            let count = contributions.days.get(&day).copied().unwrap_or(0);
            let index = usize::try_from((date - start).whole_days()).unwrap_or_default() + offset;
            self.total += count;
            self.active_days += usize::from(count > 0);
            streak = if count > 0 { streak + 1 } else { 0 };
            self.longest_streak = self.longest_streak.max(streak);
            monthly[date.month() as usize - 1] += count;
            self.days.push(ContributionDay {
                x: 36 + index / 7 * 15,
                y: 24 + index % 7 * 15,
                date: date.to_string(),
                count,
                level: if count == 0 { 0 } else { (count * 4).div_ceil(peak).min(4) },
            });
            date += Duration::days(1);
        }
        let max_month = monthly.iter().copied().max().unwrap_or(1).max(1);
        for (index, label) in ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].into_iter().enumerate() {
            let month = Month::try_from(u8::try_from(index + 1).expect("month index fits in u8")).expect("month");
            let month_date = Date::from_calendar_date(self.year, month, 1).expect("date");
            let week = (usize::try_from((month_date - start).whole_days()).unwrap_or_default() + offset) / 7;
            let height = monthly[index] * 64 / max_month;
            self.months.push(ContributionMonth { label, x: 36 + week * 15, count: monthly[index], bar_height: height, bar_y: 72 - height });
        }
        for (quarter, label) in ["January to March", "April to June", "July to September", "October to December"].into_iter().enumerate() {
            let months = &self.months[quarter * 3..quarter * 3 + 3];
            let first_x = months[0].x;
            let days = self
                .days
                .iter()
                .filter(|day| {
                    let month = day.date[5..7].parse::<usize>().expect("calendar month");
                    (month - 1) / 3 == quarter
                })
                .cloned()
                .map(|mut day| {
                    day.x = 36 + day.x - first_x;
                    day
                })
                .collect();
            let months = months
                .iter()
                .cloned()
                .map(|mut month| {
                    month.x = 36 + month.x - first_x;
                    month
                })
                .collect();
            self.quarters.push(ContributionQuarter { label, days, months });
        }
        self.repositories = contributions.repositories;
        self.partial = contributions.partial;
        self
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/activity.html")]
pub struct ActivityPage {
    pub page: Page,
    pub header: RepoHeader,
    pub contributions: ContributionCalendar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_year_excludes_future_days_and_empty_counts_stay_zero() {
        let calendar = ContributionCalendar::new(Some(i32::MAX), "test", "").populate(&Contributions::default());
        let today = OffsetDateTime::now_utc().date();
        assert_eq!(calendar.year, today.year());
        assert_eq!(calendar.days.len(), usize::from(today.ordinal()));
        assert_eq!(calendar.days.last().unwrap().date, today.to_string());
        assert_eq!((calendar.total, calendar.active_days, calendar.longest_streak), (0, 0, 0));
        assert!(calendar.days.iter().all(|d| d.level == 0));
        assert_eq!(ContributionCalendar::new(Some(i32::MIN), "test", "").year, 1970);
    }

    #[test]
    fn leap_year_alignment_totals_and_streaks() {
        let calendar = ContributionCalendar::new(Some(2024), "test", "");
        let start = calendar.bounds().0.div_euclid(86400);
        let data = Contributions { days: [(start, 2), (start + 1, 3), (start + 59, 1)].into(), repositories: 2, partial: false };
        let calendar = calendar.populate(&data);
        assert_eq!(calendar.days.len(), 366);
        assert_eq!((calendar.total, calendar.active_days, calendar.longest_streak), (6, 3, 2));
        assert_eq!((calendar.days[0].x, calendar.days[0].y), (36, 39)); // Monday
        assert_eq!(calendar.days[59].date, "2024-02-29");
        assert_eq!(calendar.months[0].count, 5);
        assert_eq!(calendar.months[1].count, 1);
        assert!(calendar.days.iter().all(|d| d.level <= 4));
        assert_eq!(calendar.quarters.iter().map(|q| q.days.len()).sum::<usize>(), 366);
        assert_eq!(calendar.quarters.iter().map(|q| q.months.len()).sum::<usize>(), 12);
        assert!(calendar.quarters.iter().flat_map(|q| &q.days).all(|d| d.x + 12 <= 246));
    }
}
