//! Screen quarters: with two or more games open, each game window gets its own quarter of the screen (2 x 2, the
//! whole monitor including the taskbar), in the order the games started: 1 top left, 2 top right, 3 bottom left,
//! 4 bottom right. A single game is never moved.
use std::time::{Duration, Instant};

pub const SLOTS: usize = 4;
/// How long the windows are held in their quarters after a game joins: D2R opens its window a moment after it
/// starts and may size itself again while it loads. After that the user can move them freely.
pub const HOLD: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }

/// Quarter `slot` (0..4) of a monitor.
pub fn quarter(monitor: Rect, slot: usize) -> Rect {
    let (w, h) = (monitor.w / 2, monitor.h / 2);
    Rect { x: monitor.x + (slot % 2) as i32 * w, y: monitor.y + (slot / 2) as i32 * h, w, h }
}

/// What the tiler needs from Windows (a fake one in the tests).
pub trait Screen {
    /// Main window of each pid that has one: (pid, window).
    fn windows(&mut self, pids: &[u32]) -> Vec<(u32, isize)>;
    fn monitor_of(&mut self, window: isize) -> Option<Rect>;
    fn rect_of(&mut self, window: isize) -> Option<Rect>;
    fn move_window(&mut self, window: isize, to: Rect) -> bool;
}

#[derive(Default)]
pub struct Tiler {
    slots: [Option<u32>; SLOTS],
    until: Option<Instant>,
}

impl Tiler {
    /// Which quarter each game sits in (for the window), as (slot, pid).
    pub fn placed(&self) -> Vec<(usize, u32)> { self.slots.iter().enumerate().filter_map(|(i, s)| s.map(|p| (i, p))).collect() }

    /// Called a few times a second with the running games (oldest first). Returns how many windows moved.
    pub fn tick(&mut self, now: Instant, running: &[u32], screen: &mut dyn Screen) -> usize {
        for s in self.slots.iter_mut() {
            if s.map_or(false, |p| !running.contains(&p)) { *s = None; }   // that game closed: its quarter is free
        }
        let mut wins = screen.windows(running);
        wins.sort_by_key(|w| running.iter().position(|p| *p == w.0));
        let mut joined = false;
        for (pid, _) in &wins {
            if self.slots.contains(&Some(*pid)) { continue; }
            if let Some(free) = self.slots.iter().position(|s| s.is_none()) { self.slots[free] = Some(*pid); joined = true; }
        }
        let placed: Vec<(usize, isize)> = self.slots.iter().enumerate()
            .filter_map(|(i, s)| s.and_then(|p| wins.iter().find(|w| w.0 == p)).map(|w| (i, w.1))).collect();
        if placed.len() < 2 { self.until = None; return 0; }
        if joined { self.until = Some(now + HOLD); }
        if !self.until.map_or(false, |t| now < t) { self.until = None; return 0; }
        let Some(monitor) = screen.monitor_of(placed[0].1) else { return 0 };
        let mut moved = 0;
        for (slot, win) in placed {
            let to = quarter(monitor, slot);
            if screen.rect_of(win) == Some(to) { continue; }
            if screen.move_window(win, to) { moved += 1; }
        }
        moved
    }
}

