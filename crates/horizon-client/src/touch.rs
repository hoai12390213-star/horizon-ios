//! Native Rust port of TouchController's movement and interaction modes. It supports D-pad or virtual
//! joystick movement, gesture or split interactions, context-sensitive swim/fly buttons, and the preset's
//! top-bar actions. A free finger can turn the view; gesture mode also taps or holds to interact.
//!
//! This module is the layout and the fingers' state: positions in GUI pixels in, [`Out`]s for the game to act
//! on out. game/touch.rs feeds it winit's touches and acts on what comes back.

use std::{collections::BTreeMap, fs, path::Path};

/// Touch layout and look settings, kept outside vanilla's `options.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct TouchSettings {
    pub button_scale: f32,
    pub look_sensitivity: f32,
    pub move_method: MoveMethod,
    pub control_style: ControlStyle,
    pub button_opacity: f32,
    pub sprint_button: SprintButtonLocation,
    pub joystick_sprint: bool,
    pub edit_layout: bool,
    /// Per-control screen-relative position offsets, stored as fractions of width and height.
    pub positions: BTreeMap<String, [f32; 2]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MoveMethod {
    #[default]
    Dpad,
    Joystick,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlStyle {
    #[default]
    TouchGesture,
    SplitControls,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RideKind {
    #[default]
    None,
    Mount,
    Boat,
    Minecart,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SprintButtonLocation {
    #[default]
    None,
    Right,
    RightTop,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            button_scale: 1.0,
            look_sensitivity: 1.0,
            move_method: MoveMethod::Dpad,
            control_style: ControlStyle::TouchGesture,
            button_opacity: 0.67,
            sprint_button: SprintButtonLocation::None,
            joystick_sprint: false,
            edit_layout: false,
            positions: BTreeMap::new(),
        }
    }
}

impl TouchSettings {
    pub const FILE: &str = "horizon-touch.txt";

    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(Self::FILE))
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let path = dir.join(Self::FILE);
        if *self != Self::default() || path.exists() {
            let _ = fs::write(path, self.serialize());
        }
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if let Some(control) = key.trim().strip_prefix("position.") {
                let Some((x, y)) = value.trim().split_once(',') else {
                    continue;
                };
                if let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>())
                    && x.is_finite()
                    && y.is_finite()
                {
                    settings
                        .positions
                        .insert(control.to_owned(), [x.clamp(-0.8, 0.8), y.clamp(-0.8, 0.8)]);
                }
                continue;
            }
            match key.trim() {
                "buttonScale" | "lookSensitivity" => {
                    let Ok(value) = value.trim().parse::<f32>() else {
                        continue;
                    };
                    if !value.is_finite() {
                        continue;
                    }
                    match key.trim() {
                        "buttonScale" => settings.button_scale = value.clamp(0.7, 1.5),
                        _ => settings.look_sensitivity = value.clamp(0.25, 2.0),
                    }
                }
                "moveMethod" => {
                    settings.move_method = match value.trim().to_ascii_lowercase().as_str() {
                        "joystick" => MoveMethod::Joystick,
                        _ => MoveMethod::Dpad,
                    }
                }
                "controlStyle" => {
                    settings.control_style = match value.trim().to_ascii_lowercase().as_str() {
                        "split" | "splitcontrols" => ControlStyle::SplitControls,
                        _ => ControlStyle::TouchGesture,
                    }
                }
                "buttonOpacity" => {
                    if let Ok(value) = value.trim().parse::<f32>()
                        && value.is_finite()
                    {
                        settings.button_opacity = value.clamp(0.2, 1.0);
                    }
                }
                "sprintButton" => {
                    settings.sprint_button = match value.trim().to_ascii_lowercase().as_str() {
                        "right" => SprintButtonLocation::Right,
                        "righttop" | "right_top" => SprintButtonLocation::RightTop,
                        _ => SprintButtonLocation::None,
                    }
                }
                "joystickSprint" => settings.joystick_sprint = value.trim() == "true",
                "editLayout" => settings.edit_layout = value.trim() == "true",
                _ => {}
            }
        }
        settings
    }

    pub fn serialize(&self) -> String {
        format!(
            "buttonScale: {:.2}\nlookSensitivity: {:.2}\nmoveMethod: {}\ncontrolStyle: {}\nbuttonOpacity: {:.2}\nsprintButton: {}\njoystickSprint: {}\neditLayout: {}\n{}",
            self.button_scale.clamp(0.7, 1.5),
            self.look_sensitivity.clamp(0.25, 2.0),
            match self.move_method {
                MoveMethod::Dpad => "dpad",
                MoveMethod::Joystick => "joystick",
            },
            match self.control_style {
                ControlStyle::TouchGesture => "gesture",
                ControlStyle::SplitControls => "split",
            },
            self.button_opacity.clamp(0.2, 1.0),
            match self.sprint_button {
                SprintButtonLocation::None => "none",
                SprintButtonLocation::Right => "right",
                SprintButtonLocation::RightTop => "right_top",
            },
            self.joystick_sprint,
            self.edit_layout,
            self.positions
                .iter()
                .map(|(control, offset)| format!(
                    "position.{control}: {:.4},{:.4}\n",
                    offset[0], offset[1]
                ))
                .collect::<String>(),
        )
    }
}

