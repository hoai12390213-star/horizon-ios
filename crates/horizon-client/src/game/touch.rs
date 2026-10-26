//! Touch screens (phones and tablets; Horizon's own): Pocket Edition's controls while playing, and a finger
//! as the mouse over every screen. The layout and the fingers' state are the lib's `touch`; this file acts on
//! what they say the way the keyboard and mouse would: the D-pad and buttons are their default keys, a tap is
//! a click, holding still is a held left button, and a drag turns the view as the mouse does.
use super::{Game, Screen};
use horizon_client::{
    Result,
    hud::{HudBatch, rgba},
    touch::{self, Control, ControlStyle, Out, RideKind, Touches},
};
use std::{collections::BTreeSet, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{
        DeviceId, ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
    },
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
};

/// GUI pixels a finger drags over a menu for one line of scrolling.
const SCROLL_LINE: f32 = 12.0;
const TOUCH_KEY_CHOICES: [(&str, &str); 12] = [
    ("W", "key.keyboard.w"),
    ("A", "key.keyboard.a"),
    ("S", "key.keyboard.s"),
    ("D", "key.keyboard.d"),
    ("Space", "key.keyboard.space"),
    ("Shift", "key.keyboard.left.shift"),
    ("Ctrl", "key.keyboard.left.control"),
    ("E", "key.keyboard.e"),
    ("Q", "key.keyboard.q"),
    ("F", "key.keyboard.f"),
    ("T", "key.keyboard.t"),
    ("R", "key.keyboard.r"),
];

#[derive(Default)]
pub(super) struct TouchInput {
    fingers: Touches,
    /// A touch has been seen: the controls are drawn (from the start on iPhone and iPad).
    seen: bool,
    /// The finger that is the pointer over a menu.
    menu: Option<MenuFinger>,
    /// Clicks to release once a tick has passed: vanilla samples buttons once a tick, so a tap released in
    /// the same frame would be lost (the pad holds its short presses back the same way).
    late: Vec<(MouseButton, u64)>,
    /// The button a finger held still on the world is holding down: attack to mine, or use to eat, drink or
    /// draw a bow.
    holding: Option<MouseButton>,
    /// Active virtual joystick vector; `Some([0, 0])` still owns movement input.
    movement: Option<[f32; 2]>,
    joystick_sprinting: bool,
    editor: Option<LayoutFinger>,
    selected_control: Option<Control>,
    menu_layer_fingers: BTreeSet<u64>,
    edge_swipe: Option<(u64, (f32, f32))>,
    drawer_open: bool,
    drawer_press: Option<(u64, DrawerAction)>,
    key_picker: Option<KeyPickerMode>,
    key_capture: Option<KeyPickerMode>,
    clock: Option<Instant>,
}

#[derive(Clone, Copy)]
enum KeyPickerMode {
    Add,
    Rebind(Control),
}

#[derive(Clone, Copy)]
enum DrawerAction {
    Close,
    ToggleEdit,
    ToggleMenuLayer,
    AddKey,
    Rebind,
    DeleteSelected,
    CaptureKey,
    ChooseKey(&'static str),
}

struct LayoutFinger {
    id: u64,
    control: Control,
    at: (f32, f32),
}

/// A finger over a menu. Its press waits for a frame drawn with the pointer under it, and its release for a
/// frame after the press: screens find what is under the pointer while drawing, so a pointer that moved and
/// pressed in one go would press whatever was under the old position (or nothing).
struct MenuFinger {
    id: u64,
    /// Where the scrolling last caught up with the finger (GUI pixels).
    scrolled: (f32, f32),
    /// Loop iterations since it went down.
    frames: u8,
    pressed: bool,
    lifted: bool,
    /// It went down on the close button, which already acted: the rest of it is ignored.
    closed: bool,
}

/// The key a button stands for: the vanilla default of its mapping.
fn key_of(control: Control) -> Option<KeyCode> {
    Some(match control {
        Control::Forward => KeyCode::KeyW,
        Control::Back => KeyCode::KeyS,
        Control::Left => KeyCode::KeyA,
        Control::Right => KeyCode::KeyD,
        Control::Jump => KeyCode::Space,
        Control::Ascend => KeyCode::Space,
        Control::Descend => KeyCode::ShiftLeft,
        Control::Dismount => KeyCode::ShiftLeft,
        Control::Sprint => KeyCode::ControlLeft,
        Control::Perspective => KeyCode::F5,
        Control::Chat => KeyCode::KeyT,
        Control::Pause => KeyCode::Escape,
        Control::Inventory => KeyCode::KeyE,
        Control::Sneak
        | Control::Attack
        | Control::Use
        | Control::Joystick
        | Control::Hotbar(_)
        | Control::Custom(_) => return None,
    })
}

impl Game<'_> {
    fn touch_batch(&self) -> Option<HudBatch> {
        self.renderer
            .as_ref()
            .map(|r| HudBatch::new([r.config.width, r.config.height]))
    }

    fn touch_now(&mut self) -> f64 {
        self.touch
            .clock
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f64()
    }

    /// The world is under the fingers, with no screen over it.
    fn touch_playing(&self) -> bool {
        self.title.is_none()
            && self.screen == Screen::Playing
            && self.sign_edit.is_none()
            && self.book.is_none()
            && self.player.is_some()
    }