/// The real screen.
pub struct WinScreen;
impl Screen for WinScreen {
    fn windows(&mut self, pids: &[u32]) -> Vec<(u32, isize)> { crate::win::main_windows(pids) }
    fn monitor_of(&mut self, window: isize) -> Option<Rect> { crate::win::monitor_of(window) }
    fn rect_of(&mut self, window: isize) -> Option<Rect> { crate::win::window_rect(window) }
    fn move_window(&mut self, window: isize, to: Rect) -> bool { crate::win::move_window(window, to) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const MON: Rect = Rect { x: 0, y: 0, w: 2560, h: 1440 };

    #[derive(Default)]
    struct FakeScreen { windows: Vec<(u32, isize)>, rects: HashMap<isize, Rect>, moves: usize }
    impl Screen for FakeScreen {
        fn windows(&mut self, pids: &[u32]) -> Vec<(u32, isize)> { self.windows.iter().copied().filter(|w| pids.contains(&w.0)).rev().collect() }
        fn monitor_of(&mut self, _: isize) -> Option<Rect> { Some(MON) }
        fn rect_of(&mut self, w: isize) -> Option<Rect> { self.rects.get(&w).copied() }
        fn move_window(&mut self, w: isize, to: Rect) -> bool { self.rects.insert(w, to); self.moves += 1; true }
    }

    #[test]
    fn quarters_of_the_whole_screen() {
        assert_eq!(quarter(MON, 0), Rect { x: 0, y: 0, w: 1280, h: 720 });
        assert_eq!(quarter(MON, 1), Rect { x: 1280, y: 0, w: 1280, h: 720 });
        assert_eq!(quarter(MON, 2), Rect { x: 0, y: 720, w: 1280, h: 720 });
        assert_eq!(quarter(MON, 3), Rect { x: 1280, y: 720, w: 1280, h: 720 });
        let second = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        assert_eq!(quarter(second, 3), Rect { x: -960, y: 540, w: 960, h: 540 });
    }

    #[test]
    fn one_game_stays_put_two_or_more_fill_quarters_in_start_order() {
        let t0 = Instant::now();
        let mut s = FakeScreen::default();
        let mut t = Tiler::default();
        s.windows.push((100, 1));
        s.rects.insert(1, Rect { x: 50, y: 50, w: 1920, h: 1080 });
        assert_eq!(t.tick(t0, &[100], &mut s), 0);              // a single game is never moved
        assert_eq!(s.rects[&1], Rect { x: 50, y: 50, w: 1920, h: 1080 });
        // a second game started with Play (not a group): both go to quarters
        s.windows.push((200, 2));
        assert_eq!(t.tick(t0, &[100, 200], &mut s), 2);
        assert_eq!((s.rects[&1], s.rects[&2]), (quarter(MON, 0), quarter(MON, 1)));
        // a third: bottom left; windows already in place are not moved again
        s.windows.push((300, 3));
        assert_eq!(t.tick(t0 + Duration::from_secs(1), &[100, 200, 300], &mut s), 1);
        assert_eq!(s.rects[&3], quarter(MON, 2));
        assert_eq!(t.placed(), vec![(0, 100), (1, 200), (2, 300)]);
    }

    #[test]
    fn hold_ends_then_a_closed_game_frees_its_quarter() {
        let t0 = Instant::now();
        let mut s = FakeScreen::default();
        let mut t = Tiler::default();
        s.windows = vec![(100, 1), (200, 2)];
        t.tick(t0, &[100, 200], &mut s);
        // the user moves a window after the hold: it is left alone
        s.rects.insert(2, Rect { x: 5, y: 5, w: 800, h: 600 });
        assert_eq!(t.tick(t0 + HOLD + Duration::from_secs(1), &[100, 200], &mut s), 0);
        // during the hold, a window D2R resized itself goes back
        let mut t = Tiler::default();
        t.tick(t0, &[100, 200], &mut s);
        s.rects.insert(2, Rect { x: 5, y: 5, w: 800, h: 600 });
        assert_eq!(t.tick(t0 + Duration::from_secs(3), &[100, 200], &mut s), 1);
        // game 1 closes; a new game takes the free top-left quarter
        s.windows = vec![(200, 2), (400, 4)];
        t.tick(t0 + Duration::from_secs(30), &[200, 400], &mut s);
        assert_eq!(t.placed(), vec![(0, 400), (1, 200)]);
        assert_eq!(s.rects[&4], quarter(MON, 0));
    }

    #[test]
    fn a_game_without_its_window_yet_waits() {
        let mut s = FakeScreen::default();
        let mut t = Tiler::default();
        s.windows = vec![(100, 1)];
        assert_eq!(t.tick(Instant::now(), &[100, 200], &mut s), 0);   // 200 is starting, no window yet
        assert_eq!(t.placed(), vec![(0, 100)]);
    }
}