/// A button of the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Forward,
    Back,
    Left,
    Right,
    /// The D-pad's centre: sneaking, toggled.
    Sneak,
    Jump,
    Ascend,
    Descend,
    Attack,
    Use,
    Joystick,
    Dismount,
    Sprint,
    Perspective,
    Chat,
    Pause,
    Inventory,
    Hotbar(u8),
}

impl Control {
    pub fn layout_key(self) -> String {
        format!("{self:?}")
    }
}

impl Control {
    /// The D-pad's directions, which a finger can slide between.
    fn moves(self) -> bool {
        matches!(
            self,
            Control::Forward | Control::Back | Control::Left | Control::Right
        )
    }
}

/// `[x, y, width, height]` in GUI pixels.
pub type Rect = [f32; 4];

/// The D-pad's and jump's button size on a `h` GUI-pixel-high screen: about a seventh of the height, so a
/// phone (GUI scale 4, 292 high) gets 40 GUI pixels, some 53 points, over Apple's 44-point minimum.
pub fn button_size(h: f32) -> f32 {
    (h * 0.14).clamp(26.0, 48.0)
}
/// The gap kept from the screen's edges.
const MARGIN: f32 = 14.0;
/// How far above the hotbar its slots still take a tap: a thumb lands high.
const HOTBAR_REACH: f32 = 8.0;

/// Where every button is on a `w` x `h` GUI-pixel screen.
pub fn layout(w: f32, h: f32) -> Vec<(Control, Rect)> {
    layout_scaled(w, h, 1.0)
}

/// The default layout with a player-selected button size multiplier.
pub fn layout_scaled(w: f32, h: f32, scale: f32) -> Vec<(Control, Rect)> {
    layout_for(w, h, scale, false)
}

/// The TouchController preset swaps crouch/jump for vertical swim or flight controls.
pub fn layout_for(w: f32, h: f32, scale: f32, vertical: bool) -> Vec<(Control, Rect)> {
    layout_configured(
        w,
        h,
        scale,
        vertical,
        MoveMethod::Dpad,
        ControlStyle::TouchGesture,
        RideKind::None,
        SprintButtonLocation::None,
    )
}