    fn touch_vertical(&self) -> bool {
        self.player
            .as_ref()
            .is_some_and(|player| player.flying || player.fluids.was_touching_water)
    }

    fn touch_layout(&self, width: f32, height: f32) -> Vec<(Control, touch::Rect)> {
        let ride = self.riding.map_or(RideKind::None, |id| {
            if self.boats.iter().any(|boat| boat.id == id) {
                RideKind::Boat
            } else if self.carts.iter().any(|cart| cart.id == id) {
                RideKind::Minecart
            } else {
                RideKind::Mount
            }
        });
        let mut layout = touch::layout_configured(
            width,
            height,
            self.settings.touch.button_scale,
            self.touch_vertical(),
            self.settings.touch.move_method,
            self.settings.touch.control_style,
            ride,
            self.settings.touch.sprint_button,
        );
        let button = touch::button_size(height) * self.settings.touch.button_scale;
        for id in self.settings.touch.custom_keys.keys() {
            let column = id % 2;
            let row = id / 2;
            let x = width - 2.0 * button - 24.0 + f32::from(column) * (button + 6.0);
            let y = height * 0.22 + f32::from(row) * (button + 6.0);
            layout.push((Control::Custom(*id), [x, y, button, button]));
        }
        for (control, rect) in &mut layout {
            if let Some(offset) = self.settings.touch.positions.get(&control.layout_key()) {
                rect[0] = (rect[0] + offset[0] * width).clamp(0.0, (width - rect[2]).max(0.0));
                rect[1] = (rect[1] + offset[1] * height).clamp(0.0, (height - rect[3]).max(0.0));
            }
        }
        layout
    }

    fn touch_menu_layer_visible(&self) -> bool {
        self.settings.touch.menu_layer
            && self.title.is_none()
            && self.screen == Screen::Options
            && self.player.is_some()
    }

    fn touch_controls_visible(&self) -> bool {
        self.touch_playing() || self.touch_menu_layer_visible()
    }

    fn touch_drawer_bounds(width: f32, height: f32) -> touch::Rect {
        let panel = (width * 0.48).clamp(196.0, 270.0).min(width);
        [width - panel, 0.0, panel, height]
    }

    fn touch_drawer_action_rect(index: usize, width: f32, height: f32) -> touch::Rect {
        let panel = Self::touch_drawer_bounds(width, height);
        let gap = 4.0;
        let cell_w = (panel[2] - 20.0 - gap) / 2.0;
        [
            panel[0] + 8.0 + (index % 2) as f32 * (cell_w + gap),
            38.0 + (index / 2) as f32 * 28.0,
            cell_w,
            24.0,
        ]
    }

    fn touch_drawer_key_rect(index: usize, width: f32, height: f32) -> touch::Rect {
        let panel = Self::touch_drawer_bounds(width, height);
        let gap = 4.0;
        let cell_w = (panel[2] - 20.0 - 2.0 * gap) / 3.0;
        [
            panel[0] + 8.0 + (index % 3) as f32 * (cell_w + gap),
            137.0 + (index / 3) as f32 * 28.0,
            cell_w,
            24.0,
        ]
    }

    fn touch_drawer_hit(&self, at: (f32, f32), width: f32, height: f32) -> Option<DrawerAction> {
        let panel = Self::touch_drawer_bounds(width, height);
        if touch::hit_rect([panel[0] + panel[2] - 38.0, 6.0, 30.0, 26.0], at) {
            return Some(DrawerAction::Close);
        }
        let actions = [
            DrawerAction::ToggleEdit,
            DrawerAction::ToggleMenuLayer,
            DrawerAction::Rebind,
            DrawerAction::AddKey,
            DrawerAction::DeleteSelected,
            DrawerAction::CaptureKey,
        ];
        if let Some((index, _)) = actions
            .iter()
            .enumerate()
            .find(|(i, _)| touch::hit_rect(Self::touch_drawer_action_rect(*i, width, height), at))
        {
            return Some(actions[index]);
        }
        if self.touch.key_picker.is_some()
            && let Some((index, _)) = TOUCH_KEY_CHOICES
                .iter()
                .enumerate()
                .find(|(i, _)| touch::hit_rect(Self::touch_drawer_key_rect(*i, width, height), at))
        {
            return Some(DrawerAction::ChooseKey(TOUCH_KEY_CHOICES[index].1));
        }
        None
    }

