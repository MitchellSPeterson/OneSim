//! Left-button gestures: a press that stays put is a click; one that moves past a threshold is a drag.

const DRAG_THRESHOLD: f64 = 4.0; // points

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    None,
    /// A click at this point.
    Click(f64, f64),
    /// The press moved past the threshold: the drag began at `from` (the press point) and is now at `to`.
    DragStart { from: (f64, f64), to: (f64, f64) },
    DragMove(f64, f64),
    /// The drag ended at this point; `from` is where it began (React Native gets the whole gesture at once).
    DragEnd { from: (f64, f64), to: (f64, f64) },
}

#[derive(Debug, Default)]
pub struct Gesture {
    state: State,
}

#[derive(Debug, Default, Clone, Copy)]
enum State {
    #[default]
    Idle,
    Pressed((f64, f64)),
    Dragging((f64, f64)),
}

impl Gesture {
    pub fn press(&mut self, p: (f64, f64)) {
        self.state = State::Pressed(p);
    }

    pub fn moved(&mut self, p: (f64, f64)) -> Out {
        match self.state {
            State::Pressed(from) if (p.0 - from.0).hypot(p.1 - from.1) >= DRAG_THRESHOLD => {
                self.state = State::Dragging(from);
                Out::DragStart { from, to: p }
            }
            State::Dragging(_) => Out::DragMove(p.0, p.1),
            _ => Out::None,
        }
    }

    pub fn release(&mut self, p: (f64, f64)) -> Out {
        match std::mem::take(&mut self.state) {
            State::Pressed(_) => Out::Click(p.0, p.1),
            State::Dragging(from) => Out::DragEnd { from, to: p },
            State::Idle => Out::None,
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_movement_is_still_a_click() {
        let mut g = Gesture::default();
        g.press((10.0, 10.0));
        assert_eq!(g.moved((12.0, 11.0)), Out::None);
        assert_eq!(g.release((12.0, 11.0)), Out::Click(12.0, 11.0));
        assert_eq!(g.release((0.0, 0.0)), Out::None);
    }

    #[test]
    fn large_movement_becomes_a_drag_from_the_press_point() {
        let mut g = Gesture::default();
        g.press((10.0, 10.0));
        assert_eq!(g.moved((10.0, 30.0)), Out::DragStart { from: (10.0, 10.0), to: (10.0, 30.0) });
        assert_eq!(g.moved((10.0, 50.0)), Out::DragMove(10.0, 50.0));
        assert_eq!(g.release((10.0, 60.0)), Out::DragEnd { from: (10.0, 10.0), to: (10.0, 60.0) });
    }

    #[test]
    fn moves_without_a_press_are_ignored() {
        let mut g = Gesture::default();
        assert_eq!(g.moved((1.0, 1.0)), Out::None);
    }
}
