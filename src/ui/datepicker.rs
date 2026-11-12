//! The month grid of the date picker, in the date sheet of the editor and in the pause panel
//! over the list. One picker is open at a time; Rust fills it for whichever asked.

use std::rc::Rc;

use chrono::{Datelike, NaiveDate};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::{Cal, Ctx, DateCell};
use crate::calendar;

/// A month on show, the day picked in it, and the earliest day that can be picked.
#[derive(Clone)]
pub struct DateView {
    month: NaiveDate,
    pub selected: NaiveDate,
    min: NaiveDate,
}

impl DateView {
    pub fn new(selected: NaiveDate, min: NaiveDate) -> Self {
        Self {
            month: calendar::shift_month(selected, 0),
            selected,
            min,
        }
    }

    /// Show another day, and its month.
    pub fn select(&mut self, day: NaiveDate) {
        self.selected = day;
        self.month = calendar::shift_month(day, 0);
    }

    fn can_prev(&self) -> bool {
        calendar::shift_month(self.month, -1) >= calendar::shift_month(self.min, 0)
    }

    pub fn prev(&mut self) {
        if self.can_prev() {
            self.month = calendar::shift_month(self.month, -1);
        }
    }

    pub fn next(&mut self) {
        self.month = calendar::shift_month(self.month, 1);
    }

    /// The day in this cell, if it can be picked.
    pub fn pick(&mut self, ctx: &Ctx, index: usize) -> bool {
        let week_start = ctx.settings.borrow().prefs.week_start;
        let day = calendar::month_grid(self.month, week_start)
            .get(index)
            .copied()
            .flatten()
            .filter(|day| *day >= self.min);
        if let Some(day) = day {
            self.selected = day;
        }
        day.is_some()
    }

    /// Draw the month.
    pub fn show(&self, ctx: &Ctx) {
        let Some(ui) = ctx.ui.upgrade() else {
            return;
        };
        let tr = ctx.tr.borrow();
        let week_start = ctx.settings.borrow().prefs.week_start;
        let today = chrono::Local::now().date_naive();
        let cells: Vec<DateCell> = calendar::month_grid(self.month, week_start)
            .into_iter()
            .map(|day| match day {
                Some(day) => DateCell {
                    text: day.day().to_string().into(),
                    enabled: day >= self.min,
                    selected: day == self.selected,
                    today: day == today,
                },
                None => DateCell {
                    text: SharedString::new(),
                    enabled: false,
                    selected: false,
                    today: false,
                },
            })
            .collect();
        let weekdays: Vec<SharedString> = week_start
            .day_order()
            .into_iter()
            .map(|day| SharedString::from(crate::text::day_letter(&tr, day)))
            .collect();
        let cal = ui.global::<Cal>();
        cal.set_title(
            format!(
                "{} {}",
                crate::text::month_short(&tr, self.month.month0() as usize),
                self.month.year()
            )
            .into(),
        );
        cal.set_weekdays(ModelRc::from(Rc::new(VecModel::from(weekdays))));
        cal.set_cells(ModelRc::from(Rc::new(VecModel::from(cells))));
        cal.set_can_prev(self.can_prev());
    }
}

/// What the picker was asked to do.
#[derive(Clone, Copy)]
pub enum Move {
    Prev,
    Next,
    Pick(usize),
}

/// A tap in the picker goes to the panel that has it open.
pub fn route(ctx: &Rc<Ctx>, action: Move) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    if ui.get_screen() == 1 {
        super::editor::date_action(ctx, action);
    } else {
        super::home_sheets::date_action(ctx, action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn it_does_not_go_back_before_the_month_of_the_earliest_day() {
        let mut view = DateView::new(date(2026, 12, 5), date(2026, 11, 13));
        view.prev();
        assert_eq!(view.month, date(2026, 11, 1));
        view.prev();
        assert_eq!(view.month, date(2026, 11, 1));
        view.next();
        view.next();
        assert_eq!(view.month, date(2027, 1, 1));
    }

    #[test]
    fn selecting_a_day_shows_its_month() {
        let mut view = DateView::new(date(2026, 11, 20), date(2026, 11, 13));
        view.select(date(2027, 3, 9));
        assert_eq!(view.month, date(2027, 3, 1));
        assert_eq!(view.selected, date(2027, 3, 9));
    }
}