    fn touch_drawer_event(
        &mut self,
        id: u64,
        phase: TouchPhase,
        at: (f32, f32),
        width: f32,
        height: f32,
    ) -> Result<()> {
        match phase {
            TouchPhase::Started => {
                self.touch.drawer_press = self
                    .touch_drawer_hit(at, width, height)
                    .map(|action| (id, action));
                if self.touch.drawer_press.is_none()
                    && at.0 < Self::touch_drawer_bounds(width, height)[0]
                {
                    self.touch.drawer_open = false;
                    self.touch.key_picker = None;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some((finger, action)) = self.touch.drawer_press.take()
                    && finger == id
                    && phase == TouchPhase::Ended
                {
                    self.touch_drawer_action(action);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn touch_drawer_action(&mut self, action: DrawerAction) {
        match action {
            DrawerAction::Close => {
                self.touch.drawer_open = false;
                self.touch.key_picker = None;
            }
            DrawerAction::ToggleEdit => {
                self.settings.touch.edit_layout = !self.settings.touch.edit_layout;
                if !self.settings.touch.edit_layout {
                    self.touch.editor = None;
                }
            }
            DrawerAction::ToggleMenuLayer => {
                self.settings.touch.menu_layer = !self.settings.touch.menu_layer;
            }
            DrawerAction::AddKey => self.touch.key_picker = Some(KeyPickerMode::Add),
            DrawerAction::Rebind => {
                if let Some(control) = self.touch.selected_control {
                    self.touch.key_picker = Some(KeyPickerMode::Rebind(control));
                }
            }
            DrawerAction::DeleteSelected => {
                if let Some(Control::Custom(id)) = self.touch.selected_control.take() {
                    let control = Control::Custom(id).layout_key();
                    self.settings.touch.custom_keys.remove(&id);
                    self.settings.touch.positions.remove(&control);
                    self.settings.touch.bindings.remove(&control);
                }
            }
            DrawerAction::CaptureKey => {
                self.touch.key_capture = self.touch.key_picker;
            }
            DrawerAction::ChooseKey(name) => self.touch_assign_key(name),
        }
        self.settings.save(&Game::options_path());
    }

    fn touch_assign_key(&mut self, name: &str) {
        if horizon_client::keybinds::key_code(name).is_none() {
            return;
        }
        let Some(mode) = self.touch.key_picker.take() else {
            return;
        };
        match mode {
            KeyPickerMode::Add => {
                if let Some(id) =
                    (0..=u8::MAX).find(|id| !self.settings.touch.custom_keys.contains_key(id))
                {
                    self.settings.touch.custom_keys.insert(id, name.to_owned());
                    self.touch.selected_control = Some(Control::Custom(id));
                    self.settings.touch.edit_layout = true;
                }
            }
            KeyPickerMode::Rebind(Control::Custom(id)) => {
                self.settings.touch.custom_keys.insert(id, name.to_owned());
            }
            KeyPickerMode::Rebind(control) => {
                self.settings
                    .touch
                    .bindings
                    .insert(control.layout_key(), name.to_owned());
            }
        }
    }

    pub(super) fn capture_touch_key(&mut self, key: KeyCode) -> bool {
        let Some(mode) = self.touch.key_capture.take() else {
            return false;
        };
        if let Some(name) = horizon_client::keybinds::key_name(key) {
            self.touch.key_picker = Some(mode);
            self.touch_assign_key(name);
            self.settings.save(&Game::options_path());
        }
        true
    }

    pub(super) fn touch_move(&self) -> Option<[f32; 2]> {
        self.touch.movement
    }

    fn touch_key(&self, control: Control) -> Option<KeyCode> {
        if let Some(name) = self.settings.touch.bindings.get(&control.layout_key()) {
            return horizon_client::keybinds::key_code(name);
        }
        if let Control::Custom(id) = control {
            return self
                .settings
                .touch
                .custom_keys
                .get(&id)
                .and_then(|name| horizon_client::keybinds::key_code(name));
        }
        let mapping = match control {
            Control::Forward => Some("key.forward"),
            Control::Back => Some("key.back"),
            Control::Left => Some("key.left"),
            Control::Right => Some("key.right"),
            Control::Jump | Control::Ascend => Some("key.jump"),
            Control::Sneak | Control::Descend | Control::Dismount => Some("key.sneak"),
            Control::Sprint => Some("key.sprint"),
            Control::Inventory => Some("key.inventory"),
            Control::Chat => Some("key.chat"),
            Control::Perspective => Some("key.togglePerspective"),
            _ => None,
        };
        if let Some(mapping) = mapping {
            return horizon_client::keybinds::key_code(self.settings.keys.get(mapping));
        }
        key_of(control)
    }

    fn touch_key_event(&mut self, key: KeyCode, pressed: bool) -> Result<()> {
        match self.settings.keys.translate(key) {
            Some(Ok(mapped)) => self.mapped_key(mapped, pressed, false),
            Some(Err(button)) if self.touch_playing() => self.play_mouse(button, pressed),
            Some(Err(_)) | None => Ok(()),
        }
    }

    /// The controls are on screen: after a touch, from the start on iPhone and iPad, or with `HORIZON_TOUCH=1`
    /// (laying them out on a computer).
    fn touch_shown(&self) -> bool {
        self.touch.seen
            || cfg!(target_os = "ios")
            || std::env::var_os("HORIZON_TOUCH").is_some_and(|v| v == "1")
    }

    /// winit's `WindowEvent::Touch`.
    pub(super) fn touch_event(&mut self, event_loop: &ActiveEventLoop, touch: Touch) -> Result<()> {
        self.touch.seen = true;
        self.last_input = Instant::now();
        let Some(batch) = self.touch_batch() else {
            return Ok(());
        };
        let at = (
            touch.location.x as f32 / batch.scale,
            touch.location.y as f32 / batch.scale,
        );
        if self.touch.drawer_open {
            return self.touch_drawer_event(touch.id, touch.phase, at, batch.width, batch.height);
        }
        if let Some((_, start)) = self.touch.edge_swipe.filter(|(id, _)| *id == touch.id) {
            if touch.phase == TouchPhase::Moved
                && start.0 - at.0 > 64.0
                && (start.1 - at.1).abs() < 100.0
            {
                self.touch.edge_swipe = None;
                self.touch.drawer_open = true;
                return Ok(());
            }
            if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.touch.edge_swipe = None;
                return Ok(());
            }
            return Ok(());
        }
        if touch.phase == TouchPhase::Started && at.0 >= batch.width - 18.0 && self.player.is_some()
        {
            self.touch.edge_swipe = Some((touch.id, at));
            return Ok(());
        }
        let playing = self.touch_playing();
        let menu_layer = self.touch_menu_layer_visible();
        if self.touch.menu.is_some() || (!playing && !menu_layer) {
            self.touch_menu(event_loop, touch.id, touch.phase, at);
            return Ok(());
        }
        let layout = self.touch_layout(batch.width, batch.height);
        if self.settings.touch.edit_layout {
            self.touch_edit_event(
                touch.id,
                touch.phase,
                at,
                &layout,
                batch.width,
                batch.height,
            );
            return Ok(());
        }
        if menu_layer && !playing {
            let captured = self.touch.menu_layer_fingers.contains(&touch.id);
            if touch.phase == TouchPhase::Started && touch::hit(&layout, at).is_some() {
                self.touch.menu_layer_fingers.insert(touch.id);
            } else if !captured {
                self.touch_menu(event_loop, touch.id, touch.phase, at);
                return Ok(());
            }
        }
        let cancelled = touch.phase == TouchPhase::Cancelled;
        let out = match touch.phase {
            TouchPhase::Started => {
                let now = self.touch_now();
                self.touch.fingers.start(touch.id, at, now, &layout)
            }
            TouchPhase::Moved => self.touch.fingers.moved(touch.id, at, &layout),
            TouchPhase::Ended | TouchPhase::Cancelled => self.touch.fingers.end(touch.id),
        };
        let out = if cancelled {
            out.into_iter()
                .filter(|action| *action != Out::Tap)
                .collect()
        } else {
            out
        };
        let result = self.touch_act(out, batch.scale);
        if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.touch.menu_layer_fingers.remove(&touch.id);
        }
        result
    }

    fn touch_edit_event(
        &mut self,
        id: u64,
        phase: TouchPhase,
        at: (f32, f32),
        layout: &[(Control, touch::Rect)],
        width: f32,
        height: f32,
    ) {
        match phase {
            TouchPhase::Started if self.touch.editor.is_none() => {
                if let Some(control) = touch::hit(layout, at) {
                    self.touch.selected_control = Some(control);
                    self.touch.editor = Some(LayoutFinger { id, control, at });
                }
            }
            TouchPhase::Moved => {
                if let Some(finger) = self.touch.editor.as_mut().filter(|f| f.id == id) {
                    let dx = (at.0 - finger.at.0) / width.max(1.0);
                    let dy = (at.1 - finger.at.1) / height.max(1.0);
                    let offset = self
                        .settings
                        .touch
                        .positions
                        .entry(finger.control.layout_key())
                        .or_insert([0.0, 0.0]);
                    offset[0] = (offset[0] + dx).clamp(-0.8, 0.8);
                    offset[1] = (offset[1] + dy).clamp(-0.8, 0.8);
                    finger.at = at;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled
                if self.touch.editor.as_ref().is_some_and(|f| f.id == id) =>
            {
                self.touch.editor = None;
                self.settings.save(&Game::options_path());
            }
            _ => {}
        }
    }

    /// Once a frame: a held finger starts mining, held-back clicks release, and a screen that opened over the
    /// game lets go of every finger on it.
    pub(super) fn touch_frame(&mut self, event_loop: &ActiveEventLoop) {
        if !self.touch_shown() {
            return;
        }
        self.touch_menu_frame(event_loop);
        let playing = self.touch_playing() || self.touch_menu_layer_visible();
        let out = if playing {
            let now = self.touch_now();
            self.touch.fingers.frame(now)
        } else {
            self.touch.fingers.release_all()
        };
        let ticks = self.ticks;
        let (due, keep) = std::mem::take(&mut self.touch.late)
            .into_iter()
            .partition(|(_, at)| ticks > *at || !playing);
        self.touch.late = keep;
        let scale = self.touch_batch().map_or(1.0, |b| b.scale);
        let result = due
            .into_iter()
            .try_for_each(|(button, _)| self.play_mouse(button, false))
            .and_then(|()| self.touch_act(out, scale));
        if let Err(e) = result {
            log::warn!("Horizon: touch: {e}");
        }
    }

    /// Mouse look and clicks only count with the pointer grabbed, which a touch screen has no pointer for:
    /// grab it as a click into the window would, or act as an agent's pointer where it cannot be grabbed.
    fn touch_grab(&mut self) {
        // Only when neither holds: a grab that fails (iOS has no pointer to grab) would otherwise be retried
        // on every drag, and each try makes the mouse drop its next move (`grabMouse`'s ignoreFirstMove).
        if self.captured || self.agent_pointer {
            return;
        }
        self.capture(true);
        if !self.captured {
            self.agent_pointer = true;
        }
    }

    /// The held item is used for as long as the button stays down: something eaten or drunk (its
    /// `consumable` component), or a bow, crossbow, trident, shield, spyglass, goat horn or brush.
    fn touch_holds_use(&self) -> bool {
        let Some(stack) = self.held() else {
            return false;
        };
        matches!(
            &*stack.item,
            "minecraft:bow"
                | "minecraft:crossbow"
                | "minecraft:trident"
                | "minecraft:shield"
                | "minecraft:spyglass"
                | "minecraft:goat_horn"
                | "minecraft:brush"
        ) || self
            .res
            .items()
            .get(&stack.item)
            .is_some_and(|info| info.consumable.is_some())
    }

    fn touch_act(&mut self, out: Vec<Out>, scale: f32) -> Result<()> {
        for o in out {
            match o {
                Out::Press(Control::Hotbar(slot)) => self.selected = usize::from(slot),
                Out::Press(Control::Sneak) => {
                    // A toggle either way: the crouch option toggles a press, otherwise flip the held key
                    // (as a controller's stick click does).
                    let key = self.touch_key(Control::Sneak).unwrap_or(KeyCode::ShiftLeft);
                    if key == KeyCode::ShiftLeft && self.settings.toggle_crouch {
                        self.touch_key_event(key, true)?;
                        self.touch_key_event(key, false)?;
                    } else {
                        let sneaking = self.keys.contains(&key);
                        self.touch_key_event(key, !sneaking)?;
                    }
                }
                Out::Press(Control::Attack) => {
                    self.touch_grab();
                    self.play_mouse(MouseButton::Left, true)?;
                }
                Out::Release(Control::Attack) => self.play_mouse(MouseButton::Left, false)?,
                Out::Press(Control::Use) => {
                    self.touch_grab();
                    self.play_mouse(MouseButton::Right, true)?;
                }
                Out::Release(Control::Use) => self.play_mouse(MouseButton::Right, false)?,
                Out::Move(vector) => {
                    self.touch.movement = Some(vector);
                    if self.settings.touch.joystick_sprint {
                        let sprinting = vector[1] < -0.82;
                        if sprinting != self.touch.joystick_sprinting {
                            self.mapped_key(KeyCode::ControlLeft, sprinting, false)?;
                            self.touch.joystick_sprinting = sprinting;
                        }
                    }
                }
                Out::MoveEnd => {
                    self.touch.movement = None;
                    if self.touch.joystick_sprinting {
                        self.mapped_key(KeyCode::ControlLeft, false, false)?;
                        self.touch.joystick_sprinting = false;
                    }
                }
                Out::Press(control) | Out::Release(control) => {
                    if let Some(key) = self.touch_key(control) {
                        self.touch_key_event(key, matches!(o, Out::Press(_)))?;
                    }
                }
                Out::Look(dx, dy) => {
                    self.touch_grab();
                    let sensitivity = self.settings.touch.look_sensitivity;
                    let (dx, dy) = (
                        f64::from(dx * scale * sensitivity),
                        f64::from(dy * scale * sensitivity),
                    );
                    // Relative motion, as an agent's `mouse-move` adds it: no pointer position to compare with.
                    self.mouse.add_motion(dx, dy);
                }
                // Pocket Edition: a tap hits the entity it is on, and uses the item or block otherwise.
                Out::Tap => {
                    if self.settings.touch.control_style == ControlStyle::SplitControls {
                        continue;
                    }
                    self.touch_grab();
                    let button = if self.entity_targeted() {
                        MouseButton::Left
                    } else {
                        MouseButton::Right
                    };
                    self.play_mouse(button, true)?;
                    self.touch.late.push((button, self.ticks));
                }
                // Holding still: what the hand holds decides, as in Pocket Edition. Food, potions and the items
                // used for as long as they are held (a bow, a shield...) are used until the finger lifts;
                // anything else mines.
                Out::MineStart => {
                    if self.settings.touch.control_style == ControlStyle::SplitControls {
                        continue;
                    }
                    self.touch_grab();
                    let button = if self.touch_holds_use() {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    };
                    self.touch.holding = Some(button);
                    self.play_mouse(button, true)?;
                }
                Out::MineEnd => {
                    if self.settings.touch.control_style == ControlStyle::SplitControls {
                        continue;
                    }
                    let button = self.touch.holding.take().unwrap_or(MouseButton::Left);
                    self.play_mouse(button, false)?;
                }
            }
        }
        Ok(())
    }

    // ---- menus: a finger is the mouse ----

    /// The ✕ over a screen with no Done or Back of its own: an inventory, a container, creative, chat, the
    /// statistics and advancements, a sign or a book. The title screens and the options have theirs.
    fn touch_close_shown(&self) -> bool {
        self.touch_shown()
            && self.title.is_none()
            && (self.sign_edit.is_some()
                || self.book.is_some()
                || !matches!(
                    self.screen,
                    Screen::Playing | Screen::Paused | Screen::Dead | Screen::Win | Screen::Options
                ))
    }

    fn touch_menu(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: u64,
        phase: TouchPhase,
        at: (f32, f32),
    ) {
        match phase {
            TouchPhase::Started => {
                if self.touch.menu.is_some() {
                    return;
                }
                let on_close = self.touch_close_shown()
                    && self
                        .touch_batch()
                        .is_some_and(|b| touch::hit_rect(touch::close_rect(b.width, b.height), at));
                let on_keyboard = !on_close
                    && self.touch_shown()
                    && self.field_focused()
                    && self.touch_batch().is_some_and(|b| {
                        touch::hit_rect(touch::keyboard_rect(b.width, b.height), at)
                    });
                if on_close {
                    // What Escape does where the finger is (the pad's B).
                    if let Err(e) = self.pad_back() {
                        log::warn!("Horizon: touch close: {e}");
                    }
                } else if on_keyboard {
                    self.show_system_keyboard();
                } else {
                    self.touch_pointer(event_loop, at);
                }
                self.touch.menu = Some(MenuFinger {
                    id,
                    scrolled: at,
                    frames: 0,
                    pressed: false,
                    lifted: false,
                    closed: on_close || on_keyboard,
                });
            }
            TouchPhase::Moved => {
                let Some(finger) = self.touch.menu.as_ref().filter(|f| f.id == id && !f.closed)
                else {
                    return;
                };
                let last = finger.scrolled;
                self.touch_pointer(event_loop, at);
                // A drag scrolls lists as a wheel would, the content following the finger. Not in an
                // inventory, where a drag carries the item it pressed on.
                let lines = ((at.1 - last.1) / SCROLL_LINE).trunc();
                let scrolls = self.screen != Screen::Inventory;
                if let Some(f) = self.touch.menu.as_mut() {
                    f.scrolled = (at.0, last.1 + lines * SCROLL_LINE);
                }
                if scrolls && lines != 0.0 {
                    self.touch_wheel(event_loop, lines);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(finger) = self.touch.menu.as_mut().filter(|f| f.id == id) else {
                    return;
                };
                finger.lifted = true;
                if !finger.closed {
                    self.touch_pointer(event_loop, at);
                }
            }
        }
    }

    /// Once a loop iteration: the menu finger's press, a frame after it went down, and its release, a frame
    /// after the press once it has lifted.
    fn touch_menu_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(finger) = self.touch.menu.as_mut() else {
            return;
        };
        finger.frames = finger.frames.saturating_add(1);
        if finger.closed {
            if finger.lifted {
                self.touch.menu = None;
            }
        } else if !finger.pressed {
            if finger.frames >= 2 {
                finger.pressed = true;
                self.touch_button(event_loop, true);
            }
        } else if finger.lifted {
            self.touch.menu = None;
            self.touch_button(event_loop, false);
        }
    }

    fn touch_pointer(&mut self, event_loop: &ActiveEventLoop, at: (f32, f32)) {
        let (Some(id), Some(batch)) = (self.window.as_ref().map(|w| w.id()), self.touch_batch())
        else {
            return;
        };
        let position =
            PhysicalPosition::new(f64::from(at.0 * batch.scale), f64::from(at.1 * batch.scale));
        self.window_event(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position,
            },
        );
    }

    fn touch_button(&mut self, event_loop: &ActiveEventLoop, pressed: bool) {
        let Some(id) = self.window.as_ref().map(|w| w.id()) else {
            return;
        };
        let state = if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        };
        self.window_event(
            event_loop,
            id,
            WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button: MouseButton::Left,
            },
        );
    }

    fn touch_wheel(&mut self, event_loop: &ActiveEventLoop, lines: f32) {
        let Some(id) = self.window.as_ref().map(|w| w.id()) else {
            return;
        };
        self.window_event(
            event_loop,
            id,
            WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, lines),
                phase: TouchPhase::Moved,
            },
        );
    }