/// TouchController's movement and interaction variants over the shared Horizon screen layout.
pub fn layout_configured(
    w: f32,
    h: f32,
    scale: f32,
    vertical: bool,
    movement: MoveMethod,
    style: ControlStyle,
    ride: RideKind,
    sprint_button: SprintButtonLocation,
) -> Vec<(Control, Rect)> {
    let s = button_size(h) * scale.clamp(0.7, 1.5);
    let small = (s * 0.65).round();
    let (cx, cy) = (MARGIN + s, h - MARGIN - 2.0 * s - 8.0);
    let split = style == ControlStyle::SplitControls;
    let mut v = Vec::new();
    if ride == RideKind::Minecart {
        match movement {
            MoveMethod::Dpad => v.push((Control::Forward, [cx, cy, s, s])),
            MoveMethod::Joystick => {
                let side = s * 2.0;
                v.push((Control::Joystick, [MARGIN, h - MARGIN - side, side, side]));
            }
        }
        v.push((Control::Dismount, [w - MARGIN - s, cy, s, s]));
    } else {
        match movement {
            MoveMethod::Dpad => {
                v.extend([
                    (Control::Forward, [cx, cy - s, s, s]),
                    (Control::Left, [cx - s, cy, s, s]),
                    (Control::Right, [cx + s, cy, s, s]),
                    (Control::Back, [cx, cy + s, s, s]),
                ]);
                if !vertical && !split {
                    v.push((Control::Sneak, [cx, cy, s, s]));
                }
            }
            MoveMethod::Joystick => {
                let side = s * 2.0;
                v.push((Control::Joystick, [MARGIN, h - MARGIN - side, side, side]));
            }
        }
        if ride == RideKind::Boat {
            v.push((Control::Dismount, [w - MARGIN - s, cy + s * 0.55, s, s]));
        }
    }
    if ride != RideKind::Minecart && vertical {
        v.push((Control::Ascend, [w - MARGIN - s, cy, s, s]));
        v.push((Control::Descend, [w - MARGIN - s, cy + s + 4.0, s, s]));
    } else if ride != RideKind::Minecart && (split || movement == MoveMethod::Joystick) {
        v.push((Control::Jump, [w - MARGIN - s, cy - s * 0.55, s, s]));
        v.push((Control::Sneak, [w - MARGIN - s, cy + s * 0.55, s, s]));
    } else if ride != RideKind::Minecart {
        v.push((Control::Jump, [w - MARGIN - s, cy, s, s]));
    }
    if ride == RideKind::Mount {
        v.push((Control::Dismount, [w - MARGIN - s, cy + s + 4.0, s, s]));
    }
    if split {
        v.push((Control::Attack, [w - MARGIN - 2.1 * s, cy - s * 0.5, s, s]));
        v.push((Control::Use, [w - MARGIN - 2.1 * s, cy + s * 0.65, s, s]));
    }
    match sprint_button {
        SprintButtonLocation::None => {}
        SprintButtonLocation::Right => {
            v.push((Control::Sprint, [w - MARGIN - s, cy - s * 1.65, s, s]));
        }
        SprintButtonLocation::RightTop => {
            v.push((
                Control::Sprint,
                [w - MARGIN - small, 2.0 + small + 4.0, small, small],
            ));
        }
    }
    let top = [Control::Chat, Control::Pause];
    let x0 = (w - 2.0 * small - 4.0) / 2.0;
    for (i, control) in top.into_iter().enumerate() {
        v.push((control, [x0 + i as f32 * (small + 4.0), 2.0, small, small]));
    }
    // Gui.extractItemHotbar's slots: the bar is 182 wide, centred, 22 high at the bottom.
    let (hx, hy) = ((w / 2.0 - 91.0).floor(), h - 22.0);
    for slot in 0..9u8 {
        v.push((
            Control::Hotbar(slot),
            [
                hx + 1.0 + 20.0 * f32::from(slot),
                hy - HOTBAR_REACH,
                20.0,
                22.0 + HOTBAR_REACH,
            ],
        ));
    }
    v.push((
        Control::Inventory,
        [hx + 184.0, hy - HOTBAR_REACH, 30.0, 22.0 + HOTBAR_REACH],
    ));
    v
}

/// The close button over a screen that has none of its own (an inventory, chat, a sign): the top right.
pub fn close_rect(w: f32, h: f32) -> Rect {
    let s = (button_size(h) * 0.75).round();
    [w - s - 4.0, 4.0, s, s]
}

/// The keyboard button, left of the close button: over a focused text field, it asks for the system's
/// keyboard again.
pub fn keyboard_rect(w: f32, h: f32) -> Rect {
    let c = close_rect(w, h);
    [c[0] - c[2] - 4.0, c[1], c[2], c[3]]
}

/// The point is inside the rectangle.
pub fn hit_rect(r: Rect, at: (f32, f32)) -> bool {
    contains(r, at)
}