    // ---- drawing ----

    /// The buttons over the HUD while playing: vanilla's button, a little see-through as Pocket Edition's
    /// were, lit while held, with white marks shadowed like Minecraft's text.
    pub(super) fn draw_touch_controls(&self, batch: &mut HudBatch) {
        if !self.touch_shown() || (!self.touch_controls_visible() && !self.touch.drawer_open) {
            return;
        }
        let atlas = &self.gui.atlas;
        if self.touch_controls_visible() {
            for (control, r) in self.touch_layout(batch.width, batch.height) {
                if matches!(control, Control::Hotbar(_)) {
                    continue;
                }
                if control == Control::Joystick {
                    touch_button(
                        batch,
                        atlas,
                        r,
                        self.touch.movement.is_some(),
                        self.settings.touch.button_opacity,
                    );
                    let vector = self.touch.movement.unwrap_or([0.0, 0.0]);
                    let knob = r[2] * 0.42;
                    let travel = (r[2] - knob) / 2.0;
                    let knob_rect = [
                        r[0] + travel + -vector[0] * travel,
                        r[1] + travel + -vector[1] * travel,
                        knob,
                        knob,
                    ];
                    touch_button(
                        batch,
                        atlas,
                        knob_rect,
                        true,
                        self.settings.touch.button_opacity,
                    );
                    continue;
                }
                touch_button(
                    batch,
                    atlas,
                    r,
                    self.touch.fingers.held(control)
                        || (self.settings.touch.edit_layout
                            && self.touch.selected_control == Some(control)),
                    self.settings.touch.button_opacity,
                );
                if let Control::Custom(id) = control {
                    if let Some(key) = self.settings.touch.custom_keys.get(&id) {
                        let label = horizon_client::keybinds::display_name(key).plain();
                        let text_width = HudBatch::text_width(atlas, &label);
                        batch.text(
                            atlas,
                            &label,
                            r[0] + (r[2] - text_width) / 2.0,
                            r[1] + (r[3] - 9.0) / 2.0,
                            rgba(255, 255, 255, 240),
                            true,
                        );
                    }
                } else {
                    shadowed(r, self.settings.touch.button_opacity, |r, color| {
                        glyph(batch, atlas, control, r, color)
                    });
                }
            }
        }
        if self.touch.drawer_open {
            self.draw_touch_drawer(batch);
        }
    }

    fn draw_touch_drawer(&self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let panel = Self::touch_drawer_bounds(batch.width, batch.height);
        batch.fill(
            atlas,
            panel[0],
            panel[1],
            panel[2],
            panel[3],
            rgba(12, 12, 12, 238),
        );
        batch.text(
            atlas,
            "Touch controls",
            panel[0] + 10.0,
            12.0,
            u32::MAX,
            true,
        );
        let close = [panel[0] + panel[2] - 38.0, 6.0, 30.0, 26.0];
        touch_button(batch, atlas, close, false, 0.8);
        batch.text(atlas, "X", close[0] + 11.0, close[1] + 8.0, u32::MAX, true);
        let labels = [
            if self.settings.touch.edit_layout {
                "Edit: ON"
            } else {
                "Edit: OFF"
            },
            if self.settings.touch.menu_layer {
                "Menu layer: ON"
            } else {
                "Menu layer: OFF"
            },
            if self.touch.selected_control.is_some() {
                "Rebind selected"
            } else {
                "Select a key"
            },
            "Add key button",
            "Delete selected",
            "Use keyboard key",
        ];
        for (index, label) in labels.iter().enumerate() {
            let rect = Self::touch_drawer_action_rect(index, batch.width, batch.height);
            touch_button(batch, atlas, rect, false, 0.8);
            let width = HudBatch::text_width(atlas, label);
            batch.text(
                atlas,
                label,
                rect[0] + (rect[2] - width) / 2.0,
                rect[1] + 7.0,
                u32::MAX,
                true,
            );
        }
        if self.touch.key_picker.is_some() {
            batch.text(atlas, "Choose a key", panel[0] + 8.0, 123.0, u32::MAX, true);
            for (index, (label, _)) in TOUCH_KEY_CHOICES.iter().enumerate() {
                let rect = Self::touch_drawer_key_rect(index, batch.width, batch.height);
                touch_button(batch, atlas, rect, false, 0.8);
                let width = HudBatch::text_width(atlas, label);
                batch.text(
                    atlas,
                    label,
                    rect[0] + (rect[2] - width) / 2.0,
                    rect[1] + 7.0,
                    u32::MAX,
                    true,
                );
            }
        }
    }
}