fn contains(r: Rect, (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

/// The button under a point.
pub fn hit(layout: &[(Control, Rect)], at: (f32, f32)) -> Option<Control> {
    layout
        .iter()
        .find(|(_, r)| contains(*r, at))
        .map(|(c, _)| *c)
}

/// What the game does with the fingers' changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Press(Control),
    Release(Control),
    /// Turn the view by this much, in GUI pixels dragged.
    Look(f32, f32),
    /// A short tap away from the buttons: use, or attack what the crosshair is on.
    Tap,
    /// A finger held still away from the buttons: mining starts, and stops when it lifts.
    MineStart,
    MineEnd,
    /// A virtual joystick's normalized movement vector (`[left, forward]`).
    Move([f32; 2]),
    /// The virtual joystick finger lifted.
    MoveEnd,
}

/// How far a finger moves (GUI pixels) before a touch is a drag and not a tap.
const SLOP: f32 = 8.0;
/// How long a finger stays still (seconds) before it mines.
const HOLD: f64 = 0.35;

#[derive(Clone, Debug)]
enum Kind {
    Button(Control),
    Joystick { center: (f32, f32), radius: f32 },
    Ignored,
    World { dragged: bool, mining: bool },
}

fn joystick_vector(center: (f32, f32), radius: f32, at: (f32, f32)) -> [f32; 2] {
    let x = (at.0 - center.0) / radius;
    let y = (at.1 - center.1) / radius;
    let length = x.hypot(y);
    let (x, y) = if length > 1.0 {
        (x / length, y / length)
    } else {
        (x, y)
    };
    [-x, -y]
}

#[derive(Clone, Debug)]
struct Finger {
    id: u64,
    kind: Kind,
    start: (f32, f32),
    last: (f32, f32),
    since: f64,
}

/// The fingers on the screen while playing.
#[derive(Clone, Debug, Default)]
pub struct Touches {
    fingers: Vec<Finger>,
}

impl Touches {
    /// A finger went down at `at`, `now` seconds into some clock.
    pub fn start(
        &mut self,
        id: u64,
        at: (f32, f32),
        now: f64,
        layout: &[(Control, Rect)],
    ) -> Vec<Out> {
        let mut out = self.end(id);
        let kind = match hit(layout, at) {
            Some(Control::Joystick) => {
                if self
                    .fingers
                    .iter()
                    .any(|finger| matches!(finger.kind, Kind::Joystick { .. }))
                {
                    Kind::Ignored
                } else {
                    let rect = layout
                        .iter()
                        .find(|(control, _)| *control == Control::Joystick)
                        .map_or([at.0, at.1, 1.0, 1.0], |(_, rect)| *rect);
                    let center = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
                    let radius = rect[2].min(rect[3]) / 2.0;
                    out.push(Out::Move(joystick_vector(center, radius, at)));
                    Kind::Joystick { center, radius }
                }
            }
            Some(control) => {
                out.push(Out::Press(control));
                Kind::Button(control)
            }
            None => Kind::World {
                dragged: false,
                mining: false,
            },
        };
        self.fingers.push(Finger {
            id,
            kind,
            start: at,
            last: at,
            since: now,
        });
        out
    }

    /// A finger moved: the D-pad follows it from direction to direction, and a finger on the world turns
    /// the view.
    pub fn moved(&mut self, id: u64, at: (f32, f32), layout: &[(Control, Rect)]) -> Vec<Out> {
        let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match &mut f.kind {
            Kind::Joystick { center, radius } => {
                out.push(Out::Move(joystick_vector(*center, *radius, at)));
            }
            Kind::Button(control) if control.moves() => {
                if let Some(next) = hit(layout, at).filter(|c| c.moves() && c != control) {
                    out.push(Out::Release(*control));
                    out.push(Out::Press(next));
                    *control = next;
                }
            }
            Kind::Button(_) => {}
            Kind::Ignored => {}
            Kind::World { dragged, .. } => {
                let (dx, dy) = (at.0 - f.start.0, at.1 - f.start.1);
                if !*dragged && dx.hypot(dy) > SLOP {
                    *dragged = true;
                }
                if *dragged {
                    out.push(Out::Look(at.0 - f.last.0, at.1 - f.last.1));
                }
            }
        }
        f.last = at;
        out
    }

    /// A finger lifted (or the system cancelled it).
    pub fn end(&mut self, id: u64) -> Vec<Out> {
        let Some(i) = self.fingers.iter().position(|f| f.id == id) else {
            return Vec::new();
        };
        match self.fingers.remove(i).kind {
            Kind::Joystick { .. } => vec![Out::MoveEnd],
            Kind::Button(control) => vec![Out::Release(control)],
            Kind::Ignored => Vec::new(),
            Kind::World { mining: true, .. } => vec![Out::MineEnd],
            Kind::World { dragged: false, .. } => vec![Out::Tap],
            Kind::World { .. } => Vec::new(),
        }
    }

    /// Once a frame: a finger on the world that has stayed still long enough starts mining.
    pub fn frame(&mut self, now: f64) -> Vec<Out> {
        let mut out = Vec::new();
        for f in &mut self.fingers {
            if let Kind::World {
                dragged: false,
                mining,
            } = &mut f.kind
                && !*mining
                && now - f.since >= HOLD
            {
                *mining = true;
                out.push(Out::MineStart);
            }
        }
        out
    }

    /// Every finger lifted at once (a screen opened over the game).
    pub fn release_all(&mut self) -> Vec<Out> {
        let ids: Vec<u64> = self.fingers.iter().map(|f| f.id).collect();
        ids.into_iter()
            .flat_map(|id| {
                let mut out = self.end(id);
                // A cancelled touch is no tap.
                out.retain(|o| *o != Out::Tap);
                out
            })
            .collect()
    }

    /// A finger holds this button down (drawn pressed).
    pub fn held(&self, control: Control) -> bool {
        self.fingers
            .iter()
            .any(|f| matches!(f.kind, Kind::Button(c) if c == control))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(layout: &[(Control, Rect)], control: Control) -> (f32, f32) {
        let r = layout.iter().find(|(c, _)| *c == control).unwrap().1;
        (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
    }

    #[test]
    fn buttons_do_not_overlap_and_fit_the_screen() {
        for (w, h) in [(633.0, 292.0), (427.0, 240.0), (480.0, 270.0)] {
            let l = layout(w, h);
            for (i, (a, ra)) in l.iter().enumerate() {
                assert!(
                    ra[0] >= 0.0 && ra[1] >= 0.0 && ra[0] + ra[2] <= w && ra[1] + ra[3] <= h,
                    "{a:?} off screen"
                );
                for (b, rb) in &l[i + 1..] {
                    let apart = ra[0] + ra[2] <= rb[0]
                        || rb[0] + rb[2] <= ra[0]
                        || ra[1] + ra[3] <= rb[1]
                        || rb[1] + rb[3] <= ra[1];
                    assert!(apart, "{a:?} overlaps {b:?} at {w}x{h}");
                }
            }
        }
    }

    #[test]
    fn the_dpad_follows_a_sliding_finger() {
        let l = layout(633.0, 292.0);
        let mut t = Touches::default();
        assert_eq!(
            t.start(1, at(&l, Control::Forward), 0.0, &l),
            [Out::Press(Control::Forward)]
        );
        assert!(t.held(Control::Forward));
        assert_eq!(
            t.moved(1, at(&l, Control::Right), &l),
            [Out::Release(Control::Forward), Out::Press(Control::Right)]
        );
        // Off the pad, the last direction stays held, as in Pocket Edition.
        assert_eq!(t.moved(1, (300.0, 100.0), &l), []);
        assert_eq!(t.end(1), [Out::Release(Control::Right)]);
        assert!(!t.held(Control::Right));
    }

    #[test]
    fn a_quick_still_touch_taps_and_a_held_one_mines() {
        let l = layout(633.0, 292.0);
        let mut t = Touches::default();
        assert_eq!(t.start(1, (300.0, 100.0), 0.0, &l), []);
        assert_eq!(t.frame(0.1), []);
        assert_eq!(t.end(1), [Out::Tap]);

        t.start(2, (300.0, 100.0), 1.0, &l);
        assert_eq!(t.frame(1.3), []);
        assert_eq!(t.frame(1.4), [Out::MineStart]);
        assert_eq!(t.frame(1.5), []);
        assert_eq!(t.end(2), [Out::MineEnd]);
    }

    #[test]
    fn a_dragging_finger_turns_the_view_and_does_not_tap() {
        let l = layout(633.0, 292.0);
        let mut t = Touches::default();
        t.start(1, (300.0, 100.0), 0.0, &l);
        // Within the slop (a thumb's wobble): no look yet, and still a tap candidate.
        assert_eq!(t.moved(1, (305.0, 100.0), &l), []);
        assert_eq!(t.moved(1, (313.0, 104.0), &l), [Out::Look(8.0, 4.0)]);
        assert_eq!(t.frame(5.0), []);
        assert_eq!(t.end(1), []);
    }

    #[test]
    fn two_fingers_move_and_look_at_once() {
        let l = layout(633.0, 292.0);
        let mut t = Touches::default();
        t.start(1, at(&l, Control::Forward), 0.0, &l);
        t.start(2, (400.0, 100.0), 0.0, &l);
        assert_eq!(t.moved(2, (420.0, 100.0), &l), [Out::Look(20.0, 0.0)]);
        assert!(t.held(Control::Forward));
        let mut all = t.release_all();
        all.sort_by_key(|o| format!("{o:?}"));
        assert_eq!(all, [Out::Release(Control::Forward)]);
    }
}