impl Game<'_> {
    /// The buttons over a screen: ✕ where the screen has no way out of its own on a touch screen
    /// (`touch_close_shown`), and ⌨ beside it while a text field has focus.
    pub(super) fn draw_touch_close(&mut self, batch: &mut HudBatch) {
        let held = self.touch.menu.as_ref().is_some_and(|f| f.closed);
        let keyboard = self.touch_shown() && self.field_focused();
        let atlas = &self.gui.atlas;
        if self.touch_close_shown() {
            let r = touch::close_rect(batch.width, batch.height);
            touch_button(batch, atlas, r, held, self.settings.touch.button_opacity);
            shadowed(r, self.settings.touch.button_opacity, |r, color| {
                // Two diagonals of cells.
                let u = cell_size(r);
                let (cx, cy) = ((r[0] + r[2] / 2.0).floor(), (r[1] + r[3] / 2.0).floor());
                for i in -3..=3 {
                    let d = i as f32 * u;
                    batch.fill(atlas, cx + d - u / 2.0, cy + d - u / 2.0, u, u, color);
                    batch.fill(atlas, cx - d - u / 2.0, cy + d - u / 2.0, u, u, color);
                }
            });
        }
        if keyboard {
            let r = touch::keyboard_rect(batch.width, batch.height);
            touch_button(batch, atlas, r, false, self.settings.touch.button_opacity);
            shadowed(r, self.settings.touch.button_opacity, |r, color| {
                // A keyboard: two rows of keys over a space bar.
                let u = cell_size(r) * 0.75;
                let (cx, cy) = ((r[0] + r[2] / 2.0).floor(), (r[1] + r[3] / 2.0).floor());
                for row in 0..2 {
                    for key in 0..5 {
                        let x = cx + (key as f32 - 2.5) * u * 1.6;
                        let y = cy + (row as f32 - 1.5) * u * 1.6;
                        batch.fill(atlas, x, y, u, u, color);
                    }
                }
                batch.fill(atlas, cx - 2.5 * u, cy + 1.7 * u, 5.0 * u, u, color);
            });
        }
    }
}

/// Vanilla's button sprite, nine-sliced to the touch button's size: highlighted while held, and see-through
/// enough to keep the world behind it in sight.
fn touch_button(
    batch: &mut HudBatch,
    atlas: &horizon_client::hud::HudAtlas,
    r: touch::Rect,
    held: bool,
    opacity: f32,
) {
    let sprite = if held {
        "gui/sprites/widget/button_highlighted"
    } else {
        "gui/sprites/widget/button"
    };
    let alpha = ((if held {
        230.0
    } else {
        255.0 * opacity.clamp(0.2, 1.0)
    }) as u8)
        .max(1);
    horizon_client::ui::nine_slice(
        batch,
        atlas,
        sprite,
        r[0],
        r[1],
        r[2],
        r[3],
        rgba(255, 255, 255, alpha),
    );
}

/// The size of a mark's cell on a button `r`: two GUI pixels on the old 26-pixel buttons, growing with them.
fn cell_size(r: touch::Rect) -> f32 {
    (r[2] / 13.0).round().max(2.0)
}

/// Draw a mark twice, as Minecraft draws text: a dark copy a little down and right, then the white one.
fn shadowed(r: touch::Rect, opacity: f32, mut mark: impl FnMut(touch::Rect, u32)) {
    let off = (cell_size(r) / 2.0).max(1.0);
    let opacity = opacity.clamp(0.2, 1.0);
    mark(
        [r[0] + off, r[1] + off, r[2], r[3]],
        rgba(63, 63, 63, (230.0 * opacity) as u8),
    );
    mark(r, rgba(255, 255, 255, (240.0 * opacity) as u8));
}

/// A button's mark, in cells of a few pixels centred on it.
fn glyph(
    batch: &mut HudBatch,
    atlas: &horizon_client::hud::HudAtlas,
    control: Control,
    r: touch::Rect,
    color: u32,
) {
    let (cx, cy) = ((r[0] + r[2] / 2.0).floor(), (r[1] + r[3] / 2.0).floor());
    let u = cell_size(r);
    let cell = |batch: &mut HudBatch, x: f32, y: f32, w: f32, h: f32| {
        batch.fill(atlas, cx + x * u, cy + y * u, w * u, h * u, color);
    };
    // A triangle pointing up, down, left or right: rows (or columns) narrowing to its tip.
    let arrow = |batch: &mut HudBatch, dir: (f32, f32)| {
        for i in 0..4 {
            let i = i as f32;
            let len = 7.0 - 2.0 * i;
            let at = 1.5 - i;
            match dir {
                (0.0, d) => cell(batch, -len / 2.0, at * -d - 0.5, len, 1.0),
                (d, _) => cell(batch, at * -d - 0.5, -len / 2.0, 1.0, len),
            }
        }
    };
    match control {
        Control::Forward => arrow(batch, (0.0, -1.0)),
        Control::Back => arrow(batch, (0.0, 1.0)),
        Control::Left => arrow(batch, (-1.0, 0.0)),
        Control::Right => arrow(batch, (1.0, 0.0)),
        Control::Ascend => arrow(batch, (0.0, -1.0)),
        Control::Descend => arrow(batch, (0.0, 1.0)),
        Control::Dismount => arrow(batch, (0.0, 1.0)),
        Control::Sprint => {
            arrow(batch, (0.0, -1.0));
            cell(batch, -2.0, 2.5, 4.0, 1.0);
        }
        Control::Attack => {
            cell(batch, -2.5, -2.5, 5.0, 5.0);
            cell(batch, -4.0, -0.5, 8.0, 1.0);
            cell(batch, -0.5, -4.0, 1.0, 8.0);
        }
        Control::Use => {
            cell(batch, -2.5, -2.5, 5.0, 1.0);
            cell(batch, -2.5, 1.5, 5.0, 1.0);
            cell(batch, -2.5, -1.5, 1.0, 3.0);
            cell(batch, 1.5, -1.5, 1.0, 3.0);
        }
        Control::Joystick => cell(batch, -2.0, -2.0, 4.0, 4.0),
        // Sneak: a hollow diamond; jump: a filled one.
        Control::Sneak | Control::Jump => {
            for i in 0..4 {
                let i = i as f32;
                let len = 7.0 - 2.0 * i;
                for y in [-i - 0.5, i - 0.5] {
                    if control == Control::Jump {
                        cell(batch, -len / 2.0, y, len, 1.0);
                    } else {
                        cell(batch, -len / 2.0, y, 1.0, 1.0);
                        cell(batch, len / 2.0 - 1.0, y, 1.0, 1.0);
                    }
                }
            }
        }
        // Perspective: an eye.
        Control::Perspective => {
            cell(batch, -2.5, -1.5, 5.0, 1.0);
            cell(batch, -3.5, -0.5, 1.0, 1.0);
            cell(batch, -0.5, -0.5, 1.0, 1.0);
            cell(batch, 2.5, -0.5, 1.0, 1.0);
            cell(batch, -2.5, 0.5, 5.0, 1.0);
        }
        // Chat: a speech bubble with its tail.
        Control::Chat => {
            cell(batch, -3.0, -2.5, 6.0, 4.0);
            cell(batch, -2.0, 1.5, 1.0, 1.0);
        }
        // Pause: two bars.
        Control::Pause => {
            cell(batch, -2.0, -2.5, 1.5, 5.0);
            cell(batch, 0.5, -2.5, 1.5, 5.0);
        }
        // The inventory: three dots, as Pocket Edition's button beside the hotbar.
        Control::Inventory => {
            for x in [-3.0, -0.5, 2.0] {
                cell(batch, x, -0.5, 1.0, 1.0);
            }
        }
        Control::Hotbar(_) | Control::Custom(_) => {}
    }
}
